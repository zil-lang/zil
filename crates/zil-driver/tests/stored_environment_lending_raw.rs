//! Untrusted executable origins and public lending claims are checked separately.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_ir::{
    CallableKind, CandidatePlan, Instruction, InstructionKind, Type, TypedModule, ValueId,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = "fn make(a:i32,b:i32)->impl Fn()->&i32{return move()=>{b;return &a}}fn main()->i32{let f=make(17,25);let s=f();return *s-17}";

fn typed(source: &str) -> TypedModule {
    let s = SourceUnit::new(FileId(0), ModuleId(0), "raw184.zil", source);
    let ast = zil_syntax::parse(&s).into_result().unwrap();
    let core = zil_expand::expand(&s, &ast).unwrap();
    zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default()).unwrap()
}
fn baseline() -> (TypedModule, CandidatePlan) {
    let m = typed(SOURCE);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    (m, p)
}
fn record(name: &str, m: &TypedModule, p: &CandidatePlan, results: &[Vec<Diagnostic>]) {
    if let Some(base) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let dir = PathBuf::from(base).join(format!(
            "raw184-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("module.txt"), format!("{m:#?}")).unwrap();
        fs::write(dir.join("candidate.txt"), format!("{p:#?}")).unwrap();
        fs::write(dir.join("results.txt"), format!("{results:#?}")).unwrap();
    }
}
fn reject(name: &str, m: TypedModule, mut p: CandidatePlan, code: &str) {
    p.module_fingerprint = m.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
    assert_eq!(decoded, (m.clone(), p.clone()));
    let results = [
        zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap_err(),
        zil_verify::verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap_err(),
    ];
    record(name, &m, &p, &results);
    for ds in results {
        assert!(
            ds.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::SafeReject
                && d.code == code),
            "{name}: {ds:?}"
        );
    }
}
fn synchronize(m: &mut TypedModule) {
    for f in &mut m.functions {
        if let Some(c) = &mut f.callable_return_contract {
            c.lending_captures = m.closures[c.closure.0 as usize].lending_captures.clone();
            c.kind = m.closures[c.closure.0 as usize].kind;
        }
    }
}

#[test]
fn raw_and_wire_reconstruct_exact_environment_sources() {
    for (name, indices) in [("missing", vec![]), ("overclaim", vec![0, 1])] {
        let (mut m, p) = baseline();
        m.closures[0].lending_captures = indices;
        synchronize(&mut m);
        reject(name, m, p, "E-VERIFY-LENDING");
    }
    let (mut m, p) = baseline();
    let actual = m.closures[0].lending_captures[0];
    m.closures[0].lending_captures = vec![1 - actual];
    synchronize(&mut m);
    reject("wrong-source", m, p, "E-VERIFY-LENDING");
    let (mut m, _) = baseline();
    let entry = m.closures[0].function.0 as usize;
    let local = m.functions[entry]
        .locals
        .iter()
        .find(|l| l.name == "a")
        .unwrap()
        .id;
    let mut changed = false;
    for i in m.functions[entry]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if matches!(i.kind, InstructionKind::BorrowCapture { .. }) {
            i.kind = InstructionKind::BorrowLocal { local };
            changed = true;
        }
    }
    assert!(changed);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    reject("activation-address", m, p, "E-BORROW-ESCAPE");
}

#[test]
fn raw_and_wire_reject_consumed_and_by_value_environment_results() {
    let (mut m, p) = baseline();
    m.closures[0].kind = CallableKind::FnOnce;
    synchronize(&mut m);
    reject("consuming-kind", m, p, "E-VERIFY-LENDING");
    for once in [false, true] {
        let (mut m, _) = baseline();
        let mut changed = false;
        for f in &mut m.functions {
            for b in &mut f.blocks {
                let mut rebuilt = Vec::new();
                for mut i in std::mem::take(&mut b.instructions) {
                    if let InstructionKind::CallClosureShared { callee, arguments } = &i.kind {
                        let callee = *callee;
                        let arguments = arguments.clone();
                        let value = ValueId(f.values.len() as u32);
                        f.values.push(f.locals[callee.0 as usize].ty.clone());
                        rebuilt.push(Instruction {
                            result: Some(value),
                            kind: InstructionKind::Load(callee),
                            span: i.span,
                        });
                        i.kind = if once {
                            InstructionKind::CallClosureOnce {
                                callee: value,
                                arguments,
                            }
                        } else {
                            InstructionKind::CallClosure {
                                callee: value,
                                arguments,
                            }
                        };
                        changed = true;
                    }
                    rebuilt.push(i);
                }
                b.instructions = rebuilt;
            }
        }
        assert!(changed);
        let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
        reject(
            if once { "once-call" } else { "by-value-call" },
            m,
            p,
            if once {
                "E-VERIFY-CLOSURE-CALL"
            } else {
                "E-VERIFY-LENDING"
            },
        );
    }
}

