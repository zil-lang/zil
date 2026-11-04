//! Hand-authored raw kernel cases; source evidence is kept in impl253_source.
use super::*;
use zil_base::{FileId, Fingerprint, ModuleId, SCHEMA_VERSION, Span, TargetProfile};

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
        reference_return_bound: None,
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
        reference_return_bound: None,
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
                life: None,
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
fn accepted(module: &TypedModule) {
    let limits = Limits::default();
    let expected = analyze(module, &limits).unwrap();
    let bytes = zil_ir::wire::encode_module_candidate(module, &expected, &limits).unwrap();
    let raw = zil_ir::wire::decode_module_candidate(&bytes, &limits)
        .unwrap()
        .0;
    assert_eq!(analyze(&raw, &limits).unwrap(), expected);
}
fn rejected(module: &TypedModule) {
    let limits = Limits::default();
    assert!(analyze(module, &limits).is_err());
    let bytes =
        zil_ir::wire::encode_module_candidate(module, &CandidatePlan::empty_for(module), &limits)
            .unwrap();
    let raw = zil_ir::wire::decode_module_candidate(&bytes, &limits)
        .unwrap()
        .0;
    assert!(analyze(&raw, &limits).is_err());
}

#[test]
fn parameter_return_proves_nonzero_index_and_each_mode_raw_wire() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        accepted(&base(kind));
    }
}

#[test]
fn parameter_return_rejects_wrong_index_cleanup_copy_and_same_type_new_instance() {
    let m = base(CallableKind::FnOnce);
    let mut bad = m.clone();
    bad.functions[1]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .source = CallableReturnSource::Parameter(0);
    rejected(&bad);
    let mut bad = m.clone();
    bad.functions[1].callable_contract.as_mut().unwrap().roles[0].cleanup = CallableCleanup::Owned;
    rejected(&bad);
    let mut bad = m.clone();
    bad.functions[1].blocks[0].instructions[1].kind = InstructionKind::Load(LocalId(1));
    rejected(&bad);
    let mut bad = m;
    bad.functions[1].blocks[0].instructions[1].kind = InstructionKind::MakeClosure {
        closure: ClosureId(0),
        captures: vec![],
    };
    rejected(&bad);
}

#[test]
fn repeatable_calls_before_return_preserve_mode_and_once_call_consumes_instance() {
    for kind in [CallableKind::Fn, CallableKind::FnMut] {
        let mut m = base(kind);
        let f = &mut m.functions[1];
        f.values.push(Type::I32);
        let call = if kind == CallableKind::Fn {
            InstructionKind::CallClosureShared {
                callee: LocalId(1),
                arguments: vec![],
            }
        } else {
            InstructionKind::CallClosureMut {
                callee: LocalId(1),
                arguments: vec![],
            }
        };
        f.blocks[0]
            .instructions
            .insert(1, op(Some(ValueId(1)), call));
        f.callable_contract.as_mut().unwrap().traces =
            CallableTraces::Finite(vec![CallableTrace {
                calls: vec![CallEvent { parameter: 1, kind }],
                exit: CallableExit::Return,
            }]);
        accepted(&m);
    }
    let mut m = base(CallableKind::FnOnce);
    let f = &mut m.functions[1];
    f.values.push(Type::I32);
    f.blocks[0].instructions.insert(
        2,
        op(
            Some(ValueId(1)),
            InstructionKind::CallClosureOnce {
                callee: ValueId(0),
                arguments: vec![],
            },
        ),
    );
    f.callable_contract.as_mut().unwrap().traces = CallableTraces::Finite(vec![CallableTrace {
        calls: vec![CallEvent {
            parameter: 1,
            kind: CallableKind::FnOnce,
        }],
        exit: CallableExit::Return,
    }]);
    rejected(&m);
}

