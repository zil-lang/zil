//! Factory results constructed directly as raw IR, without Types or Analyze.
use super::*;

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

fn both(m: &TypedModule) -> [CompileResult<VerifiedModule>; 2] {
    let plan = resource_tests::plan(m);
    let bytes = zil_ir::wire::encode_module_candidate(m, &plan, &Limits::default()).unwrap();
    [
        verify_module(m.clone(), plan, &m.target, &Limits::default()),
        verify_module_bytes(&bytes, &m.target, &Limits::default()),
    ]
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
