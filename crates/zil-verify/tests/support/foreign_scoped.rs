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
        retained_return_sources: None,
        storage_life: None,
        call_kind: zil_ir::CallKind::Direct,
        reference_return_bound: None,
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
        protocol_contract: None,
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
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"independent-scoped-runtime-fixture"),
        target: TargetProfile::host_llvm(),
        functions: vec![register, close, callback_entry, body_entry, caller],
        closures: vec![
            ClosureDefinition {
                storage_life: None,
                id: ClosureId(0),
                function: FunctionId(2),
                captures: vec![],
                parameters: vec![Type::I32],
                result: Type::I32,
                kind: CallableKind::FnMut,
                lending_captures: vec![],
            },
            ClosureDefinition {
                storage_life: None,
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
                life: None,
                parameter: 0,
                closure: ClosureId(0),
                kind: CallableKind::FnMut,
                hold: vec![],
                call: vec![],
                writes: vec![],
                cleanup: CallableCleanup::Borrowed,
            },
            CallableRole {
                life: None,
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

/// Independently prepared concrete ownership, with no producer ownership proof.
pub fn local_owned_body(package: bool) -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = local_body_wrapper(true);
    let payload = Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    let managed = Type::Managed(Box::new(payload.clone()));
    let captured = if package {
        Type::Package {
            name: "Owned".into(),
            fields: vec![("token".into(), managed.clone())],
        }
    } else {
        managed.clone()
    };
    m.resources.push(ResourceDefinition {
        name: "Token".into(),
        payload,
        teardown: None,
    });
    m.closures[1].captures = vec![captured.clone()];
    m.functions[4].values.extend([Type::I32, managed]);
    let mut preparation = vec![
        instruction(Some(4), InstructionKind::Constant(Constant::I32(1))),
        instruction(
            Some(5),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(4)],
            },
        ),
    ];
    let capture = if package {
        m.functions[4].values.push(captured.clone());
        preparation.push(instruction(
            Some(6),
            InstructionKind::MakePackage {
                fields: vec![ValueId(5)],
            },
        ));
        ValueId(6)
    } else {
        ValueId(5)
    };
    let InstructionKind::MakeClosure { captures, .. } =
        &mut m.functions[4].blocks[0].instructions[1].kind
    else {
        unreachable!()
    };
    *captures = vec![capture];
    m.functions[4].blocks[0]
        .instructions
        .splice(1..1, preparation);
    for value in 4..m.functions[4].values.len() {
        p.functions[4].holds.push(ValueHold {
            value: ValueId(value as u32),
            roots: vec![],
        });
    }
    if package {
        // Exact scope/storage and capture view plumbing, independent of the
        // ownership mutations exercised by the tests.
        let caller = &mut m.functions[4];
        caller.blocks[0].instructions.insert(
            0,
            instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        );
        caller.blocks[0].instructions.extend([
            instruction(None, InstructionKind::StorageDead(LocalId(1))),
            instruction(None, InstructionKind::StorageDead(LocalId(0))),
            instruction(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ]);
        p.functions[4].blocks[0].initialized_out.clear();
        let entry = &mut m.functions[3];
        entry.locals.push(LocalDecl {
            id: LocalId(0),
            name: "owned".into(),
            ty: captured,
            mutable: true,
            span: span(),
        });
        entry.blocks[0].instructions.insert(
            0,
            instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        );
        entry.blocks[0].instructions.insert(
            1,
            instruction(
                None,
                InstructionKind::BindPackageCapture {
                    local: LocalId(0),
                    index: 0,
                },
            ),
        );
        entry.blocks[0].instructions.extend([
            instruction(
                None,
                InstructionKind::EndPackageCapture {
                    local: LocalId(0),
                    index: 0,
                },
            ),
            instruction(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ]);
    }
    (m, p)
}

pub fn ordinary_local_owner() -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = local_owned_body(false);
    let f = &mut m.functions[4];
    f.locals[1].ty = m.closures[1].captures[0].clone();
    f.values = vec![
        f.locals[0].ty.clone(),
        Type::I32,
        f.locals[1].ty.clone(),
        Type::I32,
    ];
    f.return_type = Type::I32;
    let event = CallEvent {
        parameter: 0,
        kind: CallableKind::FnMut,
    };
    f.callable_contract.as_mut().unwrap().traces = CallableTraces::Finite(vec![CallableTrace {
        calls: vec![event],
        exit: CallableExit::Return,
    }]);
    f.blocks[0].instructions = vec![
        instruction(None, InstructionKind::StorageLive(LocalId(1))),
        instruction(Some(1), InstructionKind::Constant(Constant::I32(19))),
        instruction(
            Some(2),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(1)],
            },
        ),
        instruction(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(2),
            },
        ),
        instruction(Some(0), InstructionKind::Load(LocalId(0))),
        instruction(
            Some(3),
            InstructionKind::CallClosureBorrowed {
                callee: ValueId(0),
                kind: CallableKind::FnMut,
                arguments: vec![ValueId(1)],
            },
        ),
        instruction(None, InstructionKind::DropManaged { local: LocalId(1) }),
        instruction(None, InstructionKind::StorageDead(LocalId(1))),
    ];
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
    p.functions[4].holds.truncate(4);
    for hold in &mut p.functions[4].holds {
        hold.roots.clear();
    }
    p.functions[4].holds[0].roots = vec![HoldRoot::Parameter(0)];
    (m, p)
}

