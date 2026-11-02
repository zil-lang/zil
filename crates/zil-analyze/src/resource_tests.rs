//! Independent producer diagnostics over raw IR; no Verify API is imported.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};

fn s() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn payload() -> Type {
    Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    }
}
fn managed() -> Type {
    Type::Managed(Box::new(payload()))
}
fn desc() -> Type {
    Type::Desc(Box::new(payload()))
}
fn inst(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: s(),
    }
}
struct Body(TypedFunction);
impl Body {
    fn new(id: u32) -> Self {
        Self(TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(id),
            name: format!("f{id}"),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![],
            values: vec![],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(None),
            }],
            entry: BlockId(0),
            span: s(),
        })
    }
    fn local(&mut self, ty: Type) -> LocalId {
        let id = LocalId(self.0.locals.len() as u32);
        self.0.locals.push(LocalDecl {
            id,
            name: format!("l{}", id.0),
            ty,
            mutable: true,
            span: s(),
        });
        self.op(InstructionKind::StorageLive(id));
        id
    }
    fn value(&mut self, ty: Type, kind: InstructionKind) -> ValueId {
        let id = ValueId(self.0.values.len() as u32);
        self.0.values.push(ty);
        self.0.blocks[0].instructions.push(inst(Some(id), kind));
        id
    }
    fn op(&mut self, kind: InstructionKind) {
        self.0.blocks[0].instructions.push(inst(None, kind));
    }
    fn constant(&mut self, n: i32) -> ValueId {
        self.value(Type::I32, InstructionKind::Constant(Constant::I32(n)))
    }
    fn finish(mut self, value: ValueId) -> TypedFunction {
        self.0.blocks[0].terminator = Terminator::Return(Some(value));
        self.0
    }
}
fn fixture(kind: CallableKind, borrowed: bool, count: usize) -> TypedModule {
    let mut main = Body::new(0);
    let mut captures = vec![];
    let mut owners = vec![];
    for index in 0..count {
        let local = main.local(managed());
        owners.push(local);
        let value = main.constant(index as i32);
        let owner = main.value(
            managed(),
            InstructionKind::MakeManaged {
                fields: vec![value],
            },
        );
        main.op(InstructionKind::Store {
            local,
            value: owner,
        });
        captures.push(if borrowed {
            main.value(desc(), InstructionKind::DescribeLocal { local })
        } else {
            main.value(managed(), InstructionKind::TakeLocal { local })
        });
    }
    let environment = main.local(Type::Closure(ClosureId(0)));
    let closure = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures,
        },
    );
    main.op(InstructionKind::Store {
        local: environment,
        value: closure,
    });
    let result = match kind {
        CallableKind::Fn => main.value(
            Type::I32,
            InstructionKind::CallClosureShared {
                callee: environment,
                arguments: vec![],
            },
        ),
        CallableKind::FnMut => main.value(
            Type::I32,
            InstructionKind::CallClosureMut {
                callee: environment,
                arguments: vec![],
            },
        ),
        CallableKind::FnOnce => {
            let value = main.value(
                Type::Closure(ClosureId(0)),
                InstructionKind::TakeLocal { local: environment },
            );
            main.value(
                Type::I32,
                InstructionKind::CallClosureOnce {
                    callee: value,
                    arguments: vec![],
                },
            )
        }
        _ => unreachable!(),
    };
    if !borrowed || kind == CallableKind::FnOnce {
        main.op(InstructionKind::DropClosure { local: environment });
    }
    main.op(InstructionKind::StorageDead(environment));
    for local in owners.into_iter().rev() {
        main.op(InstructionKind::DropManaged { local });
        main.op(InstructionKind::StorageDead(local));
    }
    let mut entry = Body::new(1);
    entry.0.environment = Some(ClosureId(0));
    let result_entry = if count == 0 {
        entry.constant(3)
    } else {
        let d = entry.value(
            desc(),
            if borrowed {
                InstructionKind::ReadCapture { index: 0 }
            } else {
                InstructionKind::DescribeCapture { index: 0 }
            },
        );
        entry.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 })
    };
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![main.finish(result), entry.finish(result_entry)],
        closures: vec![ClosureDefinition {
            lending_captures: Vec::new(),
            id: ClosureId(0),
            function: FunctionId(1),
            kind,
            captures: (0..count)
                .map(|_| if borrowed { desc() } else { managed() })
                .collect(),
            parameters: vec![],
            result: Type::I32,
        }],
        resources: vec![ResourceDefinition {
            name: "Token".into(),
            payload: payload(),
            teardown: None,
        }],
        unsupported: vec![],
    }
}
fn append_entry(module: &mut TypedModule, kind: InstructionKind) {
    module.functions[1].blocks[0]
        .instructions
        .push(inst(None, kind));
}
fn add_helper(module: &mut TypedModule, write: bool) {
    let mut helper = Body::new(2);
    helper.0.locals.push(LocalDecl {
        id: LocalId(0),
        name: "arg".into(),
        ty: desc(),
        mutable: false,
        span: s(),
    });
    helper.0.parameters.push(LocalId(0));
    let d = helper.value(desc(), InstructionKind::Load(LocalId(0)));
    let n = helper.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    if write {
        helper.op(InstructionKind::WriteDesc {
            desc: d,
            index: 0,
            value: n,
        });
    }
    module.functions.push(helper.finish(n));
    module.functions[1].blocks[0].instructions[1].kind = InstructionKind::Call {
        function: FunctionId(2),
        arguments: vec![ValueId(0)],
    };
}