#[test]
fn branch_and_loop_keep_the_same_position_without_global_origin_union() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        let mut m = base(kind);
        let f = &mut m.functions[1];
        f.values = vec![
            Type::Bool,
            Type::Closure(ClosureId(0)),
            Type::Closure(ClosureId(0)),
        ];
        let mut yes = vec![op(
            Some(ValueId(1)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        )];
        yes.extend(exit());
        let mut no = vec![op(
            Some(ValueId(2)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        )];
        no.extend(exit());
        f.blocks = vec![
            block(
                0,
                vec![
                    enter(),
                    op(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
                ],
                Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(1),
                    else_block: BlockId(2),
                },
            ),
            block(1, yes, Terminator::Return(Some(ValueId(1)))),
            block(2, no, Terminator::Return(Some(ValueId(2)))),
        ];
        accepted(&m);
        m.functions[1].blocks[2].instructions[0].kind = InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![],
        };
        rejected(&m);
        let mut m = base(kind);
        let f = &mut m.functions[1];
        f.values = vec![Type::Bool, Type::I32, Type::Closure(ClosureId(0))];
        let calls = if kind == CallableKind::FnOnce {
            vec![]
        } else {
            vec![CallEvent { parameter: 1, kind }]
        };
        let body = if kind == CallableKind::FnOnce {
            vec![]
        } else {
            vec![op(
                Some(ValueId(1)),
                if kind == CallableKind::Fn {
                    InstructionKind::CallClosureShared {
                        callee: LocalId(1),
                        arguments: vec![],
                    }
                } else {
                    InstructionKind::CallClosureMut {
                        callee: LocalId(1),
                        arguments: vec![],
                    }
                },
            )]
        };
        let mut end = vec![op(
            Some(ValueId(2)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        )];
        end.extend(exit());
        f.blocks = vec![
            block(0, vec![enter()], Terminator::Jump(BlockId(1))),
            block(
                1,
                vec![op(Some(ValueId(0)), InstructionKind::Load(LocalId(0)))],
                Terminator::Branch {
                    condition: ValueId(0),
                    then_block: BlockId(2),
                    else_block: BlockId(3),
                },
            ),
            block(2, body, Terminator::Jump(BlockId(1))),
            block(3, end, Terminator::Return(Some(ValueId(2)))),
        ];
        f.callable_contract.as_mut().unwrap().traces = CallableTraces::Flow(CallableFlow {
            entry: 0,
            nodes: vec![
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Edges(vec![1]),
                },
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Edges(vec![2, 3]),
                },
                CallableFlowNode {
                    calls,
                    next: CallableFlowNext::Edges(vec![1]),
                },
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Exit(CallableExit::Return),
                },
            ],
        });
        accepted(&m);
    }
}

#[test]
fn nested_return_substitutes_actual_source_and_never_upgrades_outer_once() {
    let mut m = base(CallableKind::FnOnce);
    let mut wrapper = m.functions[1].clone();
    wrapper.id = FunctionId(2);
    wrapper.name = "wrapper".into();
    wrapper.values = vec![
        Type::Closure(ClosureId(0)),
        Type::Bool,
        Type::Closure(ClosureId(0)),
    ];
    wrapper.callable_return_contract.as_mut().unwrap().kind = CallableKind::Fn;
    wrapper.callable_contract.as_mut().unwrap().roles[0].kind = CallableKind::Fn;
    let mut instructions = vec![
        enter(),
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        ),
        op(Some(ValueId(1)), InstructionKind::Load(LocalId(0))),
        op(
            Some(ValueId(2)),
            InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(1), ValueId(0)],
            },
        ),
    ];
    instructions.extend(exit());
    wrapper.blocks = vec![block(0, instructions, Terminator::Return(Some(ValueId(2))))];
    m.functions.push(wrapper);
    accepted(&m);
    let f = &mut m.functions[2];
    f.callable_return_contract.as_mut().unwrap().kind = CallableKind::FnOnce;
    f.callable_contract.as_mut().unwrap().roles[0].kind = CallableKind::FnOnce;
    f.locals.push(local(2, Type::Closure(ClosureId(0))));
    f.values.extend([Type::I32, Type::Closure(ClosureId(0))]);
    f.blocks[0].instructions.splice(
        4..4,
        [
            op(None, InstructionKind::StorageLive(LocalId(2))),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(2),
                    value: ValueId(2),
                },
            ),
            op(
                Some(ValueId(3)),
                InstructionKind::CallClosureShared {
                    callee: LocalId(2),
                    arguments: vec![],
                },
            ),
            op(
                Some(ValueId(4)),
                InstructionKind::TakeLocal { local: LocalId(2) },
            ),
            op(None, InstructionKind::StorageDead(LocalId(2))),
        ],
    );
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
    rejected(&m);
}

