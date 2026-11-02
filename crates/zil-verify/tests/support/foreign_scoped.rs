//! Raw runtime fixture. No Types/Analyze routine or producer certificate builds it.
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;
pub fn span() -> Span {
    Span::new(FileId(0), 0, 100)
}
fn function(
    id: u32,
    parameters: Vec<Type>,
    result: Type,
    environment: Option<ClosureId>,
) -> TypedFunction {
    TypedFunction {
        id: FunctionId(id),
        name: format!("scoped_{id}"),
        foreign: None,
        environment,
        parameters: (0..parameters.len()).map(|i| LocalId(i as u32)).collect(),
        locals: parameters
            .into_iter()
            .enumerate()
            .map(|(i, ty)| LocalDecl {
                id: LocalId(i as u32),
                name: format!("arg{i}"),
                ty,
                mutable: false,
                span: span(),
            })
            .collect(),
        values: vec![],
        blocks: vec![],
        entry: BlockId(0),
        return_type: result,
        result_mode: PassingMode::Infer,
        callable_contract: None,
        callable_return_contract: None,
        span: span(),
    }
}
pub fn instruction(result: Option<u32>, kind: InstructionKind) -> Instruction {
    Instruction {
        result: result.map(ValueId),
        kind,
        span: span(),
    }
}
pub fn fixture() -> (TypedModule, CandidatePlan) {
    let callback = Type::ForeignCallback {
        parameters: vec![Type::ForeignHandle, Type::I32],
        result: Box::new(Type::I64),
    };
    let mut register = function(
        0,
        vec![callback, Type::ForeignHandle],
        Type::ForeignHandle,
        None,
    );
    register.foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "scoped_register".into(),
        static_callbacks: true,
        noescape_parameters: vec![],
        scoped_callback: Some(ScopedCallbackRole::Register),
    });
    let mut close = function(1, vec![Type::ForeignHandle], Type::Unit, None);
    close.foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "scoped_close".into(),
        static_callbacks: true,
        noescape_parameters: vec![],
        scoped_callback: Some(ScopedCallbackRole::Close),
    });
    let mut callback_entry = function(2, vec![Type::I32], Type::I32, Some(ClosureId(0)));
    callback_entry.values = vec![Type::I32];
    callback_entry.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![instruction(Some(0), InstructionKind::Load(LocalId(0)))],
        terminator: Terminator::Return(Some(ValueId(0))),
    }];
    let mut body_entry = function(3, vec![], Type::I32, Some(ClosureId(1)));
    body_entry.values = vec![Type::I32];
    body_entry.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![instruction(
            Some(0),
            InstructionKind::Constant(Constant::I32(7)),
        )],
        terminator: Terminator::Return(Some(ValueId(0))),
    }];
    let pair = Type::Tuple(vec![Type::Bool, Type::I32]);
    let mut caller = function(4, vec![], pair.clone(), None);
    caller.locals = vec![LocalDecl {
        id: LocalId(0),
        name: "f".into(),
        ty: Type::Closure(ClosureId(0)),
        mutable: true,
        span: span(),
    }];
    caller.values = vec![
        Type::Closure(ClosureId(0)),
        Type::Address {
            mutable: true,
            inner: Box::new(Type::Closure(ClosureId(0))),
        },
        Type::Closure(ClosureId(1)),
        pair,
    ];
    caller.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            instruction(None, InstructionKind::StorageLive(LocalId(0))),
            instruction(
                Some(0),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![],
                },
            ),
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(0),
                },
            ),
            instruction(
                Some(1),
                InstructionKind::BorrowLocalMut { local: LocalId(0) },
            ),
            instruction(
                Some(2),
                InstructionKind::MakeClosure {
                    closure: ClosureId(1),
                    captures: vec![],
                },
            ),
            instruction(None, InstructionKind::UnsafeEnter { region: 7 }),
            instruction(
                Some(3),
                InstructionKind::WithForeignCallback {
                    register: FunctionId(0),
                    close: FunctionId(1),
                    callback: ValueId(1),
                    body: ValueId(2),
                    region: 7,
                },
            ),
            instruction(None, InstructionKind::UnsafeExit { region: 7 }),
        ],
        terminator: Terminator::Return(Some(ValueId(3))),
    }];
    let module = TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"independent-scoped-runtime-fixture"),
        target: TargetProfile::host_llvm(),
        functions: vec![register, close, callback_entry, body_entry, caller],
        closures: vec![
            ClosureDefinition {
                id: ClosureId(0),
                function: FunctionId(2),
                captures: vec![],
                parameters: vec![Type::I32],
                result: Type::I32,
                kind: CallableKind::FnMut,
                lending_captures: vec![],
            },
            ClosureDefinition {
                id: ClosureId(1),
                function: FunctionId(3),
                captures: vec![],
                parameters: vec![],
                result: Type::I32,
                kind: CallableKind::FnOnce,
                lending_captures: vec![],
            },
        ],
        resources: vec![],
        unsupported: vec![],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions = module
        .functions
        .iter()
        .map(|f| FunctionPlan {
            function: f.id,
            blocks: f
                .blocks
                .iter()
                .map(|b| BlockPlan {
                    block: b.id,
                    initialized_in: f.parameters.clone(),
                    initialized_out: if f.id == FunctionId(4) {
                        vec![LocalId(0)]
                    } else {
                        f.parameters.clone()
                    },
                })
                .collect(),
            holds: f
                .values
                .iter()
                .enumerate()
                .map(|(i, _)| ValueHold {
                    value: ValueId(i as u32),
                    roots: if f.id == FunctionId(4) && i == 1 {
                        vec![HoldRoot::Local(LocalId(0))]
                    } else {
                        vec![]
                    },
                })
                .collect(),
        })
        .collect();
    (module, plan)
}
/// Same concrete F type in two distinct positions; only the chosen capture
/// source changes when `alias` is true. The body really calls its captured g.
pub fn borrowed_closure_body(alias: bool) -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = fixture();
    let address = Type::Address {
        mutable: true,
        inner: Box::new(Type::Closure(ClosureId(0))),
    };
    m.closures[1].captures = vec![address.clone()];
    m.functions[3].values = vec![address.clone(), Type::I32, Type::I32];
    m.functions[3].blocks[0].instructions = vec![
        instruction(Some(0), InstructionKind::ReadCapture { index: 0 }),
        instruction(Some(1), InstructionKind::Constant(Constant::I32(9))),
        instruction(
            Some(2),
            InstructionKind::CallClosureBorrowed {
                callee: ValueId(0),
                kind: CallableKind::FnMut,
                arguments: vec![ValueId(1)],
            },
        ),
    ];
    m.functions[3].blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    p.functions[3].holds = vec![
        ValueHold {
            value: ValueId(0),
            roots: vec![HoldRoot::Capture(0)],
        },
        ValueHold {
            value: ValueId(1),
            roots: vec![],
        },
        ValueHold {
            value: ValueId(2),
            roots: vec![],
        },
    ];
    m.functions[4].locals.push(LocalDecl {
        id: LocalId(1),
        name: "g".into(),
        ty: Type::Closure(ClosureId(0)),
        mutable: true,
        span: span(),
    });
    m.functions[4]
        .values
        .extend([Type::Closure(ClosureId(0)), address]);
    let borrowed = if alias {
        InstructionKind::ReborrowAddress {
            address: ValueId(1),
            mutable: true,
        }
    } else {
        InstructionKind::BorrowLocalMut { local: LocalId(1) }
    };
    m.functions[4].blocks[0].instructions.splice(
        4..4,
        [
            instruction(None, InstructionKind::StorageLive(LocalId(1))),
            instruction(
                Some(4),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![],
                },
            ),
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(4),
                },
            ),
            instruction(Some(5), borrowed),
        ],
    );
    if let InstructionKind::MakeClosure { captures, .. } =
        &mut m.functions[4].blocks[0].instructions[8].kind
    {
        *captures = vec![ValueId(5)]
    }
    let root = HoldRoot::Local(LocalId(if alias { 0 } else { 1 }));
    p.functions[4].holds[2].roots = vec![root];
    p.functions[4].holds.extend([
        ValueHold {
            value: ValueId(4),
            roots: vec![],
        },
        ValueHold {
            value: ValueId(5),
            roots: vec![root],
        },
    ]);
    p.functions[4].blocks[0].initialized_out.push(LocalId(1));
    (m, p)
}
pub fn borrowed_scalar_body() -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = fixture();
    let address = Type::Address {
        mutable: true,
        inner: Box::new(Type::I32),
    };
    m.closures[1].captures = vec![address.clone()];
    m.functions[3].values = vec![address.clone(), Type::I32, Type::I32, Type::I32];
    m.functions[3].blocks[0].instructions = vec![
        instruction(Some(0), InstructionKind::ReadCapture { index: 0 }),
        instruction(
            Some(1),
            InstructionKind::ReadAddress {
                address: ValueId(0),
            },
        ),
        instruction(Some(2), InstructionKind::Constant(Constant::I32(1))),
        instruction(
            Some(3),
            InstructionKind::Binary {
                op: BinaryOp::Add,
                left: ValueId(1),
                right: ValueId(2),
            },
        ),
        instruction(
            None,
            InstructionKind::WriteAddress {
                address: ValueId(0),
                value: ValueId(3),
            },
        ),
    ];
    m.functions[3].blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
    p.functions[3].holds = (0..4)
        .map(|i| ValueHold {
            value: ValueId(i),
            roots: if i == 0 {
                vec![HoldRoot::Capture(0)]
            } else {
                vec![]
            },
        })
        .collect();
    m.functions[4].locals.push(LocalDecl {
        id: LocalId(1),
        name: "x".into(),
        ty: Type::I32,
        mutable: true,
        span: span(),
    });
    m.functions[4].values.extend([Type::I32, address]);
    m.functions[4].blocks[0].instructions.splice(
        4..4,
        [
            instruction(None, InstructionKind::StorageLive(LocalId(1))),
            instruction(Some(4), InstructionKind::Constant(Constant::I32(20))),
            instruction(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(4),
                },
            ),
            instruction(
                Some(5),
                InstructionKind::BorrowLocalMut { local: LocalId(1) },
            ),
        ],
    );
    if let InstructionKind::MakeClosure { captures, .. } =
        &mut m.functions[4].blocks[0].instructions[8].kind
    {
        *captures = vec![ValueId(5)]
    }
    p.functions[4].holds[2].roots = vec![HoldRoot::Local(LocalId(1))];
    p.functions[4].holds.extend([
        ValueHold {
            value: ValueId(4),
            roots: vec![],
        },
        ValueHold {
            value: ValueId(5),
            roots: vec![HoldRoot::Local(LocalId(1))],
        },
    ]);
    p.functions[4].blocks[0].initialized_out.push(LocalId(1));
    (m, p)
}
/// Public two-role wrapper, constructed independently of all source producers.
pub fn wrapper() -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = fixture();
    let address = Type::Address {
        mutable: true,
        inner: Box::new(Type::Closure(ClosureId(0))),
    };
    let f = &mut m.functions[4];
    f.parameters = vec![LocalId(0), LocalId(1)];
    f.locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "f".into(),
            ty: address.clone(),
            mutable: false,
            span: span(),
        },
        LocalDecl {
            id: LocalId(1),
            name: "body".into(),
            ty: Type::Closure(ClosureId(1)),
            mutable: false,
            span: span(),
        },
    ];
    f.values = vec![
        address,
        Type::Closure(ClosureId(1)),
        Type::Tuple(vec![Type::Bool, Type::I32]),
    ];
    f.blocks[0].instructions = vec![
        instruction(Some(0), InstructionKind::Load(LocalId(0))),
        instruction(Some(1), InstructionKind::TakeLocal { local: LocalId(1) }),
        instruction(None, InstructionKind::UnsafeEnter { region: 7 }),
        instruction(
            Some(2),
            InstructionKind::WithForeignCallback {
                register: FunctionId(0),
                close: FunctionId(1),
                callback: ValueId(0),
                body: ValueId(1),
                region: 7,
            },
        ),
        instruction(None, InstructionKind::UnsafeExit { region: 7 }),
    ];
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    let event = |parameter, kind| vec![CallEvent { parameter, kind }];
    let edges = |next| CallableFlowNext::Edges(next);
    f.callable_contract = Some(CallableParameterContract {
        roles: vec![
            CallableRole {
                parameter: 0,
                closure: ClosureId(0),
                kind: CallableKind::FnMut,
                hold: vec![],
                call: vec![],
                writes: vec![],
                cleanup: CallableCleanup::Borrowed,
            },
            CallableRole {
                parameter: 1,
                closure: ClosureId(1),
                kind: CallableKind::FnOnce,
                hold: vec![],
                call: vec![],
                writes: vec![],
                cleanup: CallableCleanup::Owned,
            },
        ],
        traces: CallableTraces::Flow(CallableFlow {
            entry: 0,
            nodes: vec![
                CallableFlowNode {
                    calls: vec![],
                    next: edges(vec![1, 2, 3]),
                },
                CallableFlowNode {
                    calls: event(0, CallableKind::FnMut),
                    next: edges(vec![1, 2, 3]),
                },
                CallableFlowNode {
                    calls: event(1, CallableKind::FnOnce),
                    next: edges(vec![4, 3]),
                },
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Exit(CallableExit::Return),
                },
                CallableFlowNode {
                    calls: event(0, CallableKind::FnMut),
                    next: edges(vec![4, 3]),
                },
            ],
        }),
    });
    p.functions[4].blocks[0].initialized_in = vec![LocalId(0), LocalId(1)];
    p.functions[4].blocks[0].initialized_out = vec![LocalId(0)];
    p.functions[4].holds = vec![
        ValueHold {
            value: ValueId(0),
            roots: vec![HoldRoot::Parameter(0)],
        },
        ValueHold {
            value: ValueId(1),
            roots: vec![],
        },
        ValueHold {
            value: ValueId(2),
            roots: vec![],
        },
    ];
    (m, p)
}