#[test]
fn independent_resource_closure_flow_accepts_repeatable_and_consuming_environments() {
    for (kind, borrowed, count) in [
        (CallableKind::Fn, false, 2),
        (CallableKind::Fn, true, 2),
        (CallableKind::FnMut, false, 1),
        (CallableKind::FnOnce, false, 2),
        (CallableKind::FnOnce, false, 0),
    ] {
        analyze(&fixture(kind, borrowed, count), &Limits::default()).unwrap();
    }
}

fn owned_callback_fixture(kind: CallableKind, count: usize) -> TypedModule {
    let event = Type::Package {
        name: "Event".into(),
        fields: vec![("token".into(), managed())],
    };
    let mut main = Body::new(0);
    main.op(InstructionKind::ScopeEnter { scope: ScopeId(0) });
    let environment = main.local(Type::Closure(ClosureId(0)));
    let closure = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![],
        },
    );
    main.op(InstructionKind::Store {
        local: environment,
        value: closure,
    });
    for n in 0..count {
        let local = main.local(event.clone());
        let n = main.constant(n as i32);
        let token = main.value(managed(), InstructionKind::MakeManaged { fields: vec![n] });
        let argument = main.value(
            event.clone(),
            InstructionKind::MakePackage {
                fields: vec![token],
            },
        );
        let call = match kind {
            CallableKind::Fn => InstructionKind::CallClosureShared {
                callee: environment,
                arguments: vec![argument],
            },
            CallableKind::FnMut => InstructionKind::CallClosureMut {
                callee: environment,
                arguments: vec![argument],
            },
            CallableKind::FnOnce => {
                let callee = main.value(
                    Type::Closure(ClosureId(0)),
                    InstructionKind::TakeLocal { local: environment },
                );
                InstructionKind::CallClosureOnce {
                    callee,
                    arguments: vec![argument],
                }
            }
            _ => unreachable!(),
        };
        let result = main.value(event.clone(), call);
        main.op(InstructionKind::Store {
            local,
            value: result,
        });
        main.op(InstructionKind::DropPackage { local });
        main.op(InstructionKind::StorageDead(local));
    }
    if kind == CallableKind::FnOnce {
        main.op(InstructionKind::DropClosure { local: environment });
    }
    main.op(InstructionKind::StorageDead(environment));
    let zero = main.constant(0);
    main.op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    let mut entry = Body::new(1);
    entry.0.environment = Some(ClosureId(0));
    entry.0.return_type = event.clone();
    entry.0.locals.push(LocalDecl {
        id: LocalId(0),
        name: "event".into(),
        ty: event.clone(),
        mutable: false,
        span: s(),
    });
    entry.0.parameters.push(LocalId(0));
    entry.op(InstructionKind::ScopeEnter { scope: ScopeId(0) });
    let result = entry.value(
        event.clone(),
        InstructionKind::TakeLocal { local: LocalId(0) },
    );
    entry.op(InstructionKind::DropPackage { local: LocalId(0) });
    entry.op(InstructionKind::StorageDead(LocalId(0)));
    entry.op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    let mut m = fixture(kind, false, 0);
    m.functions = vec![main.finish(zero), entry.finish(result)];
    m.closures[0].parameters = vec![event.clone()];
    m.closures[0].result = event;
    m
}

