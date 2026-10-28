//! A Store result mode selects the success payload, never the whole union.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "impl186-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, source))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *source,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect(),
    }
}
fn check(label: &str, source: &str) -> CompileReport {
    let dir = directory(label);
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_project(
        &project(&[("main", source)]),
        &CompileOptions {
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}
fn accepted(label: &str, source: &str) {
    let report = check(label, source);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
fn rejected(label: &str, source: &str, category: ResultCategory, message: &str) {
    let report = check(label, source);
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
    assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message)),
        "{label}: {}",
        report.to_json()
    );
}
fn native(label: &str, files: &[(&str, &str)], stdout: &[u8], exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(label);
        for (name, source) in files {
            fs::write(dir.join(format!("{name}.zil")), source).unwrap();
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
        let output = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout.log"), &output.stdout).unwrap();
        fs::write(dir.join("stderr.log"), &output.stderr).unwrap();
        fs::write(
            dir.join("native.json"),
            format!(
                "{{\"optimization\":\"{optimization:?}\",\"exit_code\":{},\"expected_exit\":{exit},\"stdout_hex\":\"{}\",\"expected_stdout_hex\":\"{}\"}}\n",
                output.status.code().map_or("null".into(), |code| code.to_string()),
                output.stdout.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                stdout.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            ),
        ).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{output:?}");
        assert_eq!(output.stdout, stdout);
        assert!(output.stderr.is_empty(), "{output:?}");
    }
}

const MODEL: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource R{code:i32}impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package P{manages r:R;pub n:i32}
package Q{lead:u8;manages r:R;pub wide:i64}
enum E{Fail}
fn p(code:i32)->P=>P{r:R{code:code},n:7}
fn q(code:i32)->Q=>Q{lead:u8(3),r:R{code:code},wide:123456789}
"#;
const HELPERS: &str = r#"
fn empty<T:StoreElement>()->Store<T> contract{result(create)}=>store_new()
fn make<T:StoreElement>(value:T,fail:bool)->E!Store<T> contract{result(create)}{
 let s:Store<T>=empty();if fail{return E.Fail};
 if !store_reserve(s,1){return E.Fail};store_push(s,value);return give s
}
fn forward<T:StoreElement>(give x:E!Store<T>)->E!Store<T> contract{result(give)}=>x
fn wrap<T:StoreElement>(give x:Store<T>)->E!Store<T> contract{result(give)}=>give x
"#;

#[test]
fn written_modes_are_checked_with_and_without_instantiation() {
    for (definition, call) in [
        (
            "fn good<T:StoreElement>()->E!Store<T>contract{result(create)}=>store_new()",
            "good::<P>()",
        ),
        (
            "fn good<T:StoreElement>()->E!Store<T>contract{result(create)}=>empty()",
            "good::<P>()",
        ),
        (
            "fn good<T:StoreElement>()->E!Store<T>contract{result(create)}=>E.Fail",
            "good::<P>()",
        ),
        (
            "fn good<T:StoreElement>(v:T)->E!Store<T>contract{result(create)}{let a=make(v,false);let b=a;return b}",
            "good(p(65))",
        ),
        (
            "fn good<T:StoreElement>(v:T)->E!Store<T>contract{result(create)}{let a=make(v,false);let b=a?;return give b}",
            "good(p(65))",
        ),
        (
            "fn good<T:StoreElement>(give a:E!Store<T>)->E!Store<T>contract{result(give)}{let b=a;return forward(b)}",
            "good(make(p(65),false))",
        ),
        (
            "fn good<T:StoreElement>(give a:E!Store<T>)->E!Store<T>contract{result(create)}=>E.Fail",
            "good(make(p(65),false))",
        ),
        (
            "fn good<T:StoreElement>(give a:E!Store<T>)->E!Store<T>contract{result(create)}{let s=a?;return E.Fail}",
            "good(make(p(65),false))",
        ),
    ] {
        for used in [false, true] {
            let main = if used {
                format!("fn main(){{let result={call}}}")
            } else {
                "fn main(){}".into()
            };
            accepted(
                "written-modes",
                &format!("{MODEL}{HELPERS}{definition}\n{main}"),
            );
        }
    }
}

#[test]
fn two_nominal_shapes_transfer_through_ordinary_union_values() {
    let source = format!(
        r#"{MODEL}{HELPERS}
fn main()->E!i32{{
 let a=forward(make(p(65),false));let a2=a;let first=a2?;
 let second=forward(make(q(66),false))?;
 let left=store_at(first,0);let right=store_at(second,0);
 if left.n!=7||right.wide!=123456789{{return E.Fail}};
 return i32(store_len(first)+store_len(second))-2
}}"#
    );
    native("two-shapes", &[("main", &source)], b"BA", 0);
}