/// One actual F parameter; the concrete body is constructed inside the helper.
pub fn local_body_wrapper(through_local: bool) -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = wrapper();
    let f = &mut m.functions[4];
    f.parameters.pop();
    let claim = f.callable_contract.as_mut().unwrap();
    claim.roles.pop();
    let CallableTraces::Flow(flow) = &mut claim.traces else {
        unreachable!()
    };
    flow.nodes[2].calls.clear();
    f.blocks[0].instructions[1].kind = InstructionKind::MakeClosure {
        closure: ClosureId(1),
        captures: vec![],
    };
    if through_local {
        f.values.push(Type::Closure(ClosureId(1)));
        f.blocks[0].instructions.splice(
            2..2,
            [
                instruction(None, InstructionKind::StorageLive(LocalId(1))),
                instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(1),
                        value: ValueId(1),
                    },
                ),
                instruction(Some(3), InstructionKind::TakeLocal { local: LocalId(1) }),
            ],
        );
        let InstructionKind::WithForeignCallback { body, .. } =
            &mut f.blocks[0].instructions[6].kind
        else {
            unreachable!()
        };
        *body = ValueId(3);
        p.functions[4].holds.push(ValueHold {
            value: ValueId(3),
            roots: vec![],
        });
    }
    p.functions[4].blocks[0].initialized_in = vec![LocalId(0)];
    (m, p)
}