#[test]
fn impl247_owned_callbacks_consume_fresh_arguments_and_return_incoming_owners() {
    for (kind, count) in [
        (CallableKind::Fn, 2),
        (CallableKind::FnMut, 2),
        (CallableKind::FnOnce, 1),
        (CallableKind::FnOnce, 0),
    ] {
        analyze(&owned_callback_fixture(kind, count), &Limits::default()).unwrap();
    }
}

#[test]
fn impl247_owned_callback_arguments_and_results_have_one_responsibility() {
    let m = owned_callback_fixture(CallableKind::Fn, 2);
    let mut duplicate = m.clone();
    let arguments: Vec<_> = duplicate.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .filter_map(|i| {
            if let InstructionKind::CallClosureShared { arguments, .. } = &mut i.kind {
                Some(arguments)
            } else {
                None
            }
        })
        .collect();
    let first = arguments[0][0];
    arguments.into_iter().nth(1).unwrap()[0] = first;
    assert!(analyze(&duplicate, &Limits::default()).is_err());
    let mut leak = m;
    leak.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropPackage { .. }));
    assert!(analyze(&leak, &Limits::default()).is_err());
}

#[test]
fn impl247_owned_callback_definition_and_actual_call_are_checked_independently() {
    let m = owned_callback_fixture(CallableKind::FnOnce, 1);
    let mut malformed = m.clone();
    malformed.closures[0].parameters[0] = Type::I32;
    assert!(analyze(&malformed, &Limits::default()).is_err());
    let mut malformed = m.clone();
    for i in &mut malformed.functions[0].blocks[0].instructions {
        if let InstructionKind::CallClosureOnce { arguments, .. } = &mut i.kind {
            arguments.clear();
        }
    }
    assert!(analyze(&malformed, &Limits::default()).is_err());
    let mut malformed = m;
    malformed.closures[0].parameters.push(Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    });
    assert!(analyze(&malformed, &Limits::default()).is_err());
    let mut immutable = owned_callback_fixture(CallableKind::FnMut, 1);
    immutable.functions[0].locals[0].mutable = false;
    assert!(analyze(&immutable, &Limits::default()).is_err());
}

#[test]
fn impl247_owned_callback_result_cannot_export_created_freshness() {
    let mut m = owned_callback_fixture(CallableKind::FnOnce, 1);
    // The callee really creates the returned leaf, rather than merely passing
    // an incoming parameter through. Its private Created fact stays private.
    let event = m.closures[0].result.clone();
    let mut entry = Body::new(1);
    entry.0.environment = Some(ClosureId(0));
    entry.0.return_type = event.clone();
    entry.0.locals.push(LocalDecl {
        id: LocalId(0),
        name: "event".into(),
        ty: event.clone(),
        mutable: false,
        span: s(),
    });
    entry.0.parameters.push(LocalId(0));
    entry.op(InstructionKind::ScopeEnter { scope: ScopeId(0) });
    entry.op(InstructionKind::DropPackage { local: LocalId(0) });
    entry.op(InstructionKind::StorageDead(LocalId(0)));
    let n = entry.constant(7);
    let fresh = entry.value(managed(), InstructionKind::MakeManaged { fields: vec![n] });
    let result = entry.value(
        event,
        InstructionKind::MakePackage {
            fields: vec![fresh],
        },
    );
    entry.op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    m.functions[1] = entry.finish(result);
    let f = &mut m.functions[0];
    let value = ValueId(f.values.len() as u32);
    f.values.push(managed());
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropPackage { .. }))
        .unwrap();
    f.blocks[0].instructions.insert(
        at,
        inst(
            Some(value),
            InstructionKind::TakePackageField {
                local: LocalId(1),
                index: 0,
            },
        ),
    );
    f.return_type = managed();
    f.blocks[0].terminator = Terminator::Return(Some(value));
    analyze(&m, &Limits::default()).unwrap();
    // Even if the input/callee internally constructed a resource, an ordinary
    // callback result is Incoming at this boundary, never a create promise.
    m.functions[0].result_mode = PassingMode::Create;
    assert!(analyze(&m, &Limits::default()).is_err());
}

