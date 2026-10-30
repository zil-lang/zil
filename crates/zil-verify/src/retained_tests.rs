//! Factory results constructed directly as raw IR, without Types or Analyze.
use super::*;
use zil_base::ResultCategory;

fn op(result: Option<ValueId>, kind: InstructionKind, span: Span) -> Instruction {
    Instruction { result, kind, span }
}

pub(super) fn fixture(kind: CallableKind, captures: usize) -> TypedModule {
    let mut m = resource_closure_tests::fixture(kind, false, captures);
    let mut factory = m.functions[0].clone();
    let span = factory.span;
    let at = factory.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    let result = factory.blocks[0].instructions[at].result.unwrap();
    factory.blocks[0].instructions.truncate(at + 1);
    factory.blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::StorageLive(l) if l.0 as usize == captures));
    factory.locals.truncate(captures);
    factory.values.truncate(result.0 as usize + 1);
    for n in (0..captures).rev() {
        factory.blocks[0].instructions.push(op(
            None,
            InstructionKind::DropManaged {
                local: LocalId(n as u32),
            },
            span,
        ));
        factory.blocks[0].instructions.push(op(
            None,
            InstructionKind::StorageDead(LocalId(n as u32)),
            span,
        ));
    }
    factory.blocks[0].terminator = Terminator::Return(Some(result));
    factory.id = FunctionId(2);
    factory.name = "factory".into();
    factory.return_type = Type::Closure(ClosureId(0));
    factory.callable_return_contract = Some(CallableReturnContract {
        lending_captures: Vec::new(),
        closure: ClosureId(0),
        kind,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: (0..captures as u32).collect(),
    });
    let mut main = m.functions[0].clone();
    main.locals = vec![LocalDecl {
        id: LocalId(0),
        name: "saved".into(),
        ty: Type::Closure(ClosureId(0)),
        mutable: true,
        span,
    }];
    main.values = vec![Type::Closure(ClosureId(0))];
    let mut body = vec![
        op(None, InstructionKind::StorageLive(LocalId(0)), span),
        op(
            Some(ValueId(0)),
            InstructionKind::Call {
                function: FunctionId(2),
                arguments: vec![],
            },
            span,
        ),
        op(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(0),
            },
            span,
        ),
    ];
    let call = match kind {
        CallableKind::Fn => InstructionKind::CallClosureShared {
            callee: LocalId(0),
            arguments: vec![],
        },
        CallableKind::FnMut => InstructionKind::CallClosureMut {
            callee: LocalId(0),
            arguments: vec![],
        },
        CallableKind::FnOnce => {
            main.values.push(Type::Closure(ClosureId(0)));
            body.push(op(
                Some(ValueId(1)),
                InstructionKind::TakeLocal { local: LocalId(0) },
                span,
            ));
            InstructionKind::CallClosureOnce {
                callee: ValueId(1),
                arguments: vec![],
            }
        }
        _ => unreachable!(),
    };
    let call_result = ValueId(main.values.len() as u32);
    main.values.push(Type::I32);
    body.push(op(Some(call_result), call, span));
    if captures != 0 || kind == CallableKind::FnOnce {
        body.push(op(
            None,
            InstructionKind::DropClosure { local: LocalId(0) },
            span,
        ));
    }
    body.push(op(None, InstructionKind::StorageDead(LocalId(0)), span));
    main.blocks[0].instructions = body;
    main.blocks[0].terminator = Terminator::Return(Some(call_result));
    m.functions[0] = main;
    m.functions.push(factory);
    m
}

fn fixture_plan(m: &TypedModule) -> CandidatePlan {
    let mut plan = resource_tests::plan(m);
    // These are explicit fixture claims, not a call to the borrow proof. The
    // independent checker must still recover the actual environment address.
    for (f, claims) in m.functions.iter().zip(&mut plan.functions) {
        for block in &f.blocks {
            for instruction in &block.instructions {
                if let InstructionKind::BorrowCapture { index, .. } = instruction.kind {
                    claims.holds[instruction.result.unwrap().0 as usize].roots =
                        vec![HoldRoot::Capture(index)];
                }
            }
        }
    }
    plan
}