#[test]
fn propagated_failure_drops_incoming_element_and_earlier_store() {
    for (failure, output, exit) in [(false, b"BA".as_slice(), 0), (true, b"BA".as_slice(), 1)] {
        let source = format!(
            r#"{MODEL}{HELPERS}
fn main()->E!i32{{let first=make(p(65),false)?;let second=make(q(66),{failure})?;return 0}}"#
        );
        native("question-failure", &[("main", &source)], output, exit);
    }
}

#[test]
fn fresh_payload_survives_local_union_moves_and_question_rewrapping() {
    for failure in [false, true] {
        let source = format!(
            r#"{MODEL}{HELPERS}
fn fresh<T:StoreElement>(v:T,fail:bool)->E!Store<T>contract{{result(create)}}{{
 let first=make(v,fail);let second=first;let s=second?;return give s
}}
fn main()->E!i32{{let s=fresh(p(65),{failure})?;return 0}}"#
        );
        native(
            "fresh-question",
            &[("main", &source)],
            b"A",
            i32::from(failure),
        );
    }
}

#[test]
fn error_only_create_consumes_old_success_without_claiming_a_new_resource() {
    let source = format!(
        r#"{MODEL}{HELPERS}
fn discard<T:StoreElement>(give old:E!Store<T>)->E!Store<T>contract{{result(create)}}{{return E.Fail}}
fn main()->E!i32{{let lost=discard(make(p(65),false))?;return 0}}"#
    );
    native("zero-success", &[("main", &source)], b"A", 1);
}

#[test]
fn loop_local_and_discarded_unions_clean_exactly_once() {
    let source = format!(
        r#"{MODEL}{HELPERS}
fn repeat<T:StoreElement>(count:i32)->E!Store<T>contract{{result(create)}}{{
 var i=0;while i<count{{let temporary:Store<T>=empty();let u=wrap(give temporary);i+=1}};
 return empty()
}}
fn main(){{make(p(65),false);make(p(66),true);let empty=repeat::<Q>(3)}}"#
    );
    native("discard-loop", &[("main", &source)], b"AB", 0);
}

#[test]
fn unused_definitions_cannot_launder_old_success_into_create() {
    for definition in [
        "fn bad<T:StoreElement>(give s:Store<T>)->E!Store<T>contract{result(create)}=>give s",
        "fn bad<T:StoreElement>(give u:E!Store<T>)->E!Store<T>contract{result(create)}=>u",
        "fn bad<T:StoreElement>(give u:E!Store<T>)->E!Store<T>contract{result(create)}{let v=u;return v}",
        "fn bad<T:StoreElement>(give u:E!Store<T>)->E!Store<T>contract{result(create)}{let s=u?;return give s}",
        "fn bad<T:StoreElement>(give u:E!Store<T>)->E!Store<T>contract{result(create)}=>forward(u)",
        "fn bad<T:StoreElement>(v:T)->E!Store<T>contract{result(create)}{let u=forward(make(v,false));return u}",
        "fn bad<T:StoreElement>(give s:Store<T>)->E!Store<T>contract{result(create)}=>wrap(give s)",
        "fn bad<T:StoreElement>(give u:E!Store<T>,flag:bool)->E!Store<T>contract{result(create)}{if flag{return E.Fail}return u}",
        "fn bad<T:StoreElement>(give old:E!Store<T>,v:T,flag:bool)->E!Store<T>contract{result(create)}{let u=if flag{old}else{make(v,false)};return u}",
        "fn bad<T:StoreElement>(give old:Store<T>,flag:bool)->E!Store<T>contract{result(create)}{var s:Store<T>=empty();if flag{s=give old}return give s}",
    ] {
        rejected(
            "old-create",
            &format!("{MODEL}{HELPERS}{definition}\nfn main(){{}}"),
            ResultCategory::SafeReject,
            "fresh source",
        );
    }
}

