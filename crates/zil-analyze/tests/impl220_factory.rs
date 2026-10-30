//! Independent producer controls built directly as TypedIR. These are synthetic
//! raw/Wire evidence, not source acceptance or native execution evidence.
use zil_analyze::analyze;
use zil_base::{
    FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory, SCHEMA_VERSION, Span,
    TargetProfile,
};
use zil_ir::*;

fn span() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: span(),
    }
}
fn function(id: u32) -> TypedFunction {
    TypedFunction {
        id: FunctionId(id),
        name: format!("raw220_{id}"),
        foreign: None,
        environment: None,
        callable_return_contract: None,
        callable_contract: None,
        parameters: vec![],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![],
        values: vec![],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: span(),
    }
}
fn local(id: u32, ty: Type) -> LocalDecl {
    LocalDecl {
        id: LocalId(id),
        name: format!("local{id}"),
        ty,
        mutable: true,
        span: span(),
    }
}
fn definition(id: u32, kind: CallableKind, captures: Vec<Type>) -> ClosureDefinition {
    ClosureDefinition {
        id: ClosureId(id),
        function: FunctionId(id),
        captures,
        parameters: vec![],
        result: Type::Unit,
        kind,
        lending_captures: vec![],
    }
}
fn fixture(kind: CallableKind, retained: bool) -> TypedModule {
    let payload = Type::Record {
        name: "Owner220".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    let input = Type::Closure(ClosureId(0));
    let output = Type::Closure(ClosureId(1));
    let mut first = function(0);
    first.environment = Some(ClosureId(0));
    let mut second = function(1);
    second.environment = Some(ClosureId(1));
    let mut factory = function(2);
    factory.parameters = vec![LocalId(0)];
    factory.locals = vec![local(0, input.clone())];
    factory.return_type = output.clone();
    factory.callable_return_contract = Some(CallableReturnContract {
        closure: ClosureId(1),
        kind,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: if retained { vec![0] } else { vec![] },
        lending_captures: vec![],
    });
    if retained {
        factory.values = vec![input.clone(), output];
        factory.blocks[0].instructions = vec![
            instruction(Some(0), InstructionKind::TakeLocal { local: LocalId(0) }),
            instruction(
                Some(1),
                InstructionKind::MakeClosure {
                    closure: ClosureId(1),
                    captures: vec![ValueId(0)],
                },
            ),
            instruction(None, InstructionKind::StorageDead(LocalId(0))),
        ];
        factory.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    } else {
        factory.values = vec![output];
        factory.blocks[0].instructions = vec![
            instruction(None, InstructionKind::DropClosure { local: LocalId(0) }),
            instruction(None, InstructionKind::StorageDead(LocalId(0))),
            instruction(
                Some(0),
                InstructionKind::MakeClosure {
                    closure: ClosureId(1),
                    captures: vec![],
                },
            ),
        ];
        factory.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    }
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"synthetic independent owning factory 220"),
        target: TargetProfile::host_llvm(),
        resources: vec![ResourceDefinition {
            name: "Owner220".into(),
            payload: payload.clone(),
            teardown: None,
        }],
        functions: vec![first, second, factory],
        closures: vec![
            definition(0, kind, vec![Type::Managed(Box::new(payload))]),
            definition(1, kind, if retained { vec![input] } else { vec![] }),
        ],
        unsupported: vec![],
    }
}
fn transported(module: &TypedModule) -> TypedModule {
    let limits = Limits::default();
    let bytes =
        wire::encode_module_candidate(module, &CandidatePlan::empty_for(module), &limits).unwrap();
    let (decoded, _) = wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(&decoded, module);
    decoded
}
fn both(module: &TypedModule, rejection: Option<&str>) {
    for input in [module, &transported(module)] {
        let result = analyze(input, &Limits::default());
        if let Some(message) = rejection {
            let errors = result.expect_err(
                "raw admission must independently reject the actual malformed boundary",
            );
            assert!(
                errors
                    .iter()
                    .all(|e| e.phase == Phase::Analyze
                        && e.category != ResultCategory::ResourceLimit),
                "{errors:#?}"
            );
            assert!(
                errors.iter().any(|e| e.message.contains(message)),
                "wanted {message:?}: {errors:#?}"
            );
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn actual_modes_admit_retaining_or_dropping_the_input() {
    for mode in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        for retained in [true, false] {
            both(&fixture(mode, retained), None);
        }
    }
}

#[test]
fn closure_result_requires_an_exact_independent_return_contract() {
    let good = fixture(CallableKind::FnOnce, true);
    let mut absent = good.clone();
    absent.functions[2].callable_return_contract = None;
    both(&absent, Some("no public callable return contract"));
    let mut forged = good.clone();
    forged.functions[2]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup
        .clear();
    both(&forged, Some("cleanup differs"));
    let mut fake_helper = good;
    fake_helper.functions[2].callable_contract = Some(CallableParameterContract {
        roles: vec![],
        traces: CallableTraces::Finite(vec![]),
    });
    both(&fake_helper, Some("ordinary self-contained factory"));
}

#[test]
fn returning_the_input_type_does_not_become_a_fresh_factory() {
    let mut module = fixture(CallableKind::FnOnce, true);
    let f = &mut module.functions[2];
    f.return_type = Type::Closure(ClosureId(0));
    f.values[1] = f.return_type.clone();
    f.callable_return_contract.as_mut().unwrap().closure = ClosureId(0);
    both(&module, Some("different closure identity"));
}

#[test]
fn borrowed_and_multiple_environment_inputs_remain_closed() {
    let mut borrowed = fixture(CallableKind::FnOnce, true);
    borrowed.functions[2].locals[0].ty = Type::Address {
        mutable: false,
        inner: Box::new(Type::Closure(ClosureId(0))),
    };
    both(&borrowed, Some("ordinary self-contained factory"));
    let mut multiple = fixture(CallableKind::FnOnce, true);
    multiple.functions[2]
        .locals
        .push(local(1, Type::Closure(ClosureId(0))));
    multiple.functions[2].parameters.push(LocalId(1));
    both(&multiple, Some("ordinary self-contained factory"));
}

#[test]
fn same_type_positions_cannot_take_one_input_twice() {
    let mut module = fixture(CallableKind::FnOnce, true);
    let f = &mut module.functions[2];
    f.values.push(Type::Closure(ClosureId(0)));
    f.blocks[0].instructions.insert(
        1,
        instruction(Some(2), InstructionKind::TakeLocal { local: LocalId(0) }),
    );
    both(&module, Some("not initialized"));
}

#[test]
fn captured_owning_closure_cannot_be_copied_by_read_capture() {
    let mut module = fixture(CallableKind::FnOnce, true);
    module.functions[1].values.push(Type::Closure(ClosureId(0)));
    module.functions[1].blocks[0].instructions.push(instruction(
        Some(0),
        InstructionKind::ReadCapture { index: 0 },
    ));
    both(&module, Some("ReadCapture cannot copy"));
}

#[test]
fn capture_cycles_and_noncanonical_entry_identities_are_rejected() {
    let good = fixture(CallableKind::FnOnce, true);
    let mut cycle = good.clone();
    cycle.closures[0].captures = vec![Type::Closure(ClosureId(1))];
    both(&cycle, Some("contains a cycle"));
    let mut wrong = good;
    wrong.closures[0].id = ClosureId(1);
    both(&wrong, Some("actual entry identity"));
}

#[test]
fn deep_borrowed_captures_are_not_hidden_by_an_owning_parent() {
    let mut module = fixture(CallableKind::FnOnce, true);
    let mut child = definition(
        2,
        CallableKind::Fn,
        vec![Type::Desc(Box::new(module.resources[0].payload.clone()))],
    );
    child.function = FunctionId(3);
    let mut entry = function(3);
    entry.environment = Some(ClosureId(2));
    module.functions.push(entry);
    module.closures.push(child);
    module.closures[0].captures = vec![Type::Closure(ClosureId(2))];
    both(&module, Some("complete self-contained captures"));
}

#[test]
fn companion_state_cannot_hide_another_callable_input() {
    let mut module = fixture(CallableKind::FnOnce, true);
    let state = Type::Package {
        name: "HiddenState".into(),
        fields: vec![
            (
                "owner".into(),
                Type::Managed(Box::new(module.resources[0].payload.clone())),
            ),
            ("callback".into(), Type::Closure(ClosureId(0))),
        ],
    };
    module.functions[2].parameters.push(LocalId(1));
    module.functions[2].locals.push(local(1, state));
    both(&module, Some("unsupported owning or borrowed child"));
}

#[test]
fn direct_store_companion_requires_the_existing_leaf_capture_profile() {
    for hidden_callback in [false, true] {
        let mut module = fixture(CallableKind::FnOnce, true);
        let mut fields = vec![(
            "owner".into(),
            Type::Managed(Box::new(module.resources[0].payload.clone())),
        )];
        if hidden_callback {
            fields.push(("callback".into(), Type::Closure(ClosureId(0))));
        }
        let element = Type::Package {
            name: "CompanionItem220".into(),
            fields,
        };
        let payload = Type::OwnedStore {
            element: Box::new(element),
        };
        module.resources.push(ResourceDefinition {
            name: "@builtin:OwnedStore<CompanionItem220>".into(),
            payload: payload.clone(),
            teardown: None,
        });
        let factory = &mut module.functions[2];
        factory.parameters.push(LocalId(1));
        factory
            .locals
            .push(local(1, Type::Managed(Box::new(payload))));
        factory.blocks[0].instructions.extend([
            instruction(None, InstructionKind::DropManaged { local: LocalId(1) }),
            instruction(None, InstructionKind::StorageDead(LocalId(1))),
        ]);
        both(
            &module,
            if hidden_callback {
                Some("captured Store Item")
            } else {
                None
            },
        );
    }
}

fn minimum(module: &TypedModule) -> usize {
    let (mut lo, mut hi) = (0, Limits::default().max_analysis_steps);
    assert!(analyze(module, &Limits::default()).is_ok());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let limits = Limits {
            max_analysis_steps: mid,
            ..Limits::default()
        };
        match analyze(module, &limits) {
            Ok(_) => hi = mid,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|e| e.category == ResultCategory::ResourceLimit),
                    "{errors:#?}"
                );
                lo = mid + 1;
            }
        }
    }
    lo
}

#[test]
fn raw_and_wire_replay_charge_exact_n_and_reject_one_less() {
    let module = fixture(CallableKind::FnOnce, true);
    let n = minimum(&module);
    assert!(n > 0);
    for input in [&module, &transported(&module)] {
        for steps in [n, n - 1, n] {
            let result = analyze(
                input,
                &Limits {
                    max_analysis_steps: steps,
                    ..Limits::default()
                },
            );
            if steps == n {
                result.unwrap();
            } else {
                assert!(
                    result
                        .unwrap_err()
                        .iter()
                        .all(|e| e.category == ResultCategory::ResourceLimit)
                );
            }
        }
    }
    eprintln!("synthetic Analyze220 exact budget N={n}");
}