fn both(m: &TypedModule) -> [CompileResult<VerifiedModule>; 2] {
    let plan = fixture_plan(m);
    let bytes = zil_ir::wire::encode_module_candidate(m, &plan, &Limits::default()).unwrap();
    [
        verify_module(m.clone(), plan, &m.target, &Limits::default()),
        verify_module_bytes(&bytes, &m.target, &Limits::default()),
    ]
}

// 221: construct the ordinary owning boundary directly. A hand-built public
// candidate is intentionally weaker evidence than a source/native consumer.
fn empty_function(template: &TypedFunction, id: u32, result: Type) -> TypedFunction {
    let mut f = template.clone();
    f.id = FunctionId(id);
    f.name = format!("owning_{id}");
    f.environment = None;
    f.callable_contract = None;
    f.callable_return_contract = None;
    f.parameters.clear();
    f.locals.clear();
    f.values.clear();
    f.return_type = result;
    f.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![],
        terminator: Terminator::Trap,
    }];
    f
}

fn value(f: &mut TypedFunction, ty: Type, kind: InstructionKind) -> ValueId {
    let id = ValueId(f.values.len() as u32);
    f.values.push(ty);
    f.blocks[0].instructions.push(op(Some(id), kind, f.span));
    id
}

fn effect(f: &mut TypedFunction, kind: InstructionKind) {
    f.blocks[0].instructions.push(op(None, kind, f.span));
}

fn add_local(f: &mut TypedFunction, ty: Type, parameter: bool) -> LocalId {
    let id = LocalId(f.locals.len() as u32);
    f.locals.push(LocalDecl {
        id,
        name: format!("l{}", id.0),
        ty,
        mutable: true,
        span: f.span,
    });
    if parameter {
        f.parameters.push(id);
    } else {
        effect(f, InstructionKind::StorageLive(id));
    }
    id
}

fn owning_fixture(kind: CallableKind, retain_input: bool) -> TypedModule {
    let mut m = fixture(kind, 1);
    let input = Type::Closure(ClosureId(0));
    let output = Type::Closure(ClosureId(1));
    let mut entry = empty_function(&m.functions[1], 3, Type::I32);
    entry.environment = Some(ClosureId(1));
    let result = if retain_input {
        let call = if kind == CallableKind::FnOnce {
            let callee = value(
                &mut entry,
                input.clone(),
                InstructionKind::TakeCapture { index: 0 },
            );
            InstructionKind::CallClosureOnce {
                callee,
                arguments: vec![],
            }
        } else {
            let mutable = kind == CallableKind::FnMut;
            let callee = value(
                &mut entry,
                Type::Address {
                    mutable,
                    inner: Box::new(input.clone()),
                },
                InstructionKind::BorrowCapture { index: 0, mutable },
            );
            InstructionKind::CallClosureBorrowed {
                callee,
                kind,
                arguments: vec![],
            }
        };
        value(&mut entry, Type::I32, call)
    } else {
        value(
            &mut entry,
            Type::I32,
            InstructionKind::Constant(Constant::I32(7)),
        )
    };
    entry.blocks[0].terminator = Terminator::Return(Some(result));
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(3),
        captures: if retain_input {
            vec![input.clone()]
        } else {
            vec![]
        },
        parameters: vec![],
        result: Type::I32,
        kind,
        lending_captures: vec![],
    });
    m.functions.push(entry);

    let mut factory = empty_function(&m.functions[2], 4, output.clone());
    let parameter = add_local(&mut factory, input.clone(), true);
    let captures = if retain_input {
        vec![value(
            &mut factory,
            input.clone(),
            InstructionKind::TakeLocal { local: parameter },
        )]
    } else {
        vec![]
    };
    let returned = value(
        &mut factory,
        output.clone(),
        InstructionKind::MakeClosure {
            closure: ClosureId(1),
            captures,
        },
    );
    effect(
        &mut factory,
        InstructionKind::DropClosure { local: parameter },
    );
    factory.blocks[0].terminator = Terminator::Return(Some(returned));
    factory.callable_return_contract = Some(CallableReturnContract {
        closure: ClosureId(1),
        kind,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: if retain_input { vec![0] } else { vec![] },
        lending_captures: vec![],
    });
    m.functions.push(factory);

    let mut main = empty_function(&m.functions[0], 0, Type::I32);
    let child = add_local(&mut main, input.clone(), false);
    let wrapper = add_local(&mut main, output.clone(), false);
    let created = value(
        &mut main,
        input.clone(),
        InstructionKind::Call {
            function: FunctionId(2),
            arguments: vec![],
        },
    );
    effect(
        &mut main,
        InstructionKind::Store {
            local: child,
            value: created,
        },
    );
    let taken = value(
        &mut main,
        input,
        InstructionKind::TakeLocal { local: child },
    );
    let wrapped = value(
        &mut main,
        output.clone(),
        InstructionKind::Call {
            function: FunctionId(4),
            arguments: vec![taken],
        },
    );
    effect(
        &mut main,
        InstructionKind::Store {
            local: wrapper,
            value: wrapped,
        },
    );
    let call = match kind {
        CallableKind::Fn => InstructionKind::CallClosureShared {
            callee: wrapper,
            arguments: vec![],
        },
        CallableKind::FnMut => InstructionKind::CallClosureMut {
            callee: wrapper,
            arguments: vec![],
        },
        CallableKind::FnOnce => {
            let callee = value(
                &mut main,
                output,
                InstructionKind::TakeLocal { local: wrapper },
            );
            InstructionKind::CallClosureOnce {
                callee,
                arguments: vec![],
            }
        }
        _ => unreachable!(),
    };
    let result = value(&mut main, Type::I32, call);
    if retain_input || kind == CallableKind::FnOnce {
        effect(&mut main, InstructionKind::DropClosure { local: wrapper });
    }
    effect(&mut main, InstructionKind::StorageDead(wrapper));
    effect(&mut main, InstructionKind::DropClosure { local: child });
    effect(&mut main, InstructionKind::StorageDead(child));
    main.blocks[0].terminator = Terminator::Return(Some(result));
    m.functions[0] = main;
    m
}

