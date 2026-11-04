//! Raw executable/Wire controls for the first actual physical stored-leaf proof.
//! Source/Types/native coverage is recorded separately; this fixture is hand IR.
use super::*;
use zil_ir::{CandidatePlan, Terminator, Type, ValueId};
mod fixture {
    include!("stored_flow_fixture.rs");
}

fn candidate(module: &TypedModule) -> CandidatePlan {
    zil_analyze::analyze(module, &Limits::default()).unwrap()
}
fn accept(module: &TypedModule) -> CandidatePlan {
    let plan = candidate(module);
    zil_verify::verify_module(
        module.clone(),
        plan.clone(),
        &module.target,
        &Limits::default(),
    )
    .unwrap();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &Limits::default()).unwrap();
    let (decoded, decoded_plan) =
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
    assert_eq!(&decoded, module);
    assert_eq!(decoded_plan, plan);
    assert_eq!(
        zil_analyze::analyze(&decoded, &Limits::default()).unwrap(),
        plan
    );
    zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap();
    plan
}
fn reject(module: &TypedModule, mut plan: CandidatePlan, text: &str) {
    let errors = zil_analyze::analyze(module, &Limits::default()).unwrap_err();
    assert!(
        errors.iter().all(|d| d.phase == Phase::Analyze),
        "{text}: {errors:?}"
    );
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(module, &plan, &Limits::default()).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap();
    assert!(
        zil_analyze::analyze(&decoded, &Limits::default()).is_err(),
        "{text}"
    );
    for errors in [
        zil_verify::verify_module(module.clone(), plan, &module.target, &Limits::default())
            .unwrap_err(),
        zil_verify::verify_module_bytes(&wire, &module.target, &Limits::default()).unwrap_err(),
    ] {
        assert!(
            errors.iter().all(|d| d.phase == Phase::Verify),
            "{text}: {errors:?}"
        );
    }
}

#[test]
fn terminal342_physical_shared_targets_survive_carrier_end_and_forward_calls() {
    let module = fixture::module();
    let plan = accept(&module);
    assert_eq!(
        plan.functions[3].holds[4].roots,
        vec![HoldRoot::Local(LocalId(0))]
    );
    let positive = fixture::exclusive_returns(false);
    let plan = accept(&positive);
    let negative = fixture::exclusive_returns(true);
    reject(
        &negative,
        plan,
        "second call cannot reuse an exclusive parent while its returned child is live",
    );
}

#[test]
fn terminal342_physical_stored_actual_roots_modes_and_complete_inventory_are_independent() {
    let original = fixture::module();
    let plan = accept(&original);
    let mut changed = original.clone();
    changed.life_instances.clear();
    reject(&changed, plan.clone(), "missing complete nominal instance");
    let mut changed = original.clone();
    changed.life_instances[0].arguments[0] = zil_ir::LifeNominalActual::Type(Type::I64);
    reject(
        &changed,
        plan.clone(),
        "actual generic kind differs from full declaration ordinal",
    );
    let mut changed = original.clone();
    changed.life_instances[0].arguments[0] = zil_ir::LifeNominalActual::Lifetime(1);
    reject(
        &changed,
        plan.clone(),
        "nominal class outside actual domain",
    );
    let mut changed = original.clone();
    changed
        .life_declarations
        .push(changed.life_declarations[0].clone());
    reject(
        &changed,
        plan.clone(),
        "duplicate stable declaration identity",
    );
    let mut changed = original.clone();
    changed.functions[1].storage_life.as_mut().unwrap().bindings[2].binder =
        zil_ir::StorageBinder::Invocation(0);
    reject(
        &changed,
        plan.clone(),
        "stored target cannot acquire receiver invocation bound",
    );
    let mut changed = original.clone();
    let f = &mut changed.functions[3];
    f.values.push(fixture::address(Type::I64, true));
    f.blocks[0].instructions.insert(
        8,
        fixture::instruction(
            Some(6),
            InstructionKind::BorrowLocalMut { local: LocalId(0) },
        ),
    );
    let mut forged = plan.clone();
    forged.functions[3].holds.push(zil_ir::ValueHold {
        value: ValueId(6),
        roots: vec![HoldRoot::Local(LocalId(0))],
    });
    reject(
        &changed,
        forged,
        "CopyShared retains the actual target parent after carrier teardown",
    );
    let mut changed = original.clone();
    let f = &mut changed.functions[4];
    f.values.push(fixture::address(Type::I64, false));
    f.blocks[0].instructions.push(fixture::instruction(
        Some(1),
        InstructionKind::Call {
            function: zil_ir::FunctionId(4),
            arguments: vec![ValueId(0)],
        },
    ));
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    let mut forged = plan;
    forged.functions[4].holds.push(zil_ir::ValueHold {
        value: ValueId(1),
        roots: vec![HoldRoot::Parameter(0)],
    });
    reject(
        &changed,
        forged,
        "recursive bottom cannot be claimed as an actual returned parameter",
    );
}

#[test]
fn terminal342_physical_stored_current_cfg_is_reproved_and_work_has_fresh_boundaries() {
    fn boundary<T: std::fmt::Debug + PartialEq>(
        phase: Phase,
        run: impl Fn(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    ) -> usize {
        let expected = run(Limits::default().max_analysis_steps).unwrap();
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(value) => {
                    assert_eq!(value, expected);
                    high = mid
                }
                Err(d) => {
                    assert!(
                        d.iter().all(
                            |x| x.phase == phase && x.category == ResultCategory::ResourceLimit
                        ),
                        "{d:?}"
                    );
                    low = mid
                }
            }
        }
        assert_eq!(run(high).unwrap(), expected);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|x| x.phase == phase && x.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap(), expected);
        high
    }
    let original = fixture::module();
    let plan = accept(&original);
    let n = boundary(Phase::Analyze, |n| {
        zil_analyze::analyze(
            &original,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
    });
    let v = boundary(Phase::Verify, |n| {
        zil_verify::verify_module(
            original.clone(),
            plan.clone(),
            &original.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|v| v.fingerprint())
    });
    println!(
        "342 physical shared stored raw N Analyze={n} Verify={v}; N-1 ResourceLimit/fresh N identical"
    );
    let mut session = zil_analyze::AnalysisSession::default();
    let warm = session.analyze(&original, &Limits::default()).unwrap();
    assert_eq!(warm, plan);
    let mut changed = original.clone();
    let f = &mut changed.functions[1];
    f.locals.push(zil_ir::LocalDecl {
        id: LocalId(1),
        name: "activation".into(),
        ty: Type::I64,
        mutable: false,
        span: f.span,
    });
    f.values.push(Type::I64);
    f.blocks[0].instructions.pop();
    f.blocks[0].instructions.extend([
        fixture::instruction(Some(2), InstructionKind::Constant(zil_ir::Constant::I64(7))),
        fixture::instruction(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(2),
            },
        ),
        fixture::instruction(Some(1), InstructionKind::BorrowLocal { local: LocalId(1) }),
    ]);
    assert!(session.analyze(&changed, &Limits::default()).is_err());
    let mut forged = plan;
    forged.functions[1].blocks[0]
        .initialized_out
        .push(LocalId(1));
    forged.functions[1].holds.push(zil_ir::ValueHold {
        value: ValueId(2),
        roots: vec![],
    });
    reject(
        &changed,
        forged,
        "cached symbolic role cannot replace actual activation-local escape proof",
    );
}
