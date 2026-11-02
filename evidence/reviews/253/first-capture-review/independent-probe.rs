use zil_ir::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile, Limits};
use zil_analyze::analyze;
fn s() -> Span {
    Span::new(FileId(0), 0, 1)
}
fn op(result: Option<ValueId>, kind: InstructionKind) -> Instruction {
    Instruction {
        result,
        kind,
        span: s(),
    }
}
fn local(id: u32, ty: Type) -> LocalDecl {
    LocalDecl {
        id: LocalId(id),
        name: format!("l{id}"),
        ty,
        mutable: true,
        span: s(),
    }
}
fn block(id: u32, instructions: Vec<Instruction>, terminator: Terminator) -> BasicBlock {
    BasicBlock {
        id: BlockId(id),
        instructions,
        terminator,
    }
}
fn enter() -> Instruction {
    op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) })
}
fn exit() -> Vec<Instruction> {
    vec![
        op(None, InstructionKind::StorageDead(LocalId(1))),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ]
}
fn base(kind: CallableKind) -> TypedModule {
    let closure = Type::Closure(ClosureId(0));
    let callback = TypedFunction {
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(0),
        name: "callback".into(),
        foreign: None,
        environment: Some(ClosureId(0)),
        parameters: vec![],
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: vec![],
        values: vec![Type::I32],
        blocks: vec![block(
            0,
            vec![op(
                Some(ValueId(0)),
                InstructionKind::Constant(Constant::I32(7)),
            )],
            Terminator::Return(Some(ValueId(0))),
        )],
        entry: BlockId(0),
        span: s(),
    };
    let mut instructions = vec![
        enter(),
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        ),
    ];
    instructions.extend(exit());
    let keep = TypedFunction {
        callable_return_contract: Some(CallableReturnContract {
            source: CallableReturnSource::Parameter(1),
            closure: ClosureId(0),
            kind,
            hold: vec![],
            call: vec![],
            writes: vec![],
            cleanup: vec![],
            lending_captures: vec![],
        }),
        callable_contract: Some(CallableParameterContract {
            roles: vec![CallableRole {
                parameter: 1,
                closure: ClosureId(0),
                kind,
                hold: vec![],
                call: vec![],
                writes: vec![],
                cleanup: CallableCleanup::Returned,
            }],
            traces: CallableTraces::Finite(vec![CallableTrace {
                calls: vec![],
                exit: CallableExit::Return,
            }]),
        }),
        id: FunctionId(1),
        name: "keep".into(),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0), LocalId(1)],
        return_type: closure.clone(),
        result_mode: PassingMode::Infer,
        locals: vec![local(0, Type::Bool), local(1, closure.clone())],
        values: vec![closure],
        blocks: vec![block(0, instructions, Terminator::Return(Some(ValueId(0))))],
        entry: BlockId(0),
        span: s(),
    };
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![callback, keep],
        closures: vec![ClosureDefinition {
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![],
            parameters: vec![],
            result: Type::I32,
            kind: if kind == CallableKind::FnOnce {
                CallableKind::Fn
            } else {
                kind
            },
            lending_captures: vec![],
        }],
        resources: vec![],
        unsupported: vec![],
    }
}
fn owned_slot() -> TypedModule {
    let mut m = base(CallableKind::FnMut);
    let payload = Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    m.resources.push(ResourceDefinition {
        name: "Token".into(),
        payload: payload.clone(),
        teardown: None,
    });
    m.closures[0].captures = vec![Type::Managed(Box::new(payload))];
    let f = &mut m.functions[1];
    f.callable_return_contract.as_mut().unwrap().cleanup = vec![0];
    let slot = Type::Array {
        element: Box::new(Type::Closure(ClosureId(0))),
        length: 1,
    };
    f.locals.push(local(2, slot.clone()));
    f.values = vec![
        Type::Closure(ClosureId(0)),
        slot,
        Type::Address {
            mutable: true,
            inner: Box::new(Type::Closure(ClosureId(0))),
        },
        Type::I32,
        Type::Closure(ClosureId(0)),
    ];
    let mut instructions = vec![
        enter(),
        op(None, InstructionKind::StorageLive(LocalId(2))),
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::MakeOwnedArray {
                elements: vec![ValueId(0)],
            },
        ),
        op(
            None,
            InstructionKind::Store {
                local: LocalId(2),
                value: ValueId(1),
            },
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::BorrowOwnedArrayElement {
                local: LocalId(2),
                index: 0,
                mutable: true,
            },
        ),
        op(
            Some(ValueId(3)),
            InstructionKind::CallClosureBorrowed {
                callee: ValueId(2),
                kind: CallableKind::FnMut,
                arguments: vec![],
            },
        ),
        op(
            Some(ValueId(4)),
            InstructionKind::TakeOwnedArrayElement {
                local: LocalId(2),
                index: 0,
            },
        ),
        op(None, InstructionKind::DropOwnedArray { local: LocalId(2) }),
        op(None, InstructionKind::StorageDead(LocalId(2))),
    ];
    instructions.extend(exit());
    f.blocks[0] = block(0, instructions, Terminator::Return(Some(ValueId(4))));
    f.callable_contract.as_mut().unwrap().traces = CallableTraces::Finite(vec![CallableTrace {
        calls: vec![CallEvent {
            parameter: 1,
            kind: CallableKind::FnMut,
        }],
        exit: CallableExit::Return,
    }]);
    m
}