/// The caller transfers this invocation's prepared owner to an ordinary
/// factory. Its actual fresh result, not a same-typed parameter, becomes body.
pub fn factory_body(package: bool) -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = local_owned_body(package);
    let capture_type = m.closures[1].captures[0].clone();
    let mut factory = function(
        5,
        vec![capture_type.clone()],
        Type::Closure(ClosureId(1)),
        None,
    );
    factory.values = vec![capture_type, Type::Closure(ClosureId(1))];
    let mut instructions = vec![
        instruction(Some(0), InstructionKind::TakeLocal { local: LocalId(0) }),
        instruction(
            Some(1),
            InstructionKind::MakeClosure {
                closure: ClosureId(1),
                captures: vec![ValueId(0)],
            },
        ),
    ];
    if package {
        instructions.insert(
            0,
            instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        );
        instructions.extend([
            instruction(None, InstructionKind::StorageDead(LocalId(0))),
            instruction(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ]);
    }
    factory.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions,
        terminator: Terminator::Return(Some(ValueId(1))),
    }];
    factory.callable_return_contract = Some(CallableReturnContract {
        capture_sources: Some(Vec::new()),
        source: CallableReturnSource::Fresh,
        closure: ClosureId(1),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![0],
        lending_captures: vec![],
    });
    m.functions.push(factory);
    p.functions.push(FunctionPlan {
        function: FunctionId(5),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![LocalId(0)],
            initialized_out: vec![],
        }],
        holds: vec![
            ValueHold {
                value: ValueId(0),
                roots: vec![],
            },
            ValueHold {
                value: ValueId(1),
                roots: vec![],
            },
        ],
    });
    let caller = &mut m.functions[4];
    let made = caller.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    made.kind = InstructionKind::Call {
        function: FunctionId(5),
        arguments: vec![ValueId(if package { 6 } else { 5 })],
    };
    (m, p)
}

pub fn captured_local_handler(package: bool) -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = local_owned_body(package);
    // Deliberately reuse F's ClosureId for a different constructed instance.
    // Concrete identity alone cannot make this value a supplied role.
    m.closures[1].captures.push(Type::Closure(ClosureId(0)));
    let f = &mut m.functions[4];
    let handler = ValueId(f.values.len() as u32);
    f.values.push(Type::Closure(ClosureId(0)));
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    f.blocks[0].instructions.insert(
        at,
        instruction(
            Some(handler.0),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![],
            },
        ),
    );
    let InstructionKind::MakeClosure { captures, .. } = &mut f.blocks[0].instructions[at + 1].kind
    else {
        unreachable!()
    };
    captures.push(handler);
    p.functions[4].holds.push(ValueHold {
        value: handler,
        roots: vec![],
    });
    let entry = &mut m.functions[3];
    entry
        .values
        .extend([Type::Closure(ClosureId(0)), Type::I32, Type::I32]);
    let at = if package { 3 } else { 1 };
    entry.blocks[0].instructions.splice(
        at..at,
        [
            instruction(Some(1), InstructionKind::TakeCapture { index: 1 }),
            instruction(Some(2), InstructionKind::Constant(Constant::I32(9))),
            instruction(
                Some(3),
                InstructionKind::CallClosureOnce {
                    callee: ValueId(1),
                    arguments: vec![ValueId(2)],
                },
            ),
        ],
    );
    entry.blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
    p.functions[3].holds.extend((1..4).map(|value| ValueHold {
        value: ValueId(value),
        roots: vec![],
    }));
    (m, p)
}