#[test]
fn raw_and_wire_cannot_erase_result_hold_or_mutate_shared_entry() {
    let (m, mut p) = baseline();
    let mut changed = false;
    for (f, facts) in m.functions.iter().zip(&mut p.functions) {
        for i in f.blocks.iter().flat_map(|b| &b.instructions) {
            if matches!(i.kind, InstructionKind::CallClosureShared { .. }) {
                facts.holds[i.result.unwrap().0 as usize].roots.clear();
                changed = true;
            }
        }
    }
    assert!(changed);
    reject("erased-hold", m, p, "E-VERIFY-HOLD");
    let (mut m, p) = baseline();
    let entry = m.closures[0].function.0 as usize;
    let mut result = None;
    for i in m.functions[entry]
        .blocks
        .iter_mut()
        .flat_map(|b| &mut b.instructions)
    {
        if let InstructionKind::BorrowCapture { mutable, .. } = &mut i.kind {
            *mutable = true;
            result = i.result;
            break;
        }
    }
    let Type::Address { mutable, .. } = &mut m.functions[entry].values[result.unwrap().0 as usize]
    else {
        unreachable!()
    };
    *mutable = true;
    reject("shared-write", m, p, "E-VERIFY-CLOSURE-CALL");
}

#[test]
fn raw_and_wire_keep_owned_payload_write_and_cleanup_obligations() {
    let source = "fn make()->impl FnMut()->&[u8]{let b=bytes_new();return move()=>{bytes_reserve(b,4);bytes_push(b,7u8);return bytes_view(b)}}fn main()->i32{var f=make();let s=f();return i32(s.len())-1}";
    let mut m = typed(source);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    zil_verify::verify_module(m.clone(), p.clone(), &m.target, &Limits::default()).unwrap();
    m.closures[0].kind = CallableKind::Fn;
    synchronize(&mut m);
    for i in m
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
    {
        if let InstructionKind::CallClosureMut { callee, arguments } = &i.kind {
            i.kind = InstructionKind::CallClosureShared {
                callee: *callee,
                arguments: arguments.clone(),
            };
        }
    }
    reject("owned-write-as-shared", m, p, "E-VERIFY-RESOURCE");
    let mut m = typed(source);
    let p = zil_analyze::analyze(&m, &Limits::default()).unwrap();
    let mut changed = false;
    for f in &mut m.functions {
        for b in &mut f.blocks {
            if let Some(call) = b
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::CallClosureMut { .. }))
            {
                let drop = b
                    .instructions
                    .iter()
                    .position(|i| matches!(i.kind, InstructionKind::DropClosure { .. }))
                    .unwrap();
                assert!(drop > call);
                let cleanup = b.instructions.remove(drop);
                b.instructions.insert(call + 1, cleanup);
                changed = true;
            }
        }
    }
    assert!(changed);
    reject("early-environment-drop", m, p, "E-BORROW-CONFLICT");
}

#[test]
fn analyze_bounds_both_public_lending_vectors_before_fingerprinting() {
    for contract in [false, true] {
        let (mut m, _) = baseline();
        if contract {
            m.functions
                .iter_mut()
                .find_map(|f| f.callable_return_contract.as_mut())
                .unwrap()
                .lending_captures = vec![0; 17];
        } else {
            m.closures[0].lending_captures = vec![0; 17];
        }
        let limits = Limits {
            max_locals: 16,
            ..Limits::default()
        };
        let errors = zil_analyze::analyze(&m, &limits).unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Analyze
                && d.category == ResultCategory::ResourceLimit
                && d.message.contains("lending")),
            "{errors:?}"
        );
    }
}

#[test]
fn analyzer_rejects_opaque_lending_mismatch_and_pays_nonempty_metadata() {
    let (mut m, _) = baseline();
    m.functions
        .iter_mut()
        .find_map(|f| f.callable_return_contract.as_mut())
        .unwrap()
        .lending_captures
        .clear();
    let errors = zil_analyze::analyze(&m, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().any(|d| d.phase == Phase::Analyze
            && d.category == ResultCategory::SafeReject
            && d.message.contains("lending")),
        "{errors:?}"
    );
    let (m, _) = baseline();
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        match zil_analyze::analyze(&m, &limits) {
            Ok(_) => high = middle,
            Err(ds) => {
                assert!(
                    ds.iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{ds:?}"
                );
                low = middle;
            }
        }
    }
    assert!(
        zil_analyze::analyze(
            &m,
            &Limits {
                max_analysis_steps: high,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    let below = zil_analyze::analyze(
        &m,
        &Limits {
            max_analysis_steps: high - 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert!(
        below
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit)
    );
    if let Some(base) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        fs::create_dir_all(&base).unwrap();
        fs::write(
            PathBuf::from(base).join("analyze184-boundary.json"),
            format!("{{\"accepted\":{high},\"rejected\":{low}}}\n"),
        )
        .unwrap();
    }
}