#[test]
fn moving_a_returned_callable_cannot_revive_a_borrow_of_its_old_position() {
    let mut m = base(CallableKind::FnMut);
    let f = &mut m.functions[1];
    f.values = vec![
        Type::Closure(ClosureId(0)),
        Type::Address {
            mutable: true,
            inner: Box::new(Type::Closure(ClosureId(0))),
        },
        Type::I32,
    ];
    let mut instructions = vec![
        enter(),
        op(
            Some(ValueId(1)),
            InstructionKind::BorrowLocalMut { local: LocalId(1) },
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::CallClosureBorrowed {
                callee: ValueId(1),
                kind: CallableKind::FnMut,
                arguments: vec![],
            },
        ),
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        ),
    ];
    instructions.extend(exit());
    f.blocks[0].instructions = instructions;
    f.callable_contract.as_mut().unwrap().traces = CallableTraces::Finite(vec![CallableTrace {
        calls: vec![CallEvent {
            parameter: 1,
            kind: CallableKind::FnMut,
        }],
        exit: CallableExit::Return,
    }]);
    accepted(&m);
    m.functions[1].blocks[0].instructions.swap(2, 3);
    rejected(&m);
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

#[test]
fn internal_owned_slot_has_per_element_identity_liveness_and_old_position_windows() {
    let m = owned_slot();
    accepted(&m);
    let mut stale = m.clone();
    stale.functions[1].blocks[0].instructions.swap(6, 7);
    rejected(&stale);
    let mut duplicate = m.clone();
    duplicate.functions[1]
        .values
        .push(Type::Closure(ClosureId(0)));
    duplicate.functions[1].blocks[0].instructions.insert(
        8,
        op(
            Some(ValueId(5)),
            InstructionKind::TakeOwnedArrayElement {
                local: LocalId(2),
                index: 0,
            },
        ),
    );
    rejected(&duplicate);
    let mut wrong = m;
    wrong.functions[1].blocks[0].instructions[7].kind = InstructionKind::TakeOwnedArrayElement {
        local: LocalId(2),
        index: 1,
    };
    rejected(&wrong);
}

#[test]
fn internal_owned_slot_can_restore_across_loop_but_not_replace_parameter_with_fresh_same_type() {
    let mut m = owned_slot();
    let f = &mut m.functions[1];
    f.values.extend([Type::Closure(ClosureId(0)), Type::Bool]);
    let initial = f.blocks[0].instructions[..5].to_vec();
    let loop_body = vec![
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
            Some(ValueId(5)),
            InstructionKind::TakeOwnedArrayElement {
                local: LocalId(2),
                index: 0,
            },
        ),
        op(
            None,
            InstructionKind::StoreOwnedArrayElement {
                local: LocalId(2),
                index: 0,
                value: ValueId(5),
            },
        ),
    ];
    let ending = f.blocks[0].instructions[7..].to_vec();
    f.blocks = vec![
        block(0, initial, Terminator::Jump(BlockId(1))),
        block(
            1,
            vec![op(Some(ValueId(6)), InstructionKind::Load(LocalId(0)))],
            Terminator::Branch {
                condition: ValueId(6),
                then_block: BlockId(2),
                else_block: BlockId(3),
            },
        ),
        block(2, loop_body, Terminator::Jump(BlockId(1))),
        block(3, ending, Terminator::Return(Some(ValueId(4)))),
    ];
    f.callable_contract.as_mut().unwrap().traces = CallableTraces::Flow(CallableFlow {
        entry: 0,
        nodes: vec![
            CallableFlowNode {
                calls: vec![],
                next: CallableFlowNext::Edges(vec![1]),
            },
            CallableFlowNode {
                calls: vec![],
                next: CallableFlowNext::Edges(vec![2, 3]),
            },
            CallableFlowNode {
                calls: vec![CallEvent {
                    parameter: 1,
                    kind: CallableKind::FnMut,
                }],
                next: CallableFlowNext::Edges(vec![1]),
            },
            CallableFlowNode {
                calls: vec![],
                next: CallableFlowNext::Exit(CallableExit::Return),
            },
        ],
    });
    accepted(&m);
    let mut m = owned_slot();
    let payload = m.resources[0].payload.clone();
    let f = &mut m.functions[1];
    f.locals.push(local(3, Type::Closure(ClosureId(0))));
    f.values.extend([
        Type::I32,
        Type::Managed(Box::new(payload)),
        Type::Closure(ClosureId(0)),
        Type::Closure(ClosureId(0)),
    ]);
    f.blocks[0].instructions.splice(
        8..8,
        [
            op(None, InstructionKind::StorageLive(LocalId(3))),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(3),
                    value: ValueId(4),
                },
            ),
            op(None, InstructionKind::DropClosure { local: LocalId(3) }),
            op(None, InstructionKind::StorageDead(LocalId(3))),
            op(
                Some(ValueId(5)),
                InstructionKind::Constant(Constant::I32(7)),
            ),
            op(
                Some(ValueId(6)),
                InstructionKind::MakeManaged {
                    fields: vec![ValueId(5)],
                },
            ),
            op(
                Some(ValueId(7)),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![ValueId(6)],
                },
            ),
            op(
                None,
                InstructionKind::StoreOwnedArrayElement {
                    local: LocalId(2),
                    index: 0,
                    value: ValueId(7),
                },
            ),
            op(
                Some(ValueId(8)),
                InstructionKind::TakeOwnedArrayElement {
                    local: LocalId(2),
                    index: 0,
                },
            ),
        ],
    );
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(8)));
    rejected(&m);
}