pub fn role_capturing_local_handler() -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = fixture();
    let f = &mut m.functions[4];
    f.parameters = vec![LocalId(0)];
    f.locals[0].ty = Type::Closure(ClosureId(0));
    f.locals.push(LocalDecl {
        id: LocalId(1),
        name: "local".into(),
        ty: Type::Closure(ClosureId(2)),
        mutable: false,
        span: span(),
    });
    f.values = vec![
        Type::Closure(ClosureId(0)),
        Type::Closure(ClosureId(2)),
        Type::Closure(ClosureId(2)),
        Type::I32,
    ];
    f.return_type = Type::I32;
    f.callable_contract = Some(CallableParameterContract {
        roles: vec![CallableRole {
            life: None,
            parameter: 0,
            closure: ClosureId(0),
            kind: CallableKind::FnOnce,
            hold: vec![],
            call: vec![],
            writes: vec![],
            cleanup: CallableCleanup::Owned,
        }],
        traces: CallableTraces::Finite(vec![CallableTrace {
            calls: vec![],
            exit: CallableExit::Return,
        }]),
    });
    f.blocks[0].instructions = vec![
        instruction(Some(0), InstructionKind::TakeLocal { local: LocalId(0) }),
        instruction(None, InstructionKind::StorageLive(LocalId(1))),
        instruction(
            Some(1),
            InstructionKind::MakeClosure {
                closure: ClosureId(2),
                captures: vec![ValueId(0)],
            },
        ),
        instruction(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(1),
            },
        ),
        instruction(Some(2), InstructionKind::TakeLocal { local: LocalId(1) }),
        instruction(
            Some(3),
            InstructionKind::CallClosureOnce {
                callee: ValueId(2),
                arguments: vec![],
            },
        ),
        instruction(None, InstructionKind::StorageDead(LocalId(1))),
    ];
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
    let mut entry = function(5, vec![], Type::I32, Some(ClosureId(2)));
    entry.values = vec![Type::Closure(ClosureId(0)), Type::I32, Type::I32];
    entry.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            instruction(Some(0), InstructionKind::TakeCapture { index: 0 }),
            instruction(Some(1), InstructionKind::Constant(Constant::I32(9))),
            instruction(
                Some(2),
                InstructionKind::CallClosureOnce {
                    callee: ValueId(0),
                    arguments: vec![ValueId(1)],
                },
            ),
        ],
        terminator: Terminator::Return(Some(ValueId(2))),
    }];
    m.functions.push(entry);
    m.closures.push(ClosureDefinition {
        storage_life: None,
        id: ClosureId(2),
        function: FunctionId(5),
        captures: vec![Type::Closure(ClosureId(0))],
        parameters: vec![],
        result: Type::I32,
        kind: CallableKind::FnOnce,
        lending_captures: vec![],
    });
    p.functions[4].blocks[0].initialized_in = vec![LocalId(0)];
    p.functions[4].blocks[0].initialized_out.clear();
    p.functions[4].holds = (0..4)
        .map(|value| ValueHold {
            value: ValueId(value),
            roots: vec![],
        })
        .collect();
    p.functions.push(FunctionPlan {
        function: FunctionId(5),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![],
        }],
        holds: (0..3)
            .map(|value| ValueHold {
                value: ValueId(value),
                roots: vec![],
            })
            .collect(),
    });
    (m, p)
}