#[test]
fn owning_factory_raw_and_wire_transfer_or_clean_input_without_helper_contract() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        for retain_input in [false, true] {
            let m = owning_fixture(kind, retain_input);
            for result in both(&m) {
                let checked = result.unwrap_or_else(|e| panic!("{kind:?}/{retain_input}: {e:?}"));
                assert_eq!(
                    checked.callable_return_contract(FunctionId(4)),
                    m.functions[4].callable_return_contract.as_ref()
                );
                assert!(m.functions[4].callable_contract.is_none());
            }
        }
    }
}

#[test]
fn owning_factory_raw_and_wire_reject_same_result_identity_or_extra_input() {
    let base = owning_fixture(CallableKind::Fn, true);
    let mut m = base.clone();
    m.functions[4].return_type = Type::Closure(ClosureId(0));
    m.functions[4]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .closure = ClosureId(0);
    reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
    let mut m = base.clone();
    add_local(&mut m.functions[4], Type::Closure(ClosureId(0)), true);
    reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
    let mut m = base.clone();
    m.functions[4].parameters.push(LocalId(0));
    reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
    let mut m = base;
    m.functions[4].locals[0].ty = Type::Address {
        mutable: false,
        inner: Box::new(Type::Closure(ClosureId(0))),
    };
    reject_both(&m, "E-UNSUPPORTED");
}

#[test]
fn owning_factory_raw_and_wire_require_exact_return_contract() {
    let base = owning_fixture(CallableKind::Fn, true);
    let mut m = base.clone();
    m.functions[4].callable_return_contract = None;
    reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
    for mutation in 0..5 {
        let mut m = base.clone();
        let c = m.functions[4].callable_return_contract.as_mut().unwrap();
        match mutation {
            0 => c.kind = CallableKind::FnOnce,
            1 => c.cleanup.clear(),
            2 => c.cleanup.push(0),
            3 => c.hold.push(0),
            _ => c.call.push(0),
        }
        reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
    }
    let mut m = base;
    m.functions[4].callable_contract = Some(CallableParameterContract {
        roles: vec![],
        traces: CallableTraces::Finite(vec![]),
    });
    reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
}

#[test]
fn owning_factory_raw_and_wire_keep_per_value_responsibility() {
    let mut m = owning_fixture(CallableKind::FnOnce, true);
    m.closures[1].captures.push(Type::Closure(ClosureId(0)));
    m.functions[4]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup
        .push(1);
    let creation = m.functions[4].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    let InstructionKind::MakeClosure { captures, .. } = &mut creation.kind else {
        unreachable!()
    };
    captures.push(captures[0]);
    reject_both(&m, "E-VERIFY-RESOURCE");

    let mut m = owning_fixture(CallableKind::FnOnce, false);
    m.functions[4].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropClosure { .. }));
    reject_both(&m, "E-VERIFY-RESOURCE");

    let mut m = owning_fixture(CallableKind::FnOnce, true);
    m.functions[3].blocks[0].instructions[0].kind = InstructionKind::ReadCapture { index: 0 };
    reject_both(&m, "E-VERIFY-CAPTURE");
}

