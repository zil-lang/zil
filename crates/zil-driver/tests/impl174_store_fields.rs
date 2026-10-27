//! Store fields retain their initialized prefixes through real inline Package ownership.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
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
        "store-fields174-{}-{}",
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
        let output = Command::new(&executable).output().unwrap();
        fs::write(dir.join("stdout"), &output.stdout).unwrap();
        fs::write(dir.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
    }
}

fn candidate(source: &str) -> (zil_ir::TypedModule, zil_ir::CandidatePlan) {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw174.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let module = zil_types::check(&core, &target, &limits).unwrap();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits).unwrap();
    (module, plan)
}

fn rejected_candidate(
    label: &str,
    module: zil_ir::TypedModule,
    mut plan: zil_ir::CandidatePlan,
    category: ResultCategory,
) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let dir = directory();
    fs::write(dir.join(format!("{label}.bin")), &bytes).unwrap();
    for (path, errors) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits).unwrap_err(),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits).unwrap_err(),
        ),
    ] {
        fs::write(
            dir.join(format!("{label}-{path}.txt")),
            format!("{errors:#?}\n"),
        )
        .unwrap();
        assert!(
            errors.iter().all(|d| d.phase == zil_base::Phase::Verify),
            "{label}: {errors:?}"
        );
        assert!(
            errors.iter().any(|d| d.category == category),
            "{label}: {errors:?}"
        );
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R { code:i32 }
impl Drop for R { fn drop(self:Self) { unsafe { putchar(.code) } } }
package Item { manages item:R; n:i32 }
package State { manages tag:R; manages items:Store<Item>; n:i32 }
fn item(n:i32)->Item => Item { item:R{code:n}, n:n }
fn empty()->Store<Item> contract{result(create)} { return store_new() }
fn state(tag:i32,a:i32,b:i32)->State {
    let store=empty(); store_reserve(store,2);
    store_push(store,item(a)); store_push(store,item(b));
    return State {tag:R{code:tag},items:give store,n:4}
}
fn relay(state:State)->State => state
"#;

#[test]
fn complete_store_fields_construct_move_and_drop_prefix_in_field_order() {
    native(&[("main", &format!("{MODEL} fn main()->i32{{let s=relay(state(90,65,66));return i32(store_len(s.items))-2}}"))], b"ZAB", 0);
}

#[test]
fn whole_field_take_refill_and_state_transfer_have_one_responsibility() {
    native(&[("main", &format!("{MODEL} fn main()->i32{{var p=state(90,65,66);let items=give p.items;let next=state(89,67,68);p.items=give next.items;let restored=relay(p);return i32(store_len(restored.items))-2}}"))], b"ZCDYAB", 0);
}

#[test]
fn replacing_store_field_drops_old_prefix_before_new_publication() {
    native(&[("main", &format!("{MODEL} fn main()->i32{{var p=state(90,65,66);let next=state(89,67,68);p.items=give next.items;return i32(store_len(p.items))-2}}"))], b"ABYZCD", 0);
}

#[test]
fn taken_store_field_early_error_cleans_only_live_shell_fields() {
    native(&[("main", &format!("{MODEL} fn main()->void!i32{{let p=state(90,65,66);let items=give p.items;return ()}}"))], b"ABZ", 1);
}

#[test]
fn store_state_in_owned_error_and_fixed_array_recursively_cleans_prefix() {
    native(&[("main", &format!("{MODEL} fn fail()->State!i32{{return state(90,65,66)}} fn main()->State!i32{{fail()?;return 0}}"))], b"ZAB", 1);
    native(&[("main", &format!("{MODEL} fn main()->i32{{let states=[state(90,65,66),state(89,67,68)];return 0}}"))], b"ZABYCD", 0);
}

#[test]
fn sealed_definitions_are_discovered_from_state_only_signatures() {
    for signature in ["State", "[State;2]", "State!i32"] {
        let source = format!(r#"
            resource R{{code:i32}}
            package Item{{manages item:R;manages data:ByteBuffer;n:i32}}
            package State{{manages items:Store<Item>}}
            fn sink(value:{signature})->i32=>0
            fn main()->i32=>0
        "#);
        native(&[("main", &source)], b"", 0);
        let (module, _) = candidate(&source);
        assert!(module.resources.iter().any(|r|matches!(r.payload,zil_ir::Type::OwnedStore{..})));
        assert!(module.resources.iter().any(|r|r.payload==zil_ir::Type::ByteBuffer));
    }
}