pub fn local_child_handler() -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = ordinary_local_owner();
    let mut child = function(
        5,
        vec![Type::Closure(ClosureId(0)), Type::I32],
        Type::I32,
        None,
    );
    child.values = vec![Type::Closure(ClosureId(0)), Type::I32, Type::I32];
    child.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            instruction(Some(0), InstructionKind::TakeLocal { local: LocalId(0) }),
            instruction(Some(1), InstructionKind::Load(LocalId(1))),
            instruction(
                Some(2),
                InstructionKind::CallClosureOnce {
                    callee: ValueId(0),
                    arguments: vec![ValueId(1)],
                },
            ),
        ],
        terminator: Terminator::Return(Some(ValueId(2))),
    }];
    child.callable_contract = Some(CallableParameterContract {
        roles: vec![CallableRole {
            life: None,
            parameter: 0,
            closure: ClosureId(0),
            kind: CallableKind::FnOnce,
            hold: vec![],
            call: vec![],
            writes: vec![],
            cleanup: CallableCleanup::Owned,
        }],
        traces: CallableTraces::Finite(vec![CallableTrace {
            calls: vec![CallEvent {
                parameter: 0,
                kind: CallableKind::FnOnce,
            }],
            exit: CallableExit::Return,
        }]),
    });
    m.functions.push(child);
    p.functions.push(FunctionPlan {
        function: FunctionId(5),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![LocalId(0), LocalId(1)],
            initialized_out: vec![LocalId(1)],
        }],
        holds: (0..3)
            .map(|value| ValueHold {
                value: ValueId(value),
                roots: vec![],
            })
            .collect(),
    });
    let f = &mut m.functions[4];
    f.values.extend([Type::Closure(ClosureId(0)), Type::I32]);
    f.blocks[0].instructions.splice(
        5..5,
        [
            instruction(
                Some(4),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![],
                },
            ),
            instruction(
                Some(5),
                InstructionKind::Call {
                    function: FunctionId(5),
                    arguments: vec![ValueId(4), ValueId(1)],
                },
            ),
        ],
    );
    p.functions[4].holds.extend((4..6).map(|value| ValueHold {
        value: ValueId(value),
        roots: vec![],
    }));
    (m, p)
}

pub fn factory_child_handler() -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = local_child_handler();
    let mut factory = function(6, vec![], Type::Closure(ClosureId(0)), None);
    factory.values = vec![Type::Closure(ClosureId(0))];
    factory.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![instruction(
            Some(0),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![],
            },
        )],
        terminator: Terminator::Return(Some(ValueId(0))),
    }];
    factory.callable_return_contract = Some(CallableReturnContract {
        capture_sources: Some(Vec::new()),
        source: CallableReturnSource::Fresh,
        closure: ClosureId(0),
        kind: CallableKind::FnMut,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
        lending_captures: vec![],
    });
    m.functions.push(factory);
    p.functions.push(FunctionPlan {
        function: FunctionId(6),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![],
        }],
        holds: vec![ValueHold {
            value: ValueId(0),
            roots: vec![],
        }],
    });
    let made = m.functions[4].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    made.kind = InstructionKind::Call {
        function: FunctionId(6),
        arguments: vec![],
    };
    (m, p)
}

