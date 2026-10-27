//! Concrete managed-field transfer inside ordinary owning State templates.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}
fn directory() -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!(
        "state-composition177-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}
fn check(files: &[(&str, &str)]) -> CompileReport {
    let dir = directory();
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    let report = compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory();
        for (name, text) in files {
            fs::write(dir.join(format!("{name}.zil")), text).unwrap();
        }
        let executable = dir.join("program");
        let report = compile_project(
            &project(files),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(executable.clone()),
                emit_dir: Some(dir.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(dir.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let out = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout"), &out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(exit), "{out:?}");
        assert_eq!(out.stdout, stdout);
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32} impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Item{manages r:R}
package Batch{manages items:Store<Item>}
package State<F>{manages r:R;manages items:Store<Item>;callback:F;hits:i32}
fn batch()->Batch{let items:Store<Item>=store_new();store_reserve(items,1);store_push(items,Item{r:R{code:65}});return Batch{items:give items}}
fn consume(b:Batch)->i32=>0
fn make()->impl FnMut()->i32{var n=0;return move()=>{n+=1;return n}}
fn pack<F:FnMut()->i32>(callback:F)->State<F>{let b=batch();return State::<F>{r:R{code:90},items:give b.items,callback:callback,hits:0}}
"#;

#[test]
fn generic_state_takes_refills_and_calls_actual_callback_after_store_transfer() {
    let source = format!(
        "{MODEL} fn update<G:FnMut()->i32>(state:State<G>)->State<G>{{var next=state;let old=give next.items;let b=batch();next.items=give b.items;let cb=&mut next.callback;next.hits=cb();return next}} fn main()->i32{{let state=update(pack(make()));return state.hits-1}}"
    );
    native(&[("main", &source)], b"AZA", 0);
}

#[test]
fn unused_templates_reject_missing_fields_and_concrete_batch_aliases() {
    for (body, reason) in [
        (
            "let a=give state.items;let b=give state.items;return state",
            "managed Package field used after give",
        ),
        (
            "let a=give state.items;return state",
            "partial callable State must be refilled",
        ),
        (
            "let a=give state.items;let old=state.items;return state",
            "managed Package field used after give",
        ),
        (
            "let b=batch();let a=give b.items;consume(b);return state",
            "partial callable State must be refilled",
        ),
        (
            "let b=batch();let alias=b;let a=give b.items;return state",
            "generic place used after move",
        ),
    ] {
        let source = format!(
            "{MODEL} fn unused<F:FnMut()->i32>(state:State<F>)->State<F>{{{body}}} fn main()->i32=>0"
        );
        let report = check(&[("main", &source)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{body}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == zil_base::Phase::Expand
                    && d.code == "E-GENERIC-ARGUMENT"
                    && d.message.contains(reason)),
            "{body}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
}

#[test]
fn give_does_not_grant_ownership_to_copy_callable_or_descriptor_values() {
    for (body, category, code, reason) in [
        (
            "let a=give state.hits;return state",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
            "declared manages field",
        ),
        (
            "let a=give state.callback;return state",
            ResultCategory::SafeReject,
            "E-GENERIC-ARGUMENT",
            "declared manages field",
        ),
        (
            "let d=R{code:65};let a=give d;return state",
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
            "direct Package manages field",
        ),
    ] {
        let source = format!(
            "{MODEL} fn unused<F:FnMut()->i32>(state:State<F>)->State<F>{{{body}}} fn main()->i32=>0"
        );
        let report = check(&[("main", &source)]);
        assert_eq!(report.status, category, "{body}: {}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == zil_base::Phase::Expand
                    && d.code == code
                    && d.message.contains(reason)),
            "{body}: {}",
            report.to_json()
        );
    }
}

#[test]
fn direct_store_builtin_in_parametric_body_remains_a_separate_closed_protocol() {
    let source = format!(
        "{MODEL} fn unused<F:FnMut()->i32>(state:State<F>)->State<F>{{let s:Store<Item>=store_new();return state}} fn main()->i32=>0"
    );
    let report = check(&[("main", &source)]);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == zil_base::Phase::Expand
                && d.code == "E-GENERIC-ARGUMENT"
                && d.message.contains("unknown function in parametric body")),
        "{}",
        report.to_json()
    );
}

#[test]
fn borrowed_concrete_package_cannot_be_copied_into_a_local_give_capability() {
    let source = format!(
        "{MODEL} fn unused<F:FnMut()->i32>(callback:F,b:&Batch)->State<F>{{let alias=*b;return State::<F>{{r:R{{code:90}},items:give alias.items,callback:callback,hits:0}}}} fn main()->i32=>0"
    );
    let report = check(&[("main", &source)]);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == zil_base::Phase::Expand
                && d.code == "E-UNSUPPORTED"
                && d.message.contains("requires a declared bound")),
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn generic_comparison_result_hint_does_not_retype_a_concrete_helper_operand() {
    let prefix =
        format!("{MODEL} fn count(items:Store<Item>)->i32{{return i32(store_len(items))}}");
    let source = format!(
        "{prefix} fn inspect<F:FnMut()->i32>(state:State<F>)->i32{{if count(state.items)!=1{{return 3}} return state.hits}} fn main()->i32{{return inspect(pack(make()))}}"
    );
    native(&[("main", &source)], b"ZA", 0);
    let source = format!(
        "{prefix} fn unused<F:FnMut()->i32>(state:State<F>)->i32{{if count(state.items)!=true{{return 3}} return 0}} fn main()->i32=>0"
    );
    let report = check(&[("main", &source)]);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == zil_base::Phase::Expand
                && d.code == "E-GENERIC-ARGUMENT"
                && d.message.contains("generic body types must agree")),
        "{}",
        report.to_json()
    );
}
