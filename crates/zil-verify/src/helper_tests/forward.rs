//! Schema 31 raw and wire probes: a ClosureId is not an instance source.
use super::*;
use zil_base::ResultCategory;
use zil_ir::CallableReturnSource;

fn keep(actual: CallableKind, declared: CallableKind) -> Raw {
    let mut raw = fixture(actual, declared, false, 0, Type::I32);
    let function = &mut raw.module.functions[0];
    function.return_type = env();
    function.values = vec![env()];
    function.blocks[0].instructions = vec![op(
        Some(ValueId(0)),
        InstructionKind::TakeLocal { local: LocalId(0) },
    )];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    function.callable_contract.as_mut().unwrap().roles[0].cleanup = CallableCleanup::Returned;
    function.callable_return_contract = Some(CallableReturnContract {
        source: CallableReturnSource::Parameter(0),
        lending_captures: vec![],
        closure: ClosureId(0),
        kind: declared,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
    });
    raw
}

#[test]
fn raw_wire_forwarding_keeps_declared_modes_and_actual_instance() {
    for (actual, declared) in [
        (CallableKind::Fn, CallableKind::Fn),
        (CallableKind::FnMut, CallableKind::FnMut),
        (CallableKind::FnOnce, CallableKind::FnOnce),
        (CallableKind::Fn, CallableKind::FnOnce),
    ] {
        keep(actual, declared).accept();
    }
}

#[test]
fn same_type_is_not_the_input_instance() {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    // A second value of the same concrete environment type cannot stand in
    // for this invocation's actual parameter, even if it is a valid closure.
    let function = &mut raw.module.functions[0];
    function.values = vec![Type::I32, env()];
    function.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::Constant(Constant::I32(7)),
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![ValueId(0)],
            },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    raw.reject("instance");
}

#[test]
fn forged_index_cleanup_and_permission_are_rejected() {
    let mut wrong = keep(CallableKind::Fn, CallableKind::FnOnce);
    wrong.module.functions[0]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .source = CallableReturnSource::Parameter(1);
    wrong.reject("parameter");

    let mut cleanup = keep(CallableKind::Fn, CallableKind::FnOnce);
    cleanup.module.functions[0]
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[0]
        .cleanup = CallableCleanup::Owned;
    cleanup.reject("envelope");

    let mut upgrade = keep(CallableKind::Fn, CallableKind::FnOnce);
    upgrade.module.functions[0]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .kind = CallableKind::Fn;
    upgrade.reject("mode");
}

#[test]
fn fnmut_call_preserves_instance_but_fnonce_call_consumes_it() {
    let mut permitted = keep(CallableKind::FnMut, CallableKind::FnMut);
    let function = &mut permitted.module.functions[0];
    function.locals[0].mutable = true;
    function.values = vec![Type::I32, env()];
    function.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::CallClosureMut {
                callee: LocalId(0),
                arguments: vec![],
            },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    function.callable_contract.as_mut().unwrap().traces =
        CallableTraces::Finite(vec![CallableTrace {
            calls: vec![CallEvent {
                parameter: 0,
                kind: CallableKind::FnMut,
            }],
            exit: CallableExit::Return,
        }]);
    permitted.accept();

    let mut consumed = keep(CallableKind::FnOnce, CallableKind::FnOnce);
    let function = &mut consumed.module.functions[0];
    function.values = vec![env(), Type::I32, env()];
    function.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::CallClosureOnce {
                callee: ValueId(0),
                arguments: vec![],
            },
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
    ];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    consumed.reject("initialization");
}

#[test]
fn nested_keep_uses_actual_argument_and_does_not_upgrade_bound() {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let mut child = raw.module.functions[0].clone();
    child.id = FunctionId(2);
    child.name = "child_keep".into();
    raw.module.functions.push(child);
    let parent = &mut raw.module.functions[0];
    parent.values = vec![env(), env()];
    parent.blocks[0].instructions = vec![
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::Call {
                function: FunctionId(2),
                arguments: vec![ValueId(0)],
            },
        ),
    ];
    parent.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    raw.accept();

    let mut upgraded = raw.clone();
    upgraded.module.functions[2]
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[0]
        .kind = CallableKind::Fn;
    upgraded.module.functions[2]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .kind = CallableKind::Fn;
    upgraded.reject("bound");
}

#[test]
fn each_branch_returns_this_instances_parameter() {
    let mut raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let function = &mut raw.module.functions[0];
    function.locals.push(LocalDecl {
        id: LocalId(1),
        name: "flag".into(),
        ty: Type::Bool,
        mutable: false,
        span: span(),
    });
    function.parameters.push(LocalId(1));
    function.values = vec![Type::Bool, env(), env()];
    function.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: vec![op(Some(ValueId(0)), InstructionKind::Load(LocalId(1)))],
            terminator: Terminator::Branch {
                condition: ValueId(0),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![op(
                Some(ValueId(1)),
                InstructionKind::TakeLocal { local: LocalId(0) },
            )],
            terminator: Terminator::Return(Some(ValueId(1))),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![op(
                Some(ValueId(2)),
                InstructionKind::TakeLocal { local: LocalId(0) },
            )],
            terminator: Terminator::Return(Some(ValueId(2))),
        },
    ];
    raw.accept();
}

#[test]
fn owned_environment_returns_its_remaining_responsibility() {
    let mut raw = fixture(
        CallableKind::FnOnce,
        CallableKind::FnOnce,
        false,
        0,
        owned(),
    );
    let function = &mut raw.module.functions[0];
    function.return_type = env();
    function.values = vec![env()];
    function.blocks[0].instructions = vec![op(
        Some(ValueId(0)),
        InstructionKind::TakeLocal { local: LocalId(0) },
    )];
    function.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    function.callable_contract.as_mut().unwrap().roles[0].cleanup = CallableCleanup::Returned;
    function.callable_return_contract = Some(CallableReturnContract {
        source: CallableReturnSource::Parameter(0),
        lending_captures: vec![],
        closure: ClosureId(0),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![0],
    });
    raw.accept();

    let mut missing = raw.clone();
    missing.module.functions[0]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup
        .clear();
    missing.reject("cleanup");
}

#[test]
fn historical_loan_wire30_stays_rejected_without_reencoding() {
    let original = include_bytes!("../../../../evidence/impl/248/loan_revival.wire");
    let errors =
        verify_module_bytes(original, &TargetProfile::host_llvm(), &Limits::default()).unwrap_err();
    assert!(format!("{errors:?}").contains("version"), "{errors:?}");
}

#[test]
fn forward_raw_wire_budget_has_exact_fresh_boundary() {
    let raw = keep(CallableKind::Fn, CallableKind::FnOnce);
    let plan = raw.plan();
    let bytes =
        zil_ir::wire::encode_module_candidate(&raw.module, &plan, &Limits::default()).unwrap();
    for wire in [false, true] {
        let run = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if wire {
                verify_module_bytes(&bytes, &raw.module.target, &limits)
            } else {
                verify_module(
                    raw.module.clone(),
                    plan.clone(),
                    &raw.module.target,
                    &limits,
                )
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap().fingerprint();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(verified) => {
                    assert_eq!(verified.fingerprint(), expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        println!("forward254 wire={wire} N={high}; N-1=ResourceLimit; fresh N=Pass");
    }
}