/// A well-typed aggregate transfer cannot bypass the independent capture
/// shape boundary by hiding a supplied role inside a Package or owned union.
pub fn wrapped_role_capture(union: bool) -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = role_capturing_local_handler();
    let payload = Type::Record {
        name: "WrappedToken".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    let managed = Type::Managed(Box::new(payload.clone()));
    m.resources.push(ResourceDefinition {
        name: "WrappedToken".into(),
        payload,
        teardown: None,
    });
    let package = Type::Package {
        name: "HiddenRole".into(),
        fields: vec![
            ("callback".into(), Type::Closure(ClosureId(0))),
            ("owner".into(), managed.clone()),
        ],
    };
    let captured = if union {
        Type::ErrorUnion {
            error: Box::new(Type::Unit),
            result: Box::new(package.clone()),
        }
    } else {
        package.clone()
    };
    m.closures[2].captures = vec![captured.clone()];
    let f = &mut m.functions[4];
    f.values.push(package);
    let mut prepare = vec![instruction(
        Some(4),
        InstructionKind::MakePackage {
            fields: vec![ValueId(0)],
        },
    )];
    let value = if union {
        f.values.push(captured);
        prepare.push(instruction(
            Some(5),
            InstructionKind::MakeErrorUnion {
                is_error: false,
                value: ValueId(4),
            },
        ));
        ValueId(5)
    } else {
        ValueId(4)
    };
    let number = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    let owner = ValueId(f.values.len() as u32);
    f.values.push(managed);
    let InstructionKind::MakePackage { fields } = &mut prepare[0].kind else {
        unreachable!()
    };
    fields.push(owner);
    prepare.splice(
        0..0,
        [
            instruction(Some(number.0), InstructionKind::Constant(Constant::I32(1))),
            instruction(
                Some(owner.0),
                InstructionKind::MakeManaged {
                    fields: vec![number],
                },
            ),
        ],
    );
    f.blocks[0].instructions.insert(
        0,
        instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
    );
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    let InstructionKind::MakeClosure { captures, .. } = &mut f.blocks[0].instructions[at].kind
    else {
        unreachable!()
    };
    *captures = vec![value];
    f.blocks[0].instructions.splice(at..at, prepare);
    f.blocks[0].instructions.extend([
        instruction(None, InstructionKind::StorageDead(LocalId(0))),
        instruction(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ]);
    p.functions[4]
        .holds
        .extend((4..f.values.len()).map(|v| ValueHold {
            value: ValueId(v as u32),
            roots: vec![],
        }));
    // The entry leaves its captured aggregate intact for remaining cleanup;
    // no mismatched Read/Take projection is used to make the negative pass.
    m.functions[5].values = vec![Type::I32];
    m.functions[5].blocks[0].instructions = vec![instruction(
        Some(0),
        InstructionKind::Constant(Constant::I32(0)),
    )];
    m.functions[5].blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    p.functions[5].holds.truncate(1);
    if !union {
        let entry = &mut m.functions[5];
        entry.locals.push(LocalDecl {
            id: LocalId(0),
            name: "view".into(),
            ty: m.closures[2].captures[0].clone(),
            mutable: true,
            span: span(),
        });
        entry.blocks[0].instructions.insert(
            0,
            instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        );
        entry.blocks[0].instructions.insert(
            1,
            instruction(
                None,
                InstructionKind::BindPackageCapture {
                    local: LocalId(0),
                    index: 0,
                },
            ),
        );
        entry.blocks[0].instructions.extend([
            instruction(
                None,
                InstructionKind::EndPackageCapture {
                    local: LocalId(0),
                    index: 0,
                },
            ),
            instruction(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ]);
    }
    (m, p)
}

/// A source FnOnce B must not be hidden by a With-body construction, even
/// when its actual concrete environment happens to support Copy shared calls.
pub fn supplied_body_hidden_in_with(twice: bool) -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = wrapper();
    m.closures[1].kind = CallableKind::Fn;
    let mut entry = function(5, vec![], Type::I32, Some(ClosureId(2)));
    entry.values = vec![Type::Closure(ClosureId(1)), Type::I32, Type::I32];
    entry.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            instruction(Some(0), InstructionKind::TakeCapture { index: 0 }),
            instruction(
                Some(1),
                InstructionKind::CallClosure {
                    callee: ValueId(0),
                    arguments: vec![],
                },
            ),
            instruction(
                Some(2),
                if twice {
                    InstructionKind::CallClosure {
                        callee: ValueId(0),
                        arguments: vec![],
                    }
                } else {
                    InstructionKind::Constant(Constant::I32(0))
                },
            ),
        ],
        terminator: Terminator::Return(Some(ValueId(2))),
    }];
    m.functions.push(entry);
    m.closures.push(ClosureDefinition {
        storage_life: None,
        id: ClosureId(2),
        function: FunctionId(5),
        kind: CallableKind::FnOnce,
        captures: vec![Type::Closure(ClosureId(1))],
        parameters: vec![],
        result: Type::I32,
        lending_captures: vec![],
    });
    let f = &mut m.functions[4];
    f.values.push(Type::Closure(ClosureId(2)));
    f.blocks[0].instructions.insert(
        2,
        instruction(
            Some(3),
            InstructionKind::MakeClosure {
                closure: ClosureId(2),
                captures: vec![ValueId(1)],
            },
        ),
    );
    for instruction in &mut f.blocks[0].instructions {
        if let InstructionKind::WithForeignCallback { body, .. } = &mut instruction.kind {
            *body = ValueId(3);
        }
    }
    let CallableTraces::Flow(flow) = &mut f.callable_contract.as_mut().unwrap().traces else {
        unreachable!()
    };
    flow.nodes[2].calls.clear();
    p.functions[4].holds.push(ValueHold {
        value: ValueId(3),
        roots: vec![],
    });
    p.functions.push(FunctionPlan {
        function: FunctionId(5),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![],
        }],
        holds: (0..3)
            .map(|value| ValueHold {
                value: ValueId(value),
                roots: vec![],
            })
            .collect(),
    });
    (m, p)
}