// Independent variation: keep forwards the parent, and the parent body never
// calls keep. Thus the capture body belongs to the returned environment graph
// without an ordinary/concrete call edge connecting it to the keep helper.
fn capture_parent(stale: bool, connect: bool) -> TypedModule {
    let mut m = owned_slot();
    let mut parent = m.functions[0].clone();
    parent.id = FunctionId(2);
    parent.name = "returned_parent".into();
    parent.environment = Some(ClosureId(1));
    parent.locals = vec![local(0, Type::Closure(ClosureId(0)))];
    parent.values = vec![
        Type::Address { mutable: true, inner: Box::new(Type::Closure(ClosureId(0))) },
        Type::Closure(ClosureId(0)),
        Type::I32,
    ];
    let mut ins = vec![
        enter(),
        op(None, InstructionKind::StorageLive(LocalId(0))),
        op(Some(ValueId(0)), InstructionKind::BorrowCapture { index: 0, mutable: true }),
        op(Some(ValueId(2)), InstructionKind::CallClosureBorrowed { callee: ValueId(0), kind: CallableKind::FnMut, arguments: vec![] }),
        op(Some(ValueId(1)), InstructionKind::TakeCapture { index: 0 }),
        op(None, InstructionKind::Store { local: LocalId(0), value: ValueId(1) }),
        op(None, InstructionKind::DropClosure { local: LocalId(0) }),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(None, InstructionKind::ScopeExit { scope: ScopeId(0), kind: CleanupExitKind::Return }),
    ];
    if stale { ins.swap(3,4); }
    parent.blocks = vec![block(0, ins, Terminator::Return(Some(ValueId(2))))];
    m.functions.push(parent);
    m.closures.push(ClosureDefinition {
        id: ClosureId(1), function: FunctionId(2),
        captures: vec![Type::Closure(ClosureId(0))], parameters: vec![],
        result: Type::I32, kind: CallableKind::FnOnce, lending_captures: vec![],
    });
    // Replace the helper with a plain transfer of the parent environment.
    let mut keep = base(CallableKind::FnOnce).functions.remove(1);
    keep.locals[1].ty = Type::Closure(ClosureId(1));
    keep.values[0] = Type::Closure(ClosureId(1));
    keep.return_type = Type::Closure(ClosureId(1));
    let c = keep.callable_return_contract.as_mut().unwrap();
    c.closure = ClosureId(1);
    c.cleanup = vec![0];
    keep.callable_contract.as_mut().unwrap().roles[0].closure = ClosureId(1);
    if connect {
        // A matched FnOnce invocation in an otherwise unrelated, reachable
        // ordinary caller links the existing forward window pass to parent.
        let mut caller = keep.clone();
        caller.id = FunctionId(3);
        caller.name = "consumer".into();
        let contract = caller.callable_contract.as_mut().unwrap();
        contract.roles[0].cleanup = CallableCleanup::Owned;
        contract.traces = CallableTraces::Finite(vec![CallableTrace {
            calls: vec![CallEvent { parameter: 1, kind: CallableKind::FnOnce }],
            exit: CallableExit::Return,
        }]);
        caller.callable_return_contract = None;
        caller.return_type = Type::I32;
        caller.values = vec![Type::Bool, Type::Closure(ClosureId(1)), Type::Closure(ClosureId(1)), Type::I32];
        let mut ins = vec![
            enter(),
            op(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
            op(Some(ValueId(1)), InstructionKind::TakeLocal { local: LocalId(1) }),
            op(Some(ValueId(2)), InstructionKind::Call { function: FunctionId(1), arguments: vec![ValueId(0),ValueId(1)] }),
            op(Some(ValueId(3)), InstructionKind::CallClosureOnce { callee: ValueId(2), arguments: vec![] }),
        ];
        ins.extend(exit());
        caller.blocks = vec![block(0, ins, Terminator::Return(Some(ValueId(3))))];
        m.functions.push(caller);
    }
    m.functions[1] = keep;
    m
}

fn outcome(name: &str, m: &TypedModule) {
    let limits = Limits::default();
    let raw = zil_ir::wire::encode_module_candidate(m, &CandidatePlan::empty_for(m), &limits).unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&raw, &limits).unwrap().0;
    for (label, module) in [("raw",m),("wire",&decoded)] {
        match analyze(module, &limits) {
            Ok(_) => println!("{name} {label}: ACCEPT"),
            Err(ds) => println!("{name} {label}: REJECT {ds:?}"),
        }
    }
    std::fs::write(format!("../{name}.typed.txt"), format!("{m:#?}")).unwrap();
    std::fs::write(format!("../{name}.wire"), raw).unwrap();
}

fn main() {
    outcome("parent_valid_no_call_edge", &capture_parent(false, false));
    outcome("parent_stale_no_call_edge", &capture_parent(true, false));
    outcome("parent_valid_with_call_edge", &capture_parent(false, true));
    outcome("parent_stale_with_call_edge", &capture_parent(true, true));
}