#[test]
fn forward_work_is_per_request_exact_and_not_a_cached_permission() {
    let m = owned_slot();
    let defaults = Limits::default();
    let mut budget = Budget::new(&defaults);
    let expected = analyze_budgeted(&m, &defaults, &mut budget, None).unwrap();
    let n = defaults.max_analysis_steps - budget.remaining();
    for steps in [n, n - 1, n] {
        let result = analyze(
            &m,
            &Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            },
        );
        if steps == n {
            assert_eq!(result.unwrap(), expected);
        } else {
            assert!(
                result
                    .unwrap_err()
                    .iter()
                    .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
    }
    let mut session = AnalysisSession::default();
    assert_eq!(session.analyze(&m, &defaults).unwrap(), expected);
    let mut stale = m.clone();
    stale.functions[1].blocks[0].instructions.swap(6, 7);
    assert!(session.analyze(&stale, &defaults).is_err());
    assert_eq!(session.analyze(&m, &defaults).unwrap(), expected);
    println!(
        "253 raw owning slot Analyze N={n}; N-1 ResourceLimit only; fresh N identical; incremental stale loan rejected"
    );
}

#[test]
fn returning_a_child_does_not_keep_a_borrow_of_its_old_capture_position_alive() {
    let mut m = owned_slot();
    let mut parent = m.functions[0].clone();
    parent.id = FunctionId(2);
    parent.name = "parent".into();
    parent.environment = Some(ClosureId(1));
    parent.locals = vec![local(0, Type::Closure(ClosureId(0)))];
    parent.values = vec![
        Type::Address {
            mutable: true,
            inner: Box::new(Type::Closure(ClosureId(0))),
        },
        Type::Closure(ClosureId(0)),
        Type::Bool,
        Type::Closure(ClosureId(0)),
        Type::I32,
    ];
    parent.blocks = vec![block(
        0,
        vec![
            enter(),
            op(None, InstructionKind::StorageLive(LocalId(0))),
            op(
                Some(ValueId(0)),
                InstructionKind::BorrowCapture {
                    index: 0,
                    mutable: true,
                },
            ),
            op(
                Some(ValueId(4)),
                InstructionKind::CallClosureBorrowed {
                    callee: ValueId(0),
                    kind: CallableKind::FnMut,
                    arguments: vec![],
                },
            ),
            op(Some(ValueId(1)), InstructionKind::TakeCapture { index: 0 }),
            op(
                Some(ValueId(2)),
                InstructionKind::Constant(Constant::Bool(false)),
            ),
            op(
                Some(ValueId(3)),
                InstructionKind::Call {
                    function: FunctionId(1),
                    arguments: vec![ValueId(2), ValueId(1)],
                },
            ),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(3),
                },
            ),
            op(None, InstructionKind::DropClosure { local: LocalId(0) }),
            op(None, InstructionKind::StorageDead(LocalId(0))),
            op(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ],
        Terminator::Return(Some(ValueId(4))),
    )];
    m.functions.push(parent);
    m.closures.push(ClosureDefinition {
        id: ClosureId(1),
        function: FunctionId(2),
        captures: vec![Type::Closure(ClosureId(0))],
        parameters: vec![],
        result: Type::I32,
        kind: CallableKind::FnOnce,
        lending_captures: vec![],
    });
    accepted(&m);
    m.functions[2].blocks[0].instructions.swap(3, 4);
    rejected(&m);
}

