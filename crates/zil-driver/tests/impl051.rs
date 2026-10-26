//! Source resource/closure interactions; native traces test actual cleanup.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, Stage, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-051-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn source_report(text: &str, stage: Stage) -> CompileReport {
    compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "closures.zil", text),
        &CompileOptions {
            stop_after: stage,
            ..CompileOptions::default()
        },
    )
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
}
fn native(text: &str, expected: &str) -> (String, String) {
    let temp = Temp::new();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "closures.zil", text),
        &CompileOptions {
            stop_after: Stage::Build,
            output: Some(temp.0.join("plain")),
            emit_dir: Some(temp.0.join("ir")),
            ..CompileOptions::default()
        },
    );
    success(&report);
    for optimized in [false, true] {
        let binary = if optimized {
            let path = temp.0.join("optimized");
            let output = Command::new("clang")
                .args(["-x", "ir", "-O2"])
                .arg(&report.artifacts["llvm"])
                .arg("-o")
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            path
        } else {
            report.artifacts["executable"].clone()
        };
        let output = Command::new(binary).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(output.stderr.is_empty());
    }
    let typed = fs::read_to_string(&report.artifacts["typed"]).unwrap();
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(
        !llvm.contains("@malloc"),
        "concrete environment introduced implicit allocation"
    );
    (typed, llvm)
}

#[test]
fn closure051_01_resource_and_unrelated_copy_closure_coexist_native() {
    native(include_str!("../../../examples/impl051/coexist.zil"), "A");
}

#[test]
fn closure051_02_resource_context_keeps_i64_array_literals() {
    let text = "resource Block{cells:[i64;2]} fn main()->i32{let block=Block{cells:[3,4]};let identity=(n:i64)=>n;if identity(block.cells[1])==4{return 0}return 1}";
    success(&source_report(text, Stage::Verify));
}

#[test]
fn closure051_03_descriptor_reads_each_call_and_target_write_is_shared_native() {
    native(
        include_str!("../../../examples/impl051/descriptor.zil"),
        "C",
    );
}

#[test]
fn closure051_04_owned_shared_can_repeat_and_environment_transfer_is_unique_native() {
    let (typed, _) = native(
        include_str!("../../../examples/impl051/owned_shared.zil"),
        "A",
    );
    assert!(typed.contains("CallClosureShared"));
}

#[test]
fn closure051_05_owned_mutation_through_descriptor_alias_is_exclusive_native() {
    native(
        include_str!("../../../examples/impl051/owned_exclusive.zil"),
        "C",
    );
}

#[test]
fn closure051_06_body_give_owns_at_creation_and_conditional_consume_cleans_rest_native() {
    let (typed, _) = native(include_str!("../../../examples/impl051/consume.zil"), "ABC");
    assert!(typed.contains("TakeCapture"));
    assert!(typed.contains("CallClosureOnce"));
}

#[test]
fn closure051_07_uncalled_owned_environments_clean_in_capture_order_native() {
    native(include_str!("../../../examples/impl051/unused.zil"), "ABC");
}

#[test]
fn closure051_08_early_return_loop_exits_and_temporary_environments_cleanup_native() {
    native(
        include_str!("../../../examples/impl051/control.zil"),
        "BACDEF",
    );
}

const PRELUDE: &str = "resource Token{code:i32} fn consume(give token:Token){} ";

#[test]
fn closure051_09_owned_write_requires_mutable_closure_position() {
    let text = format!(
        "{PRELUDE} fn main()->i32{{let token=Token{{code:65}};let f=move ()=>{{token.code+=1}};f();return 0}}"
    );
    let report = source_report(&text, Stage::Types);
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
            .any(|d| d.code == "E-CLOSURE-MUTABILITY"),
        "{}",
        report.to_json()
    );
}

#[test]
fn closure051_10_descriptor_capture_cannot_acquire_give_authority() {
    let text = format!(
        "{PRELUDE} fn main()->i32{{let token=Token{{code:65}};let alias=token;let f=()=>consume(give alias);return 0}}"
    );
    let report = source_report(&text, Stage::Types);
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
            .any(|d| d.code == "E-GIVE-DESCRIPTOR"),
        "{}",
        report.to_json()
    );
}

#[test]
fn closure051_11_source_move_and_capture_liveness_are_independently_rejected() {
    let bodies = [
        "let token=Token{code:65};let f=move ()=>token.code;consume(give token);return 0",
        "let token=Token{code:65};let f=()=>consume(give token);f();f();return 0",
        "let token=Token{code:65};let f=()=>token.code;consume(give token);return f()",
        "let token=Token{code:65};let alias=token;let f=()=>consume(give token);return alias.code",
        "let token=Token{code:65};let alias=token;let f=move ()=>token.code+alias.code;return f()",
        "let token=Token{code:65};let alias=token;let f=move ()=>alias.code+token.code;return f()",
        "let token=Token{code:65};let alias=token;let f=move ()=>alias.code;consume(give token);return f()",
        "let token=Token{code:65};let f=move ()=>token.code;let moved=f;return f()",
    ];
    for body in bodies {
        let text = format!("{PRELUDE} fn main()->i32{{{body}}}");
        success(&source_report(&text, Stage::Types));
        let report = source_report(&text, Stage::Verify);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn closure051_12_pure_helper_stays_shared_and_recursive_write_helper_is_exclusive_native() {
    native(include_str!("../../../examples/impl051/effects.zil"), "AC");
}

const OBSERVABLE: &str = "extern \"C\" fn putchar(code:i32)->i32; resource Token{code:i32} impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}} fn consume(give token:Token){} ";

#[test]
fn closure051_13_mixed_address_and_owner_capture_ends_loan_after_once_native() {
    let text = format!(
        "{OBSERVABLE} fn main()->i32{{var n=7;let token=Token{{code:65}};let once=()=>{{consume(give token);n}};let observed=once();n=8;if observed==7 && n==8{{return 0}}return 1}}"
    );
    native(&text, "A");
}

#[test]
fn closure051_14_owned_environment_moves_through_both_conditional_edges_native() {
    let text = format!(
        "{OBSERVABLE} fn choose(flag:bool,code:i32)->i32{{let token=Token{{code}};let read=move ()=>token.code;let picked=if flag{{read}}else{{read}};if picked()!=code{{return 1}}return 0}} fn main()->i32{{if choose(true,65)!=0{{return 1}}if choose(false,66)!=0{{return 2}}return 0}}"
    );
    native(&text, "AB");
}

#[test]
fn closure051_15_shared_owned_entry_cleans_its_fresh_resource_each_call_native() {
    let text = format!(
        "{OBSERVABLE} fn main()->i32{{let token=Token{{code:65}};let read=move ()=>{{let fresh=Token{{code:66}};token.code+fresh.code}};if read()!=131 || read()!=131{{return 1}}return 0}}"
    );
    native(&text, "BBA");
}

#[test]
fn closure051_16_mutating_a_copy_payload_snapshot_keeps_owned_call_shared_native() {
    let text = "extern \"C\" fn putchar(code:i32)->i32; resource Token{code:i32;pair:(i32,i32)} impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}} fn copy_edit(token:Token)->i32{var pair=token.pair;pair.0+=1;return pair.0} fn main()->i32{let token=Token{code:65,pair:(10,20)};let read=move ()=>copy_edit(token);if read()!=11 || read()!=11{return 1}return 0}";
    native(text, "A");
}
