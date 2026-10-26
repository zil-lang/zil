//! S0/C2a source-to-native checks plus untrusted-IR mutation controls.
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, Stage, compile_source};
use zil_ir::*;

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-borrow-{}-{}",
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
fn unit(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "borrow.zil", text)
}
fn native(text: &str, expected: i64) {
    let temp = Temp::new();
    let report = compile_source(
        &unit(text),
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
        llvm.contains("type { ptr"),
        "borrow environment is not a pointer: {llvm}"
    );
    assert!(llvm.contains("getelementptr i8, ptr %local."));
    let mut child = Command::new(&report.artifacts["executable"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= end {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native timeout");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{expected}\n")
    );
    assert!(output.stderr.is_empty());
}
fn reject_source(text: &str, category: ResultCategory, phase: Phase, code: &str) {
    let report = compile_source(
        &unit(text),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(report.status, category, "{}", report.to_json());
    assert_eq!(report.phase, phase, "{}", report.to_json());
    assert_eq!(report.diagnostics[0].code, code, "{}", report.to_json());
    assert!(!report.artifacts.contains_key("llvm"));
}
fn typed(text: &str) -> TypedModule {
    let source = unit(text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    let core = zil_expand::expand(&source, &ast).unwrap();
    let core = zil_types::infer_source_closures(&core, &Limits::default()).unwrap();
    let core =
        zil_ct::evaluate_source_constants(&core, &TargetProfile::default(), &Limits::default())
            .unwrap();
    zil_types::check(&core, &TargetProfile::default(), &Limits::default()).unwrap()
}
fn verify(
    module: TypedModule,
    plan: CandidatePlan,
) -> zil_base::CompileResult<zil_verify::VerifiedModule> {
    let target = module.target.clone();
    zil_verify::verify_module(module, plan, &target, &Limits::default())
}

#[test]
fn ordinary_capture_and_all_copies_end_at_last_use() {
    native(
        "fn result() -> i64 { var base: i64 = 40; let f = x => base + x; var g = f; g = f; let total = f(1) + g(2); base = 100; return total + base; }",
        183,
    );
}
#[test]
fn parameter_address_has_distinct_native_stack_instances() {
    native(
        "fn calc(base: i64) -> i64 { let f = x => base + x; return f(1) + f(2); } fn result() -> i64 { return calc(40) + calc(100); }",
        286,
    );
}
#[test]
fn loop_local_storage_restarts_and_control_exits_end_it() {
    native(
        "fn result() -> i64 { var total: i64 = 0; for i in 0..8 { let x = i; let f = () => x; if i == 2 { total = total + f(); continue; } if i == 5 { total = total + f(); break; } total = total + f(); } return total; }",
        15,
    );
}
#[test]
fn branch_and_shadowed_local_have_distinct_places() {
    native(
        "fn result() -> i64 { var x: i64 = 40; let f = () => x; var total: i64 = 0; if true { var x: i64 = 7; x = 8; total = f() + x; } else { total = f(); } x = 100; return total + x; }",
        148,
    );
}
#[test]
fn unit_address_has_real_storage_and_unit_abi_is_erased() {
    native(
        "fn result() -> i64 { let u = (); let f = () => u; f(); f(); return 42; }",
        42,
    );
}
#[test]
fn borrowed_and_move_environments_keep_distinct_semantics() {
    native(
        "fn result() -> i64 { var x: i64 = 40; let a = move () => x; let b = () => x; let first = b(); x = 99; return first + a() + x; }",
        179,
    );
}
#[test]
fn short_circuit_loop_temps_and_rhs_last_use_preserve_storage() {
    native(
        "fn result() -> i64 { var x: i64 = 3; let f = () => x; var count: i64 = 0; while count < 3 && f() == 3 { count = count + 1; } x = f() + count; return x; }",
        6,
    );
}
#[test]
fn conflicting_writes_include_alias_copies_and_loop_backedges() {
    for text in [
        "fn result() -> i32 { var x = 1; let f = () => x; x = 2; return f(); }",
        "fn result() -> i32 { var x = 1; let f = () => x; let g = f; f(); x = 2; return g(); }",
        "fn result() -> i32 { var x = 1; let f = () => x; for i in 0..2 { f(); x = x + 1; } return 0; }",
        "fn result() -> i32 { var x = 1; let f = () => x; if true { x = 2; } return f(); }",
    ] {
        reject_source(
            text,
            ResultCategory::SafeReject,
            Phase::Verify,
            "E-BORROW-CONFLICT",
        );
    }
}
#[test]
fn exclusive_capture_calls_require_mutable_environments_and_escape_stays_closed() {
    reject_source(
        "fn result() -> i32 { var x = 1; let f = () => { x = 2; return x; }; return f(); }",
        ResultCategory::SafeReject,
        Phase::Types,
        "E-CLOSURE-MUTABILITY",
    );
    native(
        "fn result() -> i32 { var x = 1; var f = () => { x = 2; return x; }; let n=f(); return n+x; }",
        4,
    );
    let report = compile_source(
        &unit("fn result() -> i32 { var x = 1; let p = &mut x; return 0; }"),
        &CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    reject_source(
        "fn result() -> i32 { let x = 1; let f = () => x; return f; }",
        ResultCategory::Unsupported,
        Phase::Types,
        "E-UNSUPPORTED",
    );
}
#[test]
fn raw_hold_rows_bind_every_value_and_external_capture() {
    let module = typed("fn result() -> i32 { let x = 40; let f = () => x + 2; return f(); }");
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert!(
        plan.functions[0]
            .holds
            .iter()
            .any(|h| h.roots == [HoldRoot::Local(LocalId(0))])
    );
    assert!(
        plan.functions[1]
            .holds
            .iter()
            .any(|h| h.roots == [HoldRoot::Capture(0)])
    );
    verify(module.clone(), plan.clone()).unwrap();
    for mode in 0..4 {
        let mut bad = plan.clone();
        match mode {
            0 => bad.functions[0].holds.clear(),
            1 => bad.functions[0]
                .holds
                .iter_mut()
                .for_each(|h| h.roots.clear()),
            2 => bad.functions[1]
                .holds
                .iter_mut()
                .for_each(|h| h.roots.clear()),
            _ => bad.functions[0].holds[0]
                .roots
                .push(HoldRoot::Local(LocalId(999))),
        }
        assert_eq!(
            verify(module.clone(), bad).unwrap_err()[0].code,
            "E-VERIFY-HOLD"
        );
    }
}
#[test]
fn raw_dead_storage_and_reactivation_cannot_hide_a_live_reference() {
    for reactivate in [false, true] {
        let mut module =
            typed("fn result() -> i32 { let x = 40; let f = () => x + 2; return f(); }");
        let block = &mut module.functions[0].blocks[0];
        let captured = block
            .instructions
            .iter()
            .find_map(|i| match i.kind {
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } => Some(local),
                _ => None,
            })
            .unwrap();
        let insertion = block
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CallClosure { .. }))
            .unwrap();
        let original_end = block
            .instructions
            .iter()
            .position(
                |i| matches!(i.kind, InstructionKind::StorageDead(local) if local == captured),
            )
            .unwrap();
        assert!(original_end > insertion);
        if !reactivate {
            // Move the one storage end before the still-live captured use.
            block.instructions.remove(original_end);
        }
        let mut transition = vec![Instruction {
            result: None,
            kind: InstructionKind::StorageDead(captured),
            span: Default::default(),
        }];
        if reactivate {
            // Balance activations; the live capture already conflicts at Dead.
            transition.push(Instruction {
                result: None,
                kind: InstructionKind::StorageLive(captured),
                span: Default::default(),
            });
        }
        block.instructions.splice(insertion..insertion, transition);
        let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
        assert_eq!(
            verify(module, plan).unwrap_err()[0].code,
            "E-BORROW-CONFLICT"
        );
    }
}
#[test]
fn raw_address_access_requires_real_address_type() {
    let mut module = typed("fn result() -> i32 { let x = 40; let f = () => x + 2; return f(); }");
    let instruction = module.functions[1].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::ReadAddress { .. }))
        .unwrap();
    if let InstructionKind::ReadAddress { address } = &mut instruction.kind {
        *address = instruction.result.unwrap();
    }
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert_eq!(
        verify(module, plan).unwrap_err()[0].code,
        "E-VERIFY-ADDRESS"
    );
}

