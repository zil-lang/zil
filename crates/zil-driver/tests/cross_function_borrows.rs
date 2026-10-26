//! Shared argument/result contracts: real source, native O0/O2, and forged facts.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::{HoldRoot, InstructionKind, LocalId, TypedModule};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-call-borrow-{}-{}",
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
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "shared.zil", text)
}
fn execute(path: &Path, expected: i64) {
    let mut child = Command::new(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native shared-address program timed out");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, format!("{expected}\n").as_bytes());
    assert!(output.stderr.is_empty());
}
fn native(text: &str, expected: i64) -> String {
    let temp = Temp::new();
    let report = compile_source(
        &source(text),
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(temp.0.join("program")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(
        llvm.contains("getelementptr i8, ptr %local."),
        "missing real source address: {llvm}"
    );
    execute(&report.artifacts["executable"], expected);
    let optimized = temp.0.join("optimized");
    let clang = Command::new("clang")
        .args(["--target=x86_64-unknown-linux-gnu", "-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(
        clang.status.success(),
        "{}",
        String::from_utf8_lossy(&clang.stderr)
    );
    execute(&optimized, expected);
    llvm
}
fn reject(text: &str, category: ResultCategory, phase: Phase, code: &str) {
    let report = compile_source(&source(text), &CompileOptions::default());
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("llvm"));
    assert!(!report.artifacts.contains_key("executable"));
}
fn typed(text: &str) -> TypedModule {
    let source = source(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}

#[test]
fn incoming_pointer_and_exact_return_origin_use_native_abi() {
    let llvm = native(
        "fn first(a: &i64, b: &i64) -> &i64 { return a; } fn read(p: &i64) -> i64 { return *p; } fn result() -> i64 { var x: i64 = 40; var y: i64 = 2; let p = first(&x, &y); y = 99; let answer = read(p); x = 100; return answer + x + y; }",
        239,
    );
    assert!(llvm.contains("define ptr @zil.fn."), "{llvm}");
    assert!(llvm.contains("call ptr @zil.fn."), "{llvm}");
    assert!(!llvm.contains("malloc"), "references must not allocate");
}

#[test]
fn branch_union_alias_substitution_and_rebinding_preserve_last_use() {
    native(
        "fn choose(a: &i64, b: &i64, flag: bool) -> &i64 { return if flag { a } else { b }; } fn result() -> i64 { var x: i64 = 40; var y: i64 = 2; var p = choose(&x, &y, false); let first = *p; p = &y; x = 100; let second = *p; y = 200; let same = choose(&x, &x, true); return first + second + y + *same; }",
        304,
    );
}

#[test]
fn recursive_and_mutually_recursive_forwarding_find_grounded_origins() {
    native(
        "fn relay(a: &i64, b: &i64, n: i32) -> &i64 { if n == 0 { return b; } return again(a, b, n - 1); } fn again(a: &i64, b: &i64, n: i32) -> &i64 { return relay(a, b, n); } fn result() -> i64 { var x: i64 = 40; var y: i64 = 2; let p = relay(&x, &y, 8); x = 100; return *p + x; }",
        102,
    );
    native(
        "fn rotate(a: &i64, b: &i64, n: i32) -> &i64 { if n == 0 { return a; } return rotate(b, a, n - 1); } fn result() -> i64 { let x: i64 = 40; let y: i64 = 2; return *rotate(&x, &y, 7); }",
        2,
    );
}

#[test]
fn returned_pointer_composes_with_closure_and_inner_calls() {
    native(
        "fn choose(a: &i64, b: &i64) -> &i64 { return b; } fn read(p: &i64) -> i64 { return *p; } fn combine(a: &i64, b: &i64) -> i64 { let f = move () => { let p = choose(a, b); return read(p) + *a; }; return f(); } fn result() -> i64 { var x: i64 = 40; var y: i64 = 2; let p = choose(&x, &y); let snapshot: i64 = 3; let f = move () => read(p) + snapshot; let g = f; let total = f() + g() + combine(&x, &y); y = 100; return total + y; }",
        152,
    );
}

#[test]
fn bool_unit_and_i32_incoming_references_work() {
    native(
        "fn flag(p: &bool) -> &bool { return p; } fn unit(p: &()) -> &() { return p; } fn read(p: &i32) -> i32 { return *p; } fn result() -> i32 { let u = (); let yes = true; let value = 42; *unit(&u); if *flag(&yes) { return read(&value); } return 0; }",
        42,
    );
}

#[test]
fn every_returned_root_blocks_writes_until_last_use() {
    for text in [
        "fn id(a: &i32) -> &i32 { return a; } fn result() -> i32 { var x = 1; let p = id(&x); x = 2; return *p; }",
        "fn pick(a: &i32, b: &i32, flag: bool) -> &i32 { return if flag { a } else { b }; } fn result() -> i32 { var x = 1; var y = 2; let p = pick(&x, &y, true); y = 3; return *p; }",
        "fn id(a: &i32) -> &i32 { return a; } fn result() -> i32 { var x = 1; let p = id(&x); let f = move () => *p; let g = f; f(); x = 2; return g(); }",
        "fn id(a: &i32) -> &i32 { return a; } fn result() -> i32 { var x = 1; var p = &x; if true { let inner = 2; p = id(&inner); } return *p; }",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}

#[test]
fn local_storage_and_value_parameters_cannot_escape() {
    for text in [
        "fn bad() -> &i32 { let x = 1; return &x; } fn result() -> i32 { return 0; }",
        "fn bad(x: i32) -> &i32 { return &x; } fn result() -> i32 { return 0; }",
        "fn id(a: &i32) -> &i32 { return a; } fn bad(a: &i32) -> &i32 { let x = 1; return id(&x); } fn result() -> i32 { return 0; }",
        "fn bad(a: &i32, flag: bool) -> &i32 { let x = 1; return if flag { a } else { &x }; } fn result() -> i32 { return 0; }",
    ] {
        reject(
            text,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-ESCAPE",
        );
    }
    reject(
        "fn loop_ref(a: &i32) -> &i32 { return loop_ref(a); } fn result() -> i32 { return 0; }",
        ResultCategory::SafeReject,
        Phase::Verify,
        "E-BORROW-ORIGIN",
    );
}

#[test]
fn forged_argument_roots_and_missing_return_holds_fail() {
    let module = typed(
        "fn pick(a: &i64, b: &i64, flag: bool) -> &i64 { return if flag { a } else { b }; } fn result() -> i64 { let x: i64 = 40; let y: i64 = 2; return *pick(&x, &y, false); }",
    );
    let limits = Limits::default();
    let plan = zil_analyze::analyze(&module, &limits).unwrap();
    zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits).unwrap();
    let call = module.functions[1]
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap()
        .result
        .unwrap();
    assert_eq!(
        plan.functions[1].holds[call.0 as usize].roots,
        [HoldRoot::Local(LocalId(0)), HoldRoot::Local(LocalId(1))]
    );
    assert!(
        plan.functions[0]
            .holds
            .iter()
            .any(|h| h.roots == [HoldRoot::Parameter(0), HoldRoot::Parameter(1)])
    );
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan.clone())
    );
    for mode in 0..3 {
        let mut altered = plan.clone();
        match mode {
            0 => altered.functions[1].holds[call.0 as usize].roots.clear(),
            1 => {
                altered.functions[1].holds[call.0 as usize].roots =
                    vec![HoldRoot::Local(LocalId(0))]
            }
            _ => {
                altered.functions[0]
                    .holds
                    .iter_mut()
                    .find(|h| !h.roots.is_empty())
                    .unwrap()
                    .roots = vec![HoldRoot::Parameter(99)]
            }
        }
        let error = zil_verify::verify_module(module.clone(), altered, &module.target, &limits)
            .unwrap_err();
        assert_eq!(error[0].code, "E-VERIFY-HOLD");
    }
}

#[test]
fn source_module_boundaries_preserve_pointer_signatures_and_return_roots() {
    let temp = Temp::new();
    let root = temp.0.join("main.zil");
    fs::write(
        &root,
        include_str!("../../../examples/call-borrows/main.zil"),
    )
    .unwrap();
    fs::write(
        temp.0.join("select.zil"),
        include_str!("../../../examples/call-borrows/select.zil"),
    )
    .unwrap();
    let report = zil_driver::compile_path(
        &root,
        &CompileOptions {
            stop_after: Stage::Build,
            entry: Some("answer".into()),
            output: Some(temp.0.join("program")),
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.source_files.len(), 2);
    execute(&report.artifacts["executable"], 305);
    let optimized = temp.0.join("optimized");
    let output = Command::new("clang")
        .args(["-x", "ir", "-O2"])
        .arg(&report.artifacts["llvm"])
        .arg("-o")
        .arg(&optimized)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    execute(&optimized, 305);
}

#[test]
fn low_budget_never_turns_recursive_unknown_origins_into_success() {
    let module = typed(
        "fn id(a: &i32, n: i32) -> &i32 { if n == 0 { return a; } return id(a, n - 1); } fn result() -> i32 { let x = 42; return *id(&x, 4); }",
    );
    let defaults = Limits::default();
    let plan = zil_analyze::analyze(&module, &defaults).unwrap();
    let limits = Limits {
        max_analysis_steps: 1,
        ..defaults
    };
    assert_eq!(
        zil_analyze::analyze(&module, &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    assert_eq!(
        zil_verify::verify_module(module.clone(), plan, &module.target, &limits).unwrap_err()[0]
            .category,
        ResultCategory::ResourceLimit
    );
}

#[test]
fn contract_identity_tracks_return_origin_changes_only_after_verify() {
    let temp = Temp::new();
    let root = temp.0.join("main.zil");
    let library = temp.0.join("library.zil");
    fs::write(
        &root,
        "import library; fn main() -> i32 { let x = 1; let y = 2; return *library.pick(&x, &y); }",
    )
    .unwrap();
    fs::write(
        &library,
        "pub fn pick(a: &i32, b: &i32) -> &i32 { return a; }",
    )
    .unwrap();
    let report = zil_driver::compile_path(
        &root,
        &CompileOptions {
            stop_after: Stage::Types,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .scalar_interfaces
            .iter()
            .all(|i| i.result_origins.is_none() && i.contract_fingerprint.is_none())
    );
    let checked = |directory: &str| {
        let report = zil_driver::compile_path(
            &root,
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(temp.0.join(directory)),
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.artifacts.contains_key("verified-interfaces"));
        let interface = report
            .scalar_interfaces
            .iter()
            .find(|i| i.exports.iter().any(|f| f.name == "pick"))
            .unwrap();
        (
            interface.signature_fingerprint,
            interface.contract_fingerprint.unwrap(),
            interface.result_origins.clone().unwrap(),
        )
    };
    let before = checked("first");
    fs::write(
        &library,
        "pub fn pick(a: &i32, b: &i32) -> &i32 { return b; }",
    )
    .unwrap();
    let after = checked("second");
    assert_eq!(before.0, after.0);
    assert_ne!(before.1, after.1);
    assert_eq!(before.2[0].parameters, [0]);
    assert_eq!(after.2[0].parameters, [1]);
}
