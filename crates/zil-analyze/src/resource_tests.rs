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
