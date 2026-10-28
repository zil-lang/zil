//! Parametric range checking is useful before the cyclic helper Flow contract.
//! Core-only checks below claim symbolic admission, never complete verification.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, Optimization, Stage, compile_source};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn directory(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "impl190-for-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn check(label: &str, source: &str, stage: Stage) -> CompileReport {
    let dir = directory(label);
    fs::write(dir.join("main.zil"), source).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "range190.zil", source),
        &CompileOptions {
            stop_after: stage,
            emit_dir: Some(dir.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    report
}

fn accepted(label: &str, source: &str, stage: Stage) {
    let report = check(label, source, stage);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(
        report.verification_runs,
        usize::from(stage >= Stage::Verify)
    );
}

fn rejected(label: &str, source: &str, category: ResultCategory, message: &str) {
    let report = check(label, source, Stage::Verify);
    assert_eq!(report.status, category, "{label}: {}", report.to_json());
    assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(message)),
        "{label}: {}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

fn native(label: &str, source: &str, stdout: &[u8]) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let dir = directory(label);
        fs::write(dir.join("main.zil"), source).unwrap();
        let executable = dir.join("program");
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "range190.zil", source),
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
        let result = Command::new(executable).output().unwrap();
        fs::write(dir.join("stdout.log"), &result.stdout).unwrap();
        fs::write(dir.join("stderr.log"), &result.stderr).unwrap();
        fs::write(
            dir.join("native.json"),
            format!(
                "{{\"optimization\":\"{optimization:?}\",\"exit\":{},\"expected_exit\":0,\"stdout_hex\":\"{}\",\"expected_stdout_hex\":\"{}\"}}\n",
                result.status.code().map_or("null".into(), |code| code.to_string()),
                result.stdout.iter().map(|b| format!("{b:02x}")).collect::<String>(),
                stdout.iter().map(|b| format!("{b:02x}")).collect::<String>()
            ),
        ).unwrap();
        assert_eq!(result.status.code(), Some(0), "{result:?}");
        assert_eq!(result.stdout, stdout);
        assert!(result.stderr.is_empty(), "{result:?}");
    }
}

const EMIT: &str = r#"extern "C" fn putchar(n:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}"#;

#[test]
fn original_runtime_helper_range_reaches_the_independent_contract_stage() {
    // This is the unchanged minimal preflight input. Its cyclic contract is
    // outside this first test's Core-only claim and belongs to the Flow suite.
    accepted(
        "original-core",
        "fn batch<F:Fn()->i32>(f:&F,n:i32)->i32{var sum=0;for i in 0..n{sum+=f()}return sum}fn main()->i32{let x=2;let f=()=>x;return batch(&f,3)-6}",
        Stage::Core,
    );
}

#[test]
fn endpoint_calls_happen_once_in_order_and_the_iterator_does_not_escape() {
    let source = format!(
        r#"{EMIT}
fn lower(n:i64)->i64{{emit(76);return n}}
fn upper(n:i64)->i64{{emit(72);return n}}
fn visit<T>(value:T)->T{{
    let index:i64=7;var high:i64=9;
    for index in lower(index)..upper(high){{emit(i32(index)+48);high=0}}
    emit(i32(index)+48);
    return value
}}
fn main()->i32{{return visit(0)}}
"#
    );
    native("endpoint-order-scope", &source, b"LH787");
}

#[test]
fn an_endpoint_may_consume_a_symbolic_value_once_before_the_backedge() {
    let source = format!(
        r#"{EMIT}
fn start<T>(value:T)->i64{{emit(76);return 0}}
fn end()->i64{{emit(72);return 3}}
fn visit<T>(value:T)->i32{{for index in start(value)..end(){{emit(i32(index)+65)}}return 0}}
fn main()->i32{{return visit(42)}}
"#
    );
    native("endpoint-once-consumption", &source, b"LHABC");
    accepted(
        "endpoint-once-callback-core",
        "fn start<F:FnOnce()->i64>(f:F)->i32{for index in f()..i64(3){}return 0}fn main(){}",
        Stage::Core,
    );
}

#[test]
fn every_integer_width_infers_unsuffixed_bounds_without_borrowing_authority() {
    for width in ["u8", "u16", "i32", "i64"] {
        let source = format!(
            "fn sum<T>(token:T,high:{width})->(T,i32){{var sum=0;for index in 0..high{{sum+=i32(index)}}return (token,sum)}}fn main()->i32{{let result=sum(0,{width}(3));return result.0+result.1-3}}"
        );
        native(&format!("width-{width}"), &source, b"");
    }
}