#[test]
fn owning_factory_raw_and_wire_reject_recursive_or_deep_borrowed_inputs() {
    let mut m = owning_fixture(CallableKind::Fn, true);
    m.closures[0].captures = vec![Type::Closure(ClosureId(1))];
    reject_both(&m, "E-VERIFY-CLOSURE-LAYOUT");

    let mut m = owning_fixture(CallableKind::Fn, true);
    let Type::Managed(payload) = m.closures[0].captures[0].clone() else {
        unreachable!()
    };
    m.closures[0].captures = vec![Type::Closure(ClosureId(2))];
    let mut child = m.closures[0].clone();
    child.id = ClosureId(2);
    child.function = FunctionId(5);
    child.captures = vec![Type::Desc(payload)];
    let mut entry = m.functions[1].clone();
    entry.id = FunctionId(5);
    entry.environment = Some(ClosureId(2));
    m.functions.push(entry);
    m.closures.push(child);
    reject_both(&m, "E-UNSUPPORTED");
}

#[test]
fn owning_factory_raw_and_wire_reject_hidden_companion_callback() {
    let mut m = owning_fixture(CallableKind::Fn, true);
    add_local(
        &mut m.functions[4],
        Type::Package {
            name: "HiddenState".into(),
            fields: vec![("f".into(), Type::Closure(ClosureId(0)))],
        },
        true,
    );
    reject_both(&m, "E-UNSUPPORTED");
}

fn store_companion(callback: bool) -> TypedModule {
    let mut m = owning_fixture(CallableKind::Fn, true);
    let mut fields = vec![("token".into(), m.closures[0].captures[0].clone())];
    if callback {
        fields.push(("callback".into(), Type::Closure(ClosureId(0))));
    }
    let element = Type::Package {
        name: "CompanionItem".into(),
        fields,
    };
    let payload = Type::OwnedStore {
        element: Box::new(element.clone()),
    };
    let carrier = Type::Managed(Box::new(payload.clone()));
    m.resources.push(ResourceDefinition {
        name: dynamic_store::definition_name(&element),
        payload,
        teardown: None,
    });
    let local = add_local(&mut m.functions[4], carrier.clone(), true);
    effect(&mut m.functions[4], InstructionKind::DropManaged { local });
    let main = &mut m.functions[0];
    let value = ValueId(main.values.len() as u32);
    main.values.push(carrier);
    let at = main.blocks[0]
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                InstructionKind::Call {
                    function: FunctionId(4),
                    ..
                }
            )
        })
        .unwrap();
    let InstructionKind::Call { arguments, .. } = &mut main.blocks[0].instructions[at].kind else {
        unreachable!()
    };
    arguments.push(value);
    main.blocks[0].instructions.insert(
        at,
        op(
            Some(value),
            InstructionKind::OwnedStoreNew { element },
            main.span,
        ),
    );
    m
}

#[test]
fn owning_factory_raw_and_wire_direct_store_companion_cannot_hide_another_callback() {
    for result in both(&store_companion(false)) {
        result.unwrap();
    }
    reject_both(&store_companion(true), "E-UNSUPPORTED");
}

#[test]
fn owning_factory_raw_and_wire_input_mode_can_be_consuming_adaptation() {
    for actual in [CallableKind::Fn, CallableKind::FnMut] {
        let mut m = owning_fixture(CallableKind::FnOnce, true);
        m.closures[0].kind = actual;
        m.functions[2]
            .callable_return_contract
            .as_mut()
            .unwrap()
            .kind = actual;
        for result in both(&m) {
            result.unwrap();
        }
    }
}