#[test]
fn unused_union_moves_and_descriptor_aliases_do_not_create_permissions() {
    for (definition, category, message) in [
        (
            "fn bad<T:StoreElement>(give u:E!Store<T>){let a=u;let b=u}",
            ResultCategory::SafeReject,
            "used after move",
        ),
        (
            "fn bad<T:StoreElement>(give u:E!Store<T>){forward(u);forward(u)}",
            ResultCategory::SafeReject,
            "used after move",
        ),
        (
            "fn bad<T:StoreElement>(give u:E!Store<T>)->E!Store<T>contract{result(give)}=>give u",
            ResultCategory::Unsupported,
            "generic managed give",
        ),
        (
            "fn bad<T:StoreElement>(s:Store<T>)->E!Store<T>contract{result(give)}=>s",
            ResultCategory::SafeReject,
            "not a descriptor",
        ),
        (
            "fn bad<T:StoreElement>(give s:Store<T>)->E!Store<T>contract{result(give)}{let alias=s;return give alias}",
            ResultCategory::SafeReject,
            "descriptors do not own",
        ),
        (
            "fn bad<T:StoreElement>(give u:E!Store<T>)->E!Store<T>contract{result(give)}{let s=u?;let alias=s;return give alias}",
            ResultCategory::SafeReject,
            "descriptors do not own",
        ),
        (
            "fn bad<T:StoreElement>(give u:E!Store<T>){var v=u;v=forward(v)}",
            ResultCategory::Unsupported,
            "replacement is not implemented",
        ),
        (
            "fn bad<T:StoreElement>(give u:E!Store<T>){let v:E!Store<T>=u}",
            ResultCategory::Unsupported,
            "producer-inferred success mode",
        ),
        (
            "fn bad<T:StoreElement>(give u:E!Store<T>,flag:bool){while flag{forward(u)}}",
            ResultCategory::Unsupported,
            "loop must preserve",
        ),
    ] {
        rejected(
            "move-category",
            &format!("{MODEL}{HELPERS}{definition}\nfn main(){{}}"),
            category,
            message,
        );
    }
}

#[test]
fn unused_signatures_require_the_written_mode_and_actual_copy_error_shape() {
    for (definition, category, message) in [
        (
            "fn bad<T:StoreElement>()->E!Store<T> =>store_new()",
            ResultCategory::Unsupported,
            "requires result(create/give)",
        ),
        (
            "fn bad<T:StoreElement>(u:E!Store<T>){}",
            ResultCategory::Unsupported,
            "explicit give payload mode",
        ),
        (
            "fn bad<T>()->E!Store<T>contract{result(create)}=>store_new()",
            ResultCategory::SafeReject,
            "StoreElement",
        ),
        (
            "fn bad<T:StoreElement>()->T!Store<T>contract{result(create)}=>store_new()",
            ResultCategory::SafeReject,
            "finite Copy",
        ),
        (
            "fn bad<T:StoreElement>()->P!Store<T>contract{result(create)}=>store_new()",
            ResultCategory::SafeReject,
            "finite Copy",
        ),
        (
            "fn bad<T:StoreElement>()->R!Store<T>contract{result(create)}=>store_new()",
            ResultCategory::SafeReject,
            "finite Copy",
        ),
        (
            "fn bad<T:StoreElement>()->&i32!Store<T>contract{result(create)}=>store_new()",
            ResultCategory::SafeReject,
            "addresses",
        ),
    ] {
        rejected(
            "shape-mode",
            &format!("{MODEL}{HELPERS}{definition}\nfn main(){{}}"),
            category,
            message,
        );
    }
}

#[test]
fn direct_store_and_existing_copy_error_inference_keep_their_meaning() {
    let source = format!(
        r#"{MODEL}{HELPERS}
fn identity<T>(x:T)->T=>x
fn scalar()->E!i32=>identity(7)
fn from_direct<T:StoreElement>()->E!Store<T>contract{{result(create)}}=>empty()
fn from_union<T:StoreElement>(v:T)->E!Store<T>contract{{result(create)}}=>make(v,false)
fn main()->E!i32{{let a=from_direct::<P>()?;let b=from_union(q(66))?;return scalar()?-7}}
"#
    );
    native("inference", &[("main", &source)], b"B", 0);
}

#[test]
fn a_later_failing_argument_cleans_the_staged_generic_union() {
    let source = format!(
        r#"{MODEL}{HELPERS}
fn fail()->E!i32=>E.Fail
fn consume<T:StoreElement>(give pending:E!Store<T>,n:i32)->i32=>n
fn main()->E!i32{{let guard=R{{code:90}};return consume(make(p(65),false),fail()?)}}
"#
    );
    native("late-argument", &[("main", &source)], b"AZ", 1);
}

#[test]
fn concrete_wide_copy_errors_keep_zero_success_responsibility() {
    for fail in [false, true] {
        let source = format!(
            r#"{MODEL}{HELPERS}
enum WideError{{Bad(i64,u8)}}
fn create<T:StoreElement>(fail:bool)->WideError!Store<T>contract{{result(create)}}{{
 if fail{{return WideError.Bad(123456789,u8(7))}};return store_new()
}}
fn main()->WideError!i32{{let pending=create::<Q>({fail});let store=pending?;return 0}}
"#
        );
        native("copy-error", &[("main", &source)], b"", i32::from(fail));
    }
}