#[test]
fn impl247_owned_callback_budget_is_module_scoped_and_recovers_fresh() {
    let m = owned_callback_fixture(CallableKind::FnMut, 2);
    let mut budget = Budget::new(&Limits::default());
    let expected = analyze_budgeted(&m, &Limits::default(), &mut budget, None).unwrap();
    let exact = Limits::default().max_analysis_steps - budget.remaining();
    println!("impl247 owned callback Analyze N={exact}; N-1 ResourceLimit; fresh equality");
    let mut limits = Limits {
        max_analysis_steps: exact,
        ..Limits::default()
    };
    assert_eq!(analyze(&m, &limits).unwrap(), expected);
    limits.max_analysis_steps -= 1;
    assert!(
        analyze(&m, &limits)
            .unwrap_err()
            .iter()
            .any(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    assert_eq!(analyze(&m, &Limits::default()).unwrap(), expected);
}

#[test]
fn impl247_callback_data_cannot_smuggle_a_callback_state_through_store() {
    let mut m = owned_callback_fixture(CallableKind::FnOnce, 1);
    let state = Type::Package {
        name: "State".into(),
        fields: vec![
            ("token".into(), managed()),
            ("callback".into(), Type::Closure(ClosureId(1))),
        ],
    };
    let store = Type::OwnedStore {
        element: Box::new(state),
    };
    let data = Type::Package {
        name: "Event".into(),
        fields: vec![("items".into(), Type::Managed(Box::new(store.clone())))],
    };
    for f in &mut m.functions {
        for local in &mut f.locals {
            if matches!(local.ty, Type::Package { .. }) {
                local.ty = data.clone();
            }
        }
        for ty in &mut f.values {
            if matches!(ty, Type::Package { .. }) {
                *ty = data.clone();
            }
        }
        if matches!(f.return_type, Type::Package { .. }) {
            f.return_type = data.clone();
        }
    }
    m.closures[0].parameters = vec![data.clone()];
    m.closures[0].result = data;
    let mut child = Body::new(2);
    child.0.environment = Some(ClosureId(1));
    let n = child.constant(0);
    m.functions.push(child.finish(n));
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(2),
        captures: vec![],
        parameters: vec![],
        result: Type::I32,
        kind: CallableKind::Fn,
        lending_captures: vec![],
    });
    m.resources.push(ResourceDefinition {
        name: "@builtin:OwnedStore<State>".into(),
        payload: store,
        teardown: None,
    });
    let limits = Limits::default();
    let bytes =
        zil_ir::wire::encode_module_candidate(&m, &CandidatePlan::empty_for(&m), &limits).unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
        .unwrap()
        .0;
    for raw in [&m, &decoded] {
        let errors = analyze(raw, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("cannot hide an environment")),
            "{errors:?}"
        );
    }
}
#[test]
fn independent_resource_diagnostics_reject_copy_leaks_and_repeated_cleanup() {
    let mut m = fixture(CallableKind::Fn, false, 1);
    m.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropClosure { .. }));
    assert!(analyze(&m, &Limits::default()).is_err());
    let mut m = fixture(CallableKind::Fn, false, 1);
    m.functions[1].blocks[0].instructions[0].kind = InstructionKind::ReadCapture { index: 0 };
    assert!(analyze(&m, &Limits::default()).is_err());
    let mut m = fixture(CallableKind::Fn, false, 1);
    let at = m.functions[0].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::DropClosure { .. }))
        .unwrap();
    m.functions[0].blocks[0].instructions.insert(
        at,
        inst(None, InstructionKind::DropClosure { local: LocalId(1) }),
    );
    assert!(analyze(&m, &Limits::default()).is_err());
}
#[test]
fn independent_effects_distinguish_borrowed_target_and_owned_environment_writes() {
    for (kind, borrowed, accepted) in [
        (CallableKind::Fn, false, false),
        (CallableKind::FnMut, false, true),
        (CallableKind::Fn, true, true),
    ] {
        let mut m = fixture(kind, borrowed, 1);
        append_entry(
            &mut m,
            InstructionKind::WriteDesc {
                desc: ValueId(0),
                index: 0,
                value: ValueId(1),
            },
        );
        assert_eq!(analyze(&m, &Limits::default()).is_ok(), accepted);
    }
}
#[test]
fn independent_effect_fixed_point_follows_direct_and_forwarding_calls() {
    for (write, kind, borrowed, accepted) in [
        (false, CallableKind::Fn, false, true),
        (true, CallableKind::Fn, false, false),
        (true, CallableKind::FnMut, false, true),
        (true, CallableKind::Fn, true, true),
    ] {
        let mut m = fixture(kind, borrowed, 1);
        add_helper(&mut m, write);
        assert_eq!(analyze(&m, &Limits::default()).is_ok(), accepted);
        let mut forwarded = m.functions[2].clone();
        forwarded.id = FunctionId(3);
        forwarded.name = "forwarded".into();
        m.functions.push(forwarded);
        m.functions[2].blocks[0].instructions.truncate(2);
        m.functions[2].blocks[0].instructions[1].kind = InstructionKind::Call {
            function: FunctionId(3),
            arguments: vec![ValueId(0)],
        };
        assert_eq!(analyze(&m, &Limits::default()).is_ok(), accepted);
    }
}