#[test]
fn owning_factory_raw_and_wire_do_not_open_root_lending_results() {
    for input in [false, true] {
        let mut m = owning_fixture(CallableKind::Fn, true);
        let (closure, entry, factory) = if input { (0, 1, 2) } else { (1, 3, 4) };
        let result = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        m.closures[closure].captures = vec![Type::I32];
        m.closures[closure].result = result.clone();
        m.closures[closure].lending_captures = vec![0];
        m.functions[entry].return_type = result;
        let c = m.functions[factory]
            .callable_return_contract
            .as_mut()
            .unwrap();
        c.lending_captures = vec![0];
        c.cleanup.clear();
        reject_both(&m, "E-UNSUPPORTED");
    }
}

#[test]
fn owning_factory_verify_and_wire_have_exact_fresh_budget_boundaries() {
    let m = owning_fixture(CallableKind::FnOnce, true);
    let plan = fixture_plan(&m);
    let bytes = zil_ir::wire::encode_module_candidate(&m, &plan, &Limits::default()).unwrap();
    for wire in [false, true] {
        let run = |n| {
            let limits = Limits {
                max_analysis_steps: n,
                ..Limits::default()
            };
            if wire {
                verify_module_bytes(&bytes, &m.target, &limits)
            } else {
                verify_module(m.clone(), plan.clone(), &m.target, &limits)
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap().fingerprint();
        while low + 1 < high {
            let n = low + (high - low) / 2;
            match run(n) {
                Ok(checked) => {
                    assert_eq!(checked.fingerprint(), expected);
                    high = n;
                }
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.category == ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = n;
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
        println!("owning221 wire={wire} N={high}; N-1=ResourceLimit; fresh N=Pass");
    }
}
fn reject_both(m: &TypedModule, code: &str) {
    for outcome in both(m) {
        let errors = outcome.unwrap_err();
        assert!(
            errors.iter().any(|e| e.code == code),
            "wanted {code}: {errors:?}"
        );
    }
}

#[test]
fn raw_and_wire_factories_transfer_owned_and_empty_environments() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        for count in [0, 1, 2] {
            let m = fixture(kind, count);
            for result in both(&m) {
                let checked = result.unwrap();
                assert_eq!(
                    checked.callable_return_contract(FunctionId(2)),
                    m.functions[2].callable_return_contract.as_ref()
                );
                assert!(checked.callable_return_contract(FunctionId(0)).is_none());
            }
        }
    }
}

#[test]
fn raw_return_contract_cannot_forge_identity_kind_dependencies_or_cleanup() {
    let base = fixture(CallableKind::Fn, 2);
    let mut m = base.clone();
    m.functions[2].callable_return_contract = None;
    reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
    for mutation in 0..10 {
        let mut m = base.clone();
        let c = m.functions[2].callable_return_contract.as_mut().unwrap();
        match mutation {
            0 => c.closure = ClosureId(99),
            1 => c.kind = CallableKind::FnOnce,
            2 => c.hold.push(0),
            3 => c.call.push(0),
            4 => c.writes.push(0),
            5 => c.cleanup.clear(),
            6 => c.cleanup = vec![0, 0],
            7 => c.cleanup = vec![1, 0],
            8 => c.cleanup = vec![0, 2],
            _ => c.cleanup.push(1),
        }
        reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
    }
    let mut m = base.clone();
    m.functions[0].callable_return_contract = m.functions[2].callable_return_contract.clone();
    reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
    let mut m = base;
    m.functions[2].result_mode = PassingMode::Create;
    reject_both(&m, "E-VERIFY-RESOURCE");
}

#[test]
fn raw_factory_bodies_keep_exact_contracts_when_hidden_identity_is_shared() {
    let base = fixture(CallableKind::Fn, 1);
    let mut m = base.clone();
    let mut other = m.functions[2].clone();
    other.id = FunctionId(3);
    other.name = "other".into();
    m.functions.push(other);
    for result in both(&m) {
        let verified = result.unwrap();
        assert_eq!(
            verified.callable_return_contract(FunctionId(2)),
            verified.callable_return_contract(FunctionId(3))
        );
    }
    let mut m = base.clone();
    let f = &mut m.functions[2];
    let create = f.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    create.kind = InstructionKind::Call {
        function: FunctionId(2),
        arguments: vec![],
    };
    reject_both(&m, "E-VERIFY-CALLABLE-CONTRACT");
    let mut m = base;
    m.functions[2].environment = Some(ClosureId(0));
    reject_both(&m, "E-VERIFY-CLOSURE-ENTRY");
}

#[test]
fn raw_self_contained_contract_rejects_borrowed_captures() {
    let mut m = fixture(CallableKind::Fn, 1);
    let Type::Managed(inner) = m.closures[0].captures[0].clone() else {
        unreachable!()
    };
    m.closures[0].captures[0] = Type::Desc(inner);
    m.functions[2]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup
        .clear();
    reject_both(&m, "E-UNSUPPORTED");
}

#[test]
fn raw_factory_cannot_hide_external_input_effects_behind_empty_contract() {
    for ty in [
        Type::Address {
            mutable: true,
            inner: Box::new(Type::I32),
        },
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        },
        Type::Desc(Box::new(Type::Record {
            name: "Token".into(),
            fields: vec![("n".into(), Type::I32)],
        })),
    ] {
        let mut m = fixture(CallableKind::Fn, 1);
        let f = &mut m.functions[2];
        let id = LocalId(f.locals.len() as u32);
        f.parameters.push(id);
        f.locals.push(LocalDecl {
            id,
            name: "external".into(),
            ty,
            mutable: false,
            span: f.span,
        });
        reject_both(&m, "E-UNSUPPORTED");
    }
}

#[test]
fn raw_factory_resource_multiplicity_and_caller_cleanup_are_not_trusted() {
    let mut m = fixture(CallableKind::Fn, 2);
    let make = m.functions[2].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    let InstructionKind::MakeClosure { captures, .. } = &mut make.kind else {
        unreachable!()
    };
    captures[1] = captures[0];
    reject_both(&m, "E-VERIFY-RESOURCE");
    let mut m = fixture(CallableKind::Fn, 1);
    m.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropClosure { .. }));
    reject_both(&m, "E-VERIFY-RESOURCE");
}