pub fn role_hidden_through_factory(supplied: bool) -> (TypedModule, CandidatePlan) {
    let (mut m, mut p) = supplied_body_hidden_in_with(true);
    let payload = Type::Record {
        name: "Token".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    let managed = Type::Managed(Box::new(payload.clone()));
    m.resources.push(ResourceDefinition {
        name: "Token".into(),
        payload,
        teardown: None,
    });
    let package = Type::Package {
        name: "FactoryInput".into(),
        fields: vec![
            ("callback".into(), Type::Closure(ClosureId(1))),
            ("owner".into(), managed.clone()),
        ],
    };
    let mut factory = function(6, vec![package.clone()], Type::Closure(ClosureId(2)), None);
    factory.locals[0].mutable = true;
    factory.values = vec![Type::Closure(ClosureId(1)), Type::Closure(ClosureId(2))];
    factory.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![
            instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
            instruction(
                Some(0),
                InstructionKind::TakePackageField {
                    local: LocalId(0),
                    index: 0,
                },
            ),
            instruction(
                Some(1),
                InstructionKind::MakeClosure {
                    closure: ClosureId(2),
                    captures: vec![ValueId(0)],
                },
            ),
            instruction(None, InstructionKind::DropPackage { local: LocalId(0) }),
            instruction(None, InstructionKind::StorageDead(LocalId(0))),
            instruction(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ],
        terminator: Terminator::Return(Some(ValueId(1))),
    }];
    factory.callable_return_contract = Some(CallableReturnContract {
        capture_sources: Some(Vec::new()),
        source: CallableReturnSource::Fresh,
        closure: ClosureId(2),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![0],
        lending_captures: vec![],
    });
    m.functions.push(factory);
    p.functions.push(FunctionPlan {
        function: FunctionId(6),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![LocalId(0)],
            initialized_out: vec![],
        }],
        holds: (0..2)
            .map(|value| ValueHold {
                value: ValueId(value),
                roots: vec![],
            })
            .collect(),
    });
    let mut fresh = function(7, vec![], Type::Closure(ClosureId(1)), None);
    fresh.values = vec![Type::Closure(ClosureId(1))];
    fresh.blocks = vec![BasicBlock {
        id: BlockId(0),
        instructions: vec![instruction(
            Some(0),
            InstructionKind::MakeClosure {
                closure: ClosureId(1),
                captures: vec![],
            },
        )],
        terminator: Terminator::Return(Some(ValueId(0))),
    }];
    fresh.callable_return_contract = Some(CallableReturnContract {
        capture_sources: Some(Vec::new()),
        source: CallableReturnSource::Fresh,
        closure: ClosureId(1),
        kind: CallableKind::Fn,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
        lending_captures: vec![],
    });
    m.functions.push(fresh);
    p.functions.push(FunctionPlan {
        function: FunctionId(7),
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![],
            initialized_out: vec![],
        }],
        holds: vec![ValueHold {
            value: ValueId(0),
            roots: vec![],
        }],
    });
    let f = &mut m.functions[4];
    f.values
        .extend([package, Type::I32, managed, Type::Closure(ClosureId(1))]);
    let made = &mut f.blocks[0].instructions[2];
    made.kind = InstructionKind::Call {
        function: FunctionId(6),
        arguments: vec![ValueId(4)],
    };
    f.blocks[0].instructions.splice(
        2..2,
        [
            instruction(Some(5), InstructionKind::Constant(Constant::I32(1))),
            instruction(
                Some(6),
                InstructionKind::MakeManaged {
                    fields: vec![ValueId(5)],
                },
            ),
            instruction(
                Some(7),
                InstructionKind::Call {
                    function: FunctionId(7),
                    arguments: vec![],
                },
            ),
            instruction(
                Some(4),
                InstructionKind::MakePackage {
                    fields: vec![ValueId(if supplied { 1 } else { 7 }), ValueId(6)],
                },
            ),
        ],
    );
    f.blocks[0].instructions.insert(
        0,
        instruction(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
    );
    f.blocks[0].instructions.extend([
        instruction(None, InstructionKind::StorageDead(LocalId(1))),
        instruction(None, InstructionKind::StorageDead(LocalId(0))),
        instruction(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ]);
    p.functions[4].blocks[0].initialized_out.clear();
    p.functions[4].holds.extend((4..8).map(|value| ValueHold {
        value: ValueId(value),
        roots: vec![],
    }));
    (m, p)
}