#[test]
fn one_shared_root_with_many_unrelated_locals_and_blocks_fits_default_budget() {
    let mut module = typed("fn result() -> i32 { let x = 40; let f = () => x + 2; return f(); }");
    let f = &mut module.functions[0];
    for _ in 0..512 {
        let id = LocalId(f.locals.len() as u32);
        f.locals.push(LocalDecl {
            id,
            name: format!("unrelated{}", id.0),
            ty: Type::I32,
            mutable: false,
            span: Default::default(),
        });
    }
    let mut first = f.blocks.remove(0);
    let call = first
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CallClosure { .. }))
        .unwrap();
    let last = BasicBlock {
        id: BlockId(96),
        instructions: first.instructions.split_off(call),
        terminator: first.terminator.clone(),
    };
    first.terminator = Terminator::Jump(BlockId(1));
    f.blocks.push(first);
    for block in 1..96 {
        f.blocks.push(BasicBlock {
            id: BlockId(block),
            instructions: vec![],
            terminator: Terminator::Jump(BlockId(block + 1)),
        });
    }
    f.blocks.push(last);
    let plan = zil_analyze::analyze(&module, &Limits::default()).unwrap();
    assert!(plan.functions[0].holds.iter().all(|h| {
        h.roots
            .iter()
            .all(|root| *root == HoldRoot::Local(LocalId(0)))
    }));
    verify(module.clone(), plan.clone()).unwrap();
    let limits = Limits {
        max_analysis_steps: 1000,
        ..Limits::default()
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