#[test]
fn initialization_reuse_does_not_cache_callee_effect_safety() {
    let limits = Limits::default();
    let mut module = fixture(CallableKind::FnMut, false, 1);
    add_helper(&mut module, true);
    let mut session = AnalysisSession::default();
    session.analyze(&module, &limits).unwrap();
    module.closures[0].kind = CallableKind::Fn;
    for instruction in &mut module.functions[0].blocks[0].instructions {
        if let InstructionKind::CallClosureMut { callee, arguments } = &instruction.kind {
            instruction.kind = InstructionKind::CallClosureShared {
                callee: *callee,
                arguments: arguments.clone(),
            };
        }
    }
    let cold = analyze(&module, &limits);
    assert!(cold.is_err());
    assert_eq!(session.analyze(&module, &limits), cold);
    assert_eq!(
        session.last_stats().reused_functions,
        module.functions.len()
    );
    assert_eq!(session.last_stats().executed_functions, 0);
}
#[test]
fn independent_once_state_consumes_captures_and_rejects_reuse() {
    let mut m = fixture(CallableKind::FnOnce, false, 2);
    let mut entry = Body(m.functions[1].clone());
    let place = entry.local(managed());
    let moved = entry.value(managed(), InstructionKind::TakeCapture { index: 0 });
    entry.op(InstructionKind::Store {
        local: place,
        value: moved,
    });
    entry.op(InstructionKind::DropManaged { local: place });
    entry.op(InstructionKind::StorageDead(place));
    m.functions[1] = entry.0;
    analyze(&m, &Limits::default()).unwrap();
    m.functions[1].values.push(managed());
    m.functions[1].blocks[0].instructions.push(inst(
        Some(ValueId(3)),
        InstructionKind::TakeCapture { index: 0 },
    ));
    assert!(analyze(&m, &Limits::default()).is_err());
}
#[test]
fn independent_sources_reject_stale_borrowed_capture_and_repeated_consuming_ssa() {
    let mut m = fixture(CallableKind::Fn, true, 1);
    let at = m.functions[0].blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CallClosureShared { .. }))
        .unwrap();
    m.functions[0].blocks[0].instructions.insert(
        at,
        inst(None, InstructionKind::DropManaged { local: LocalId(0) }),
    );
    assert!(analyze(&m, &Limits::default()).is_err());
    let mut m = fixture(CallableKind::FnOnce, false, 0);
    let f = &mut m.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CallClosureOnce { .. }))
        .unwrap();
    let mut again = f.blocks[0].instructions[at].clone();
    again.result = Some(ValueId(f.values.len() as u32));
    f.values.push(Type::I32);
    f.blocks[0].instructions.insert(at + 1, again);
    assert!(analyze(&m, &Limits::default()).is_err());
}