#[test]
fn callable_return_contract_is_bounded_and_changes_all_candidate_identity() {
    let base = fixture(CallableKind::Fn, 1);
    let initial = base.fingerprint();
    let contract = base.functions[2].callable_return_contract.as_ref().unwrap();
    let contract_fp = callable_return_contract_fingerprint(contract);
    for mutation in 0..6 {
        let mut m = base.clone();
        let c = m.functions[2].callable_return_contract.as_mut().unwrap();
        match mutation {
            0 => c.closure = ClosureId(1),
            1 => c.kind = CallableKind::FnMut,
            2 => c.hold.push(0),
            3 => c.call.push(0),
            4 => c.writes.push(0),
            _ => c.cleanup.clear(),
        }
        assert_ne!(callable_return_contract_fingerprint(c), contract_fp);
        assert_ne!(m.fingerprint(), initial);
    }
    let mut m = base;
    m.functions[2]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup = vec![0; 10];
    let limits = Limits {
        max_locals: 4,
        ..Limits::default()
    };
    let errors =
        verify_module(m.clone(), resource_tests::plan(&m), &m.target, &limits).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == "E-RESOURCE-LIMIT"),
        "{errors:?}"
    );
    let errors =
        zil_ir::wire::encode_module_candidate(&m, &resource_tests::plan(&m), &limits).unwrap_err();
    assert!(
        errors.iter().any(|e| e.code == "E-RESOURCE-LIMIT"),
        "{errors:?}"
    );
}

#[test]
fn current_wire_accepts_current_version_and_rejects_other_versions_and_every_truncation() {
    let m = fixture(CallableKind::FnOnce, 1);
    let bytes =
        zil_ir::wire::encode_module_candidate(&m, &resource_tests::plan(&m), &Limits::default())
            .unwrap();
    // The encoder emits the shared protocol constant; stale versions remain rejected below.
    assert_eq!(&bytes[8..12], &zil_ir::wire::WIRE_VERSION.to_le_bytes());
    verify_module_bytes(&bytes, &m.target, &Limits::default()).unwrap();
    let current = zil_ir::wire::WIRE_VERSION;
    for version in (0..current).chain([current + 1, u32::MAX]) {
        let mut changed = bytes.clone();
        changed[8..12].copy_from_slice(&version.to_le_bytes());
        let errors = verify_module_bytes(&changed, &m.target, &Limits::default()).unwrap_err();
        assert_eq!(errors[0].code, "E-WIRE-VERSION", "version {version}");
    }
    for end in 0..bytes.len() {
        assert!(
            verify_module_bytes(&bytes[..end], &m.target, &Limits::default()).is_err(),
            "prefix {end}"
        );
    }
}