#[test]
fn range_width_hints_cover_arithmetic_calls_and_copy_projections() {
    let source = r#"
struct Bounds{high:i64}
fn identity<T>(value:T)->T=>value
fn sum<T>(token:T,bounds:Bounds,items:[i64;1])->(T,i32){
    var sum=0;
    for index in 0..(bounds.high+1){sum+=i32(index)}
    for index in 0..items[0]{sum+=i32(index)}
    for index in 0..identity(bounds.high){sum+=i32(index)}
    return (token,sum)
}
fn main()->i32{let result=sum(0,Bounds{high:2},[i64(3)]);return result.0+result.1-7}
"#;
    native("hint-projections", source, b"");
}

#[test]
fn empty_and_reversed_ranges_still_check_the_body_and_leave_the_outer_binding() {
    let source = r#"
fn visit<T>(token:T,low:i64,high:i64)->(T,i32){
    let index=7;var sum=0;
    for index in low..high{sum+=i32(index)}
    return (token,sum+index)
}
fn main()->i32{let empty=visit(0,i64(2),i64(2));let reversed=visit(0,i64(5),i64(2));return empty.1+reversed.1-14}
"#;
    native("empty-reversed", source, b"");
    rejected(
        "empty-still-checked",
        "fn bad<T>(value:T){for index in 0..0{let lost=value}}fn main(){}",
        ResultCategory::Unsupported,
        "generic loop must preserve outer carrier and non-Copy consumption state",
    );
}

#[test]
fn an_unconditional_return_body_keeps_the_existing_finite_once_contract() {
    for count in [0, 1, 3] {
        let answer = if count == 0 { 0 } else { 77 };
        let source = format!(
            r#"{EMIT}
resource Mark{{code:i32}}impl Drop for Mark{{fn drop(self:Self){{emit(.code)}}}}
fn choose<F:FnOnce()->i32>(f:F,n:i32)->i32{{for index in 0..n{{return f()}}return 0}}
fn main()->i32{{let mark=Mark{{code:77}};let f=move()=>mark.code;return choose(f,{count})-{answer}}}
"#
        );
        native(&format!("finite-once-{count}"), &source, b"M");
    }
}

#[test]
fn backedges_cannot_repeat_a_noncopy_or_once_consumption() {
    for (label, source) in [
        (
            "unconstrained-value",
            "fn sink<T>(value:T){}fn bad<T>(value:T){for index in 0..2{sink(value)}}fn main(){}",
        ),
        (
            "callback-once",
            "fn bad<F:FnOnce()->i32>(f:F)->i32{var total=0;for index in 0..2{total+=f()}return total}fn main(){}",
        ),
        (
            "store-give",
            "fn bad<T:StoreElement>(give store:Store<T>){for index in 0..2{let taken=give store}}fn main(){}",
        ),
    ] {
        rejected(
            label,
            source,
            ResultCategory::Unsupported,
            "generic loop must preserve outer carrier and non-Copy consumption state",
        );
    }
    // This original new probe is outside the pre-existing symbolic give lane;
    // retain its exact earlier rejection instead of claiming a loop proof.
    rejected(
        "resource-give-still-outside-lane",
        "resource R{n:i32}fn bad<F:Fn()->i32>(f:&F,give r:R){for index in 0..2{let taken=give r}}fn main(){}",
        ResultCategory::Unsupported,
        "generic managed give requires a direct Package manages field",
    );
}

#[test]
fn index_mutability_scope_widths_and_known_integer_requirements_are_checked() {
    for (label, source, category, message) in [
        (
            "immutable-index",
            "fn bad<T>(value:T){for index in 0..2{index+=1}}fn main(){}",
            ResultCategory::SafeReject,
            "assignment requires a writable place",
        ),
        (
            "index-scope",
            "fn bad<T>(value:T)->i32{for index in 0..2{}return index}fn main(){}",
            ResultCategory::SafeReject,
            "unknown value index in generic body",
        ),
        (
            "mixed-width",
            "fn bad<T>(value:T){for index in 0u8..i64(3){}}fn main(){}",
            ResultCategory::SafeReject,
            "generic body types must agree before concrete instantiation",
        ),
        (
            "boolean-range",
            "fn bad<T>(value:T){for index in false..true{}}fn main(){}",
            ResultCategory::SafeReject,
            "integer range endpoints require u8, u16, i32 or i64",
        ),
        (
            "unconstrained-bound",
            "fn bad<T>(value:T){for index in value..0{}}fn main(){}",
            ResultCategory::Unsupported,
            "generic range endpoint requires a known integer type",
        ),
        (
            "not-a-range",
            "fn bad<T>(value:T){for index in [1,2]{}}fn main(){}",
            ResultCategory::Unsupported,
            "generic for requires an integer range",
        ),
    ] {
        rejected(label, source, category, message);
    }
}

#[test]
fn loop_control_templates_keep_their_original_inputs_after_symbolic_admission() {
    for control in ["break", "continue"] {
        accepted(
            control,
            &format!("fn bad<T>(value:T){{for index in 0..2{{{control}}}}}fn main(){{}}"),
            Stage::Verify,
        );
    }
}