pub fn local_body_capture_wrapper(alias: bool) -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = local_body_wrapper(true);
    let (source, source_plan) = if alias {
        borrowed_closure_body(true)
    } else {
        borrowed_scalar_body()
    };
    m.closures[1] = source.closures[1].clone();
    m.functions[3] = source.functions[3].clone();
    p.functions[3] = source_plan.functions[3].clone();
    let f = &mut m.functions[4];
    let (capture, root) = if alias {
        f.values.push(m.closures[1].captures[0].clone());
        f.blocks[0].instructions.insert(
            1,
            instruction(
                Some(4),
                InstructionKind::ReborrowAddress {
                    address: ValueId(0),
                    mutable: true,
                },
            ),
        );
        p.functions[4].holds.push(ValueHold {
            value: ValueId(4),
            roots: vec![HoldRoot::Parameter(0)],
        });
        (ValueId(4), HoldRoot::Parameter(0))
    } else {
        f.locals.push(LocalDecl {
            id: LocalId(2),
            name: "x".into(),
            ty: Type::I32,
            mutable: true,
            span: span(),
        });
        f.values
            .extend([Type::I32, m.closures[1].captures[0].clone()]);
        f.blocks[0].instructions.splice(
            1..1,
            [
                instruction(None, InstructionKind::StorageLive(LocalId(2))),
                instruction(Some(4), InstructionKind::Constant(Constant::I32(20))),
                instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(2),
                        value: ValueId(4),
                    },
                ),
                instruction(
                    Some(5),
                    InstructionKind::BorrowLocalMut { local: LocalId(2) },
                ),
            ],
        );
        p.functions[4].holds.extend([
            ValueHold {
                value: ValueId(4),
                roots: vec![],
            },
            ValueHold {
                value: ValueId(5),
                roots: vec![HoldRoot::Local(LocalId(2))],
            },
        ]);
        p.functions[4].blocks[0].initialized_out.push(LocalId(2));
        (ValueId(5), HoldRoot::Local(LocalId(2)))
    };
    for instruction in &mut f.blocks[0].instructions {
        if let InstructionKind::MakeClosure { captures, .. } = &mut instruction.kind {
            *captures = vec![capture];
        }
    }
    p.functions[4].holds[1].roots = vec![root];
    p.functions[4].holds[3].roots = vec![root];
    (m, p)
}
