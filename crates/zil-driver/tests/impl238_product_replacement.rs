//! Author source-to-machine coverage for stable ordinary Product replacement.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
use zil_ir::{InstructionKind as I, TypedModule};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const HEAD: &str = r#"extern "C" fn putchar(code:i32)->i32;fn emit(code:i32){unsafe{putchar(code)}}resource Mark{code:i32}impl Drop for Mark{fn drop(self:Self){emit(.code)}}"#;
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "defer238-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn typed(text: &str) -> TypedModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "defer238.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let path = directory(label);
        fs::write(path.join("source.zil"), source).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "defer238.zil", source),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(path.join("program")),
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(path.join("stdout"), &output.stdout).unwrap();
        fs::write(path.join("stderr"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}
const ORIGINAL: &str = r###"extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Mark{code:i32}
impl Drop for Mark{fn drop(self:Self){emit(.code)}}
package Owner{manages marker:Mark}
fn relay(owned:Owner)->Owner{return owned}
fn main()->i32{
var current=Owner{marker:Mark{code:65}}
defer{emit(81)}
let late=Mark{code:76}
current=relay(current)
return 0
}
"###;

fn reject(label: &str, source: &str, category: ResultCategory) {
    let path = directory(label);
    fs::write(path.join("source.zil"), source).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "reject238.zil", source),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
}
#[test]
fn original_relay_retains_shell_age_and_unchanged_cleanup_oracle() {
    native("original-relay", ORIGINAL, "LQA", 0);
    native(
        "original-without-defer",
        &ORIGINAL.replace("defer{emit(81)}", ""),
        "LA",
        0,
    );
    let module = typed(ORIGINAL);
    let function = module.functions.iter().find(|f| f.name == "main").unwrap();
    let local = function
        .locals
        .iter()
        .find(|l| l.name == "current")
        .unwrap()
        .id;
    assert_eq!(
        function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter(|i| matches!(i.kind,I::Store{local:l,..}if l==local))
            .count(),
        1
    );
    assert!(
        !function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind,I::TakeLocal{local:l}if l==local))
    );
    assert!(
        function
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind,I::TakePackageField{local:l,..}if l==local))
    );
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default()).unwrap();
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
}
#[test]
fn independent_rhs_prepares_then_cleans_declared_fields_and_publishes_copy() {
    let source = format!(
        "{HEAD}package Owner{{manages first:Mark;manages second:Mark;tag:i32}}fn make()->Owner{{return Owner{{first:Mark{{code:67}},second:Mark{{code:68}},tag:2}}}}fn main()->i32{{var current=Owner{{first:Mark{{code:65}},second:Mark{{code:66}},tag:1}};defer{{emit(81)}}let late=Mark{{code:76}};current=make();if current.tag!=2{{return 9}}return 0}}"
    );
    native("independent", &source, "ABLQCD", 0);
    let partial = source.replace(
        "defer{emit(81)}",
        "let removed=give current.first;defer{emit(81)}",
    );
    native("partial", &partial, "BLQACD", 0);
    let prepared = source.replace("current=make();", "let prepared=make();current=prepared;");
    native("prepared", &prepared, "ABLQCD", 0);
}
#[test]
fn rhs_error_keeps_old_value_and_self_argument_failure_keeps_actual_stage() {
    let head = format!(
        "{HEAD}package Owner{{manages marker:Mark;tag:i32}}fn fail()->void!i32{{return ()}}fn maker(failure:bool)->void!Owner{{if failure{{return ()}}return Owner{{marker:Mark{{code:66}},tag:2}}}}fn relay_before(n:i32,owned:Owner)->Owner{{return owned}}fn relay_after(owned:Owner,n:i32)->Owner{{return owned}}"
    );
    for (label, rhs, output, exit) in [
        ("independent-error", "maker(true)?", "LQA", 1),
        ("independent-success", "maker(false)?", "ALQB", 0),
        ("argument-before", "relay_before(fail()?,current)", "LQA", 1),
        ("argument-after", "relay_after(current,fail()?)", "ALQ", 1),
    ] {
        let source = format!(
            "{head}fn main()->void!i32{{var current=Owner{{marker:Mark{{code:65}},tag:1}};defer{{emit(81)}}let late=Mark{{code:76}};current={rhs};return 0}}"
        );
        native(label, &source, output, exit);
    }
}
#[test]
fn self_call_keeps_each_actual_binding_and_copy_read_available_after_commit() {
    let head = format!(
        "{HEAD}package Owner{{manages marker:Mark;tag:i32}}fn relay(owned:Owner,n:i32)->Owner{{return owned}}"
    );
    let source = format!(
        "{head}fn main()->i32{{var current=Owner{{marker:Mark{{code:65}},tag:1}};defer{{emit(81)}}let late=Mark{{code:76}};current=relay(current,(()=>{{let current=3;return current}})());if current.tag!=1{{return 8}}return 0}}"
    );
    native("shadow", &source, "LQA", 0);
}
#[test]
fn ordinary_whole_move_copy_reads_live_windows_and_complex_self_remain_closed() {
    let head = format!(
        "{HEAD}package Owner{{manages marker:Mark;tag:i32}}fn make()->Owner{{return Owner{{marker:Mark{{code:66}},tag:2}}}}fn relay(owned:Owner,n:i32)->Owner{{return owned}}"
    );
    for (label, body, category) in [
        (
            "moved-copy",
            "let moved=current;let read=current.tag;",
            ResultCategory::SafeReject,
        ),
        (
            "moved-republish",
            "let moved=current;current=make();",
            ResultCategory::SafeReject,
        ),
        (
            "copy-loan",
            "let held=&current.tag;current=make();let read=*held;",
            ResultCategory::SafeReject,
        ),
        (
            "whole-loan",
            "let held=&current;current=make();let read=held.tag;",
            ResultCategory::SafeReject,
        ),
        (
            "callback-loan",
            "let peek=()=>current.tag;current=relay(current,peek());",
            ResultCategory::SafeReject,
        ),
        (
            "self-copy",
            "current=relay(current,current.tag);",
            ResultCategory::Unsupported,
        ),
        (
            "complex-self",
            "current=relay(if true{current}else{make()},0);",
            ResultCategory::Unsupported,
        ),
    ] {
        let source = format!(
            "{head}fn main()->i32{{var current=Owner{{marker:Mark{{code:65}},tag:1}};{body}return 0}}"
        );
        reject(label, &source, category);
    }
}
#[test]
fn replacement_types_budget_is_paid_before_field_growth() {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget238.zil", ORIGINAL);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let defaults = Limits::default();
    let target = TargetProfile::host_llvm();
    let expected = zil_types::check(&core, &target, &defaults).unwrap();
    let run = |n| match zil_types::check(
        &core,
        &target,
        &Limits {
            max_analysis_steps: n,
            ..defaults.clone()
        },
    ) {
        Ok(actual) => {
            assert_eq!(actual, expected);
            true
        }
        Err(errors) => {
            assert!(!errors.is_empty());
            assert!(
                errors
                    .iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{errors:?}"
            );
            false
        }
    };
    let (mut lo, mut hi) = (0, defaults.max_analysis_steps);
    assert!(run(hi));
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if run(mid) { hi = mid } else { lo = mid }
    }
    assert!(run(hi));
    assert!(!run(hi - 1));
    assert!(run(hi));
    let path = directory("types-budget");
    fs::write(
        path.join("boundary.json"),
        format!(
            "{{\"types\":{hi},\"n\":true,\"n_minus_one_resource_limit\":true,\"fresh_n\":true}}\n"
        ),
    )
    .unwrap();
}