#[test]
fn self_contained_forward_type_cannot_hide_different_actual_capture_operands() {
    let mut m = owned_slot();
    let mut bad = m.functions[0].clone();
    bad.id = FunctionId(2);
    bad.name = "bad_constructor".into();
    bad.environment = None;
    bad.locals = vec![local(0, Type::Closure(ClosureId(0)))];
    bad.values = vec![Type::I32, Type::Closure(ClosureId(0))];
    bad.blocks = vec![block(
        0,
        vec![
            enter(),
            op(None, InstructionKind::StorageLive(LocalId(0))),
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
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(0),
                    value: ValueId(1),
                },
            ),
            op(None, InstructionKind::DropClosure { local: LocalId(0) }),
            op(None, InstructionKind::StorageDead(LocalId(0))),
            op(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ],
        Terminator::Return(Some(ValueId(0))),
    )];
    m.functions.push(bad);
    let diagnostics = analyze(&m, &Limits::default()).unwrap_err();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("actual capture type differs")),
        "{diagnostics:?}"
    );
    rejected(&m);
}

#[test]
fn symbolic_copy_capability_is_not_obtained_by_hiding_f_in_an_old_state_value() {
    let mut m = base(CallableKind::FnOnce);
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
    let state = Type::Package {
        name: "State".into(),
        fields: vec![
            ("r".into(), managed.clone()),
            ("callback".into(), Type::Closure(ClosureId(0))),
        ],
    };
    let f = &mut m.functions[1];
    f.locals.push(local(2, state.clone()));
    f.values.extend([Type::I32, managed, state]);
    let mut instructions = vec![
        enter(),
        op(None, InstructionKind::StorageLive(LocalId(2))),
        op(
            Some(ValueId(0)),
            InstructionKind::TakeLocal { local: LocalId(1) },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::Constant(Constant::I32(7)),
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::MakeManaged {
                fields: vec![ValueId(1)],
            },
        ),
        op(
            Some(ValueId(3)),
            InstructionKind::MakePackage {
                fields: vec![ValueId(2), ValueId(0)],
            },
        ),
        op(
            None,
            InstructionKind::Store {
                local: LocalId(2),
                value: ValueId(3),
            },
        ),
        op(None, InstructionKind::DropPackage { local: LocalId(2) }),
        op(None, InstructionKind::StorageDead(LocalId(2))),
    ];
    instructions.extend(exit());
    f.blocks[0].instructions = instructions;
    rejected(&m);
}

// The legacy bytes remain immutable; migration is a separate v34 fixture.
macro_rules! reviewed_fixture_v34 {
    ($name:literal) => {{
        let historical = include_bytes!(concat!("../tests/impl253_fixtures/", $name));
        let errors =
            zil_ir::wire::decode_module_candidate(historical, &Limits::default()).unwrap_err();
        assert_eq!(errors[0].code, "E-WIRE-VERSION");
        include_bytes!(concat!("../tests/impl253_fixtures/v34/", $name)).as_slice()
    }};
}

#[test]
fn reviewed_capture_bodies_are_window_roots_with_and_without_consumer_edges() {
    // Structured Wire34 migrations of the frozen Wire32 counterexamples and controls;
    // the producer candidate in each file grants no authority.
    for (valid, bytes) in [
        (
            true,
            reviewed_fixture_v34!("parent_valid_no_call_edge.wire"),
        ),
        (
            false,
            reviewed_fixture_v34!("parent_stale_no_call_edge.wire"),
        ),
        (
            true,
            reviewed_fixture_v34!("parent_valid_with_call_edge.wire"),
        ),
        (
            false,
            reviewed_fixture_v34!("parent_stale_with_call_edge.wire"),
        ),
    ] {
        let module = zil_ir::wire::decode_module_candidate(bytes, &Limits::default())
            .unwrap()
            .0;
        if valid {
            accepted(&module);
        } else {
            rejected(&module);
        }
    }
}

#[test]
fn reviewed_ordinary_arguments_and_symbolic_copy_authority_raw_wire() {
    for (valid, bytes) in [
        (
            false,
            reviewed_fixture_v34!("escape_byvalue_symbolic_fn_call.wire"),
        ),
        (
            false,
            reviewed_fixture_v34!("escape_callback_argument.wire"),
        ),
        (
            true,
            reviewed_fixture_v34!("control_shared_callback_i32_argument.wire"),
        ),
        (
            true,
            reviewed_fixture_v34!("control_concrete_copy_byvalue_call.wire"),
        ),
        (
            false,
            reviewed_fixture_v34!("escape_shared_callback_f_argument.wire"),
        ),
        (
            false,
            reviewed_fixture_v34!("escape_two_element_array.wire"),
        ),
        (false, reviewed_fixture_v34!("escape_thin_store.wire")),
    ] {
        let module = zil_ir::wire::decode_module_candidate(bytes, &Limits::default())
            .unwrap()
            .0;
        if valid {
            accepted(&module);
        } else {
            rejected(&module);
        }
    }
}
