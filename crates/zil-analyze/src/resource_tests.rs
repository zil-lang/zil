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

// Raw runtime input: two nominally equal F values can be independent actual
// storage positions. The body dependency is selected before or after borrowing.
fn scoped_fixture(mode: u8) -> TypedModule {
    let mut main = Body::new(0);
    main.op(InstructionKind::ScopeEnter { scope: ScopeId(0) });
    let f = main.local(Type::Closure(ClosureId(0)));
    let made = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![],
        },
    );
    main.op(InstructionKind::Store {
        local: f,
        value: made,
    });
    let copied =
        (mode == 2).then(|| main.value(Type::Closure(ClosureId(0)), InstructionKind::Load(f)));
    let address_ty = Type::Address {
        mutable: true,
        inner: Box::new(Type::Closure(ClosureId(0))),
    };
    let callback = main.value(
        address_ty.clone(),
        InstructionKind::BorrowLocalMut { local: f },
    );
    if mode == 1 {
        main.value(Type::Closure(ClosureId(0)), InstructionKind::Load(f));
    }
    let captures = match mode {
        2 => vec![copied.unwrap()],
        3 => vec![callback],
        4 => {
            let other = main.local(Type::I32);
            let value = main.constant(7);
            main.op(InstructionKind::Store {
                local: other,
                value,
            });
            vec![main.value(
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
                InstructionKind::BorrowLocal { local: other },
            )]
        }
        5 => {
            let other = main.local(Type::Closure(ClosureId(0)));
            let value = main.value(
                Type::Closure(ClosureId(0)),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![],
                },
            );
            main.op(InstructionKind::Store {
                local: other,
                value,
            });
            vec![main.value(
                address_ty.clone(),
                InstructionKind::BorrowLocalMut { local: other },
            )]
        }
        _ => vec![],
    };
    let body_captures = match mode {
        2 => vec![Type::Closure(ClosureId(0))],
        3 => vec![address_ty],
        4 => vec![Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        }],
        5 => vec![address_ty],
        _ => vec![],
    };
    let body = main.value(
        Type::Closure(ClosureId(1)),
        InstructionKind::MakeClosure {
            closure: ClosureId(1),
            captures,
        },
    );
    main.op(InstructionKind::UnsafeEnter { region: 7 });
    main.value(
        Type::Tuple(vec![Type::Bool, Type::I32]),
        InstructionKind::WithForeignCallback {
            register: FunctionId(3),
            close: FunctionId(4),
            callback,
            body,
            region: 7,
        },
    );
    main.op(InstructionKind::UnsafeExit { region: 7 });
    if matches!(mode, 4 | 5) {
        main.op(InstructionKind::StorageDead(LocalId(1)));
    }
    main.op(InstructionKind::StorageDead(f));
    let zero = main.constant(0);
    main.op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    let mut callback_entry = Body::new(1);
    callback_entry.0.environment = Some(ClosureId(0));
    callback_entry.0.locals.push(LocalDecl {
        id: LocalId(0),
        name: "event".into(),
        ty: Type::I32,
        mutable: false,
        span: s(),
    });
    callback_entry.0.parameters.push(LocalId(0));
    let event = callback_entry.value(Type::I32, InstructionKind::Load(LocalId(0)));
    let mut body_entry = Body::new(2);
    body_entry.0.environment = Some(ClosureId(1));
    let result = if mode == 5 {
        let address = body_entry.value(
            Type::Address {
                mutable: true,
                inner: Box::new(Type::Closure(ClosureId(0))),
            },
            InstructionKind::ReadCapture { index: 0 },
        );
        let arg = body_entry.constant(7);
        body_entry.value(
            Type::I32,
            InstructionKind::CallClosureBorrowed {
                callee: address,
                kind: CallableKind::FnMut,
                arguments: vec![arg],
            },
        )
    } else if mode == 4 {
        let address = body_entry.value(
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            InstructionKind::ReadCapture { index: 0 },
        );
        body_entry.value(Type::I32, InstructionKind::ReadAddress { address })
    } else {
        body_entry.constant(42)
    };
    let foreign = |id, role, parameters: Vec<Type>, result| {
        let mut f = Body::new(id).0;
        f.blocks.clear();
        f.return_type = result;
        f.locals = parameters
            .into_iter()
            .enumerate()
            .map(|(i, ty)| LocalDecl {
                id: LocalId(i as u32),
                name: format!("p{i}"),
                ty,
                mutable: false,
                span: s(),
            })
            .collect();
        f.parameters = f.locals.iter().map(|l| l.id).collect();
        f.foreign = Some(ForeignDeclaration {
            abi: "C".into(),
            symbol: format!("foreign{id}"),
            noescape_parameters: vec![],
            static_callbacks: true,
            scoped_callback: Some(role),
        });
        f
    };
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        resources: vec![],
        unsupported: vec![],
        closures: vec![
            ClosureDefinition {
                id: ClosureId(0),
                function: FunctionId(1),
                kind: CallableKind::FnMut,
                captures: vec![],
                parameters: vec![Type::I32],
                result: Type::I32,
                lending_captures: vec![],
            },
            ClosureDefinition {
                id: ClosureId(1),
                function: FunctionId(2),
                kind: CallableKind::FnOnce,
                captures: body_captures,
                parameters: vec![],
                result: Type::I32,
                lending_captures: vec![],
            },
        ],
        functions: vec![
            main.finish(zero),
            callback_entry.finish(event),
            body_entry.finish(result),
            foreign(
                3,
                ScopedCallbackRole::Register,
                vec![
                    Type::ForeignCallback {
                        parameters: vec![Type::ForeignHandle, Type::I32],
                        result: Box::new(Type::I64),
                    },
                    Type::ForeignHandle,
                ],
                Type::ForeignHandle,
            ),
            foreign(
                4,
                ScopedCallbackRole::Close,
                vec![Type::ForeignHandle],
                Type::Unit,
            ),
        ],
    }
}

#[test]
fn scoped_preparation_protects_actual_position_and_allows_prior_independent_copy() {
    for mode in 0..6 {
        let module = scoped_fixture(mode);
        let limits = Limits::default();
        let result = analyze(&module, &limits);
        assert_eq!(
            result.is_ok(),
            matches!(mode, 0 | 2 | 4 | 5),
            "mode={mode}: {result:?}"
        );
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(decoded, module);
        assert_eq!(analyze(&decoded, &limits), result);
    }
}

#[test]
fn scoped_raw_claims_cannot_forge_mutability_origin_or_storage_liveness() {
    for mutation in 0..5 {
        let mut module = scoped_fixture(0);
        let f = &mut module.functions[0];
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::BorrowLocalMut { .. }))
            .unwrap();
        let InstructionKind::BorrowLocalMut { local } = f.blocks[0].instructions[at].kind else {
            unreachable!()
        };
        match mutation {
            0 => f.blocks[0].instructions[at].kind = InstructionKind::BorrowLocal { local },
            1 => f.blocks[0].instructions[at].kind = InstructionKind::Constant(Constant::Unit),
            2 => f.locals[local.0 as usize].mutable = false,
            3 => {
                f.values.push(Type::Closure(ClosureId(0)));
                f.blocks[0].instructions.insert(
                    at + 1,
                    inst(
                        Some(ValueId((f.values.len() - 1) as u32)),
                        InstructionKind::TakeLocal { local },
                    ),
                );
            }
            4 => module.closures[0].function = FunctionId(2),
            _ => unreachable!(),
        }
        let limits = Limits::default();
        let errors = analyze(&module, &limits).unwrap_err();
        assert_eq!(errors[0].category, zil_base::ResultCategory::SafeReject);
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(analyze(&decoded, &limits).unwrap_err(), errors);
    }
}

#[test]
fn scoped_exclusive_interval_rejects_a_live_shared_alias_but_not_an_ended_one() {
    for live in [false, true] {
        let mut module = scoped_fixture(0);
        module.closures[0].kind = CallableKind::Fn;
        let f = &mut module.functions[0];
        let alias = ValueId(f.values.len() as u32);
        f.values.push(Type::Address {
            mutable: false,
            inner: Box::new(Type::Closure(ClosureId(0))),
        });
        let argument = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let result = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::BorrowLocalMut { .. }))
            .unwrap();
        f.blocks[0].instructions.insert(
            at,
            inst(
                Some(alias),
                InstructionKind::BorrowLocal { local: LocalId(0) },
            ),
        );
        f.blocks[0].instructions.insert(
            at + 1,
            inst(Some(argument), InstructionKind::Constant(Constant::I32(1))),
        );
        let at = if live {
            f.blocks[0]
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::StorageDead(_)))
                .unwrap()
        } else {
            at + 2
        };
        f.blocks[0].instructions.insert(
            at,
            inst(
                Some(result),
                InstructionKind::CallClosureBorrowed {
                    callee: alias,
                    kind: CallableKind::Fn,
                    arguments: vec![argument],
                },
            ),
        );
        let limits = Limits::default();
        let outcome = analyze(&module, &limits);
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(analyze(&decoded, &limits), outcome);
        if live {
            let errors = outcome.unwrap_err();
            assert_eq!(errors[0].code, "E-BORROW-CONFLICT");
        } else {
            outcome.unwrap();
        }
    }
}

#[test]
fn scoped_exclusive_interval_preserves_only_actual_reborrow_ancestors() {
    for stored in [false, true] {
        let mut module = scoped_fixture(0);
        let f = &mut module.functions[0];
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::WithForeignCallback { .. }))
            .unwrap();
        let InstructionKind::WithForeignCallback {
            callback: parent, ..
        } = f.blocks[0].instructions[at].kind
        else {
            unreachable!()
        };
        let address = f.values[parent.0 as usize].clone();
        let mut preparation = vec![];
        let parent_local = LocalId(f.locals.len() as u32);
        let parent = if stored {
            f.locals.push(LocalDecl {
                id: parent_local,
                name: "delegated".into(),
                ty: address.clone(),
                mutable: false,
                span: s(),
            });
            preparation.extend([
                inst(None, InstructionKind::StorageLive(parent_local)),
                inst(
                    None,
                    InstructionKind::Store {
                        local: parent_local,
                        value: parent,
                    },
                ),
            ]);
            let loaded = ValueId(f.values.len() as u32);
            f.values.push(address.clone());
            preparation.push(inst(Some(loaded), InstructionKind::Load(parent_local)));
            loaded
        } else {
            parent
        };
        let child = ValueId(f.values.len() as u32);
        f.values.push(address.clone());
        preparation.push(inst(
            Some(child),
            InstructionKind::ReborrowAddress {
                address: parent,
                mutable: true,
            },
        ));
        if let InstructionKind::WithForeignCallback { callback, .. } =
            &mut f.blocks[0].instructions[at].kind
        {
            *callback = child;
        }
        let offset = preparation.len();
        f.blocks[0].instructions.splice(at..at, preparation);
        let parent = if stored {
            let loaded = ValueId(f.values.len() as u32);
            f.values.push(address);
            f.blocks[0].instructions.insert(
                at + offset + 1,
                inst(Some(loaded), InstructionKind::Load(parent_local)),
            );
            loaded
        } else {
            parent
        };
        let arg = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let result = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let at = at + offset + 1 + usize::from(stored);
        f.blocks[0].instructions.splice(
            at..at,
            [
                inst(Some(arg), InstructionKind::Constant(Constant::I32(7))),
                inst(
                    Some(result),
                    InstructionKind::CallClosureBorrowed {
                        callee: parent,
                        kind: CallableKind::FnMut,
                        arguments: vec![arg],
                    },
                ),
            ],
        );
        if stored {
            f.blocks[0].instructions.insert(
                at + 2,
                inst(None, InstructionKind::StorageDead(parent_local)),
            );
        }
        let limits = Limits::default();
        let expected = analyze(&module, &limits).unwrap();
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(analyze(&decoded, &limits).unwrap(), expected);
    }
}

fn scoped_helper(body_kind: CallableKind) -> TypedModule {
    let mut module = scoped_fixture(0);
    module.closures[1].kind = body_kind;
    let mut f = Body::new(0);
    f.0.locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "callback".into(),
            ty: Type::Address {
                mutable: true,
                inner: Box::new(Type::Closure(ClosureId(0))),
            },
            mutable: false,
            span: s(),
        },
        LocalDecl {
            id: LocalId(1),
            name: "body".into(),
            ty: Type::Closure(ClosureId(1)),
            mutable: false,
            span: s(),
        },
    ];
    f.0.parameters = vec![LocalId(0), LocalId(1)];
    let callback = f.value(f.0.locals[0].ty.clone(), InstructionKind::Load(LocalId(0)));
    let body = f.value(
        f.0.locals[1].ty.clone(),
        InstructionKind::TakeLocal { local: LocalId(1) },
    );
    f.op(InstructionKind::UnsafeEnter { region: 7 });
    f.value(
        Type::Tuple(vec![Type::Bool, Type::I32]),
        InstructionKind::WithForeignCallback {
            register: FunctionId(3),
            close: FunctionId(4),
            callback,
            body,
            region: 7,
        },
    );
    f.op(InstructionKind::UnsafeExit { region: 7 });
    let result = f.constant(42);
    f.0.callable_contract = Some(CallableParameterContract {
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
                    next: CallableFlowNext::Edges(vec![1, 2, 3]),
                },
                CallableFlowNode {
                    calls: vec![CallEvent {
                        parameter: 0,
                        kind: CallableKind::FnMut,
                    }],
                    next: CallableFlowNext::Edges(vec![1, 2, 3]),
                },
                CallableFlowNode {
                    calls: vec![CallEvent {
                        parameter: 1,
                        kind: CallableKind::FnOnce,
                    }],
                    next: CallableFlowNext::Edges(vec![4, 3]),
                },
                CallableFlowNode {
                    calls: vec![],
                    next: CallableFlowNext::Exit(CallableExit::Return),
                },
                CallableFlowNode {
                    calls: vec![CallEvent {
                        parameter: 0,
                        kind: CallableKind::FnMut,
                    }],
                    next: CallableFlowNext::Edges(vec![4, 3]),
                },
            ],
        }),
    });
    module.functions[0] = f.finish(result);
    module
}

#[test]
fn scoped_helper_rebuilds_repeatable_f_and_single_consumed_body_flow() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        let module = scoped_helper(kind);
        analyze(&module, &Limits::default()).unwrap();
        if kind == CallableKind::Fn {
            let mut ordinary_bound = module.clone();
            ordinary_bound.functions[0]
                .callable_contract
                .as_mut()
                .unwrap()
                .roles[1]
                .kind = CallableKind::Fn;
            analyze(&ordinary_bound, &Limits::default()).unwrap();
        }
        let mut forged = module.clone();
        let CallableTraces::Flow(flow) = &mut forged.functions[0]
            .callable_contract
            .as_mut()
            .unwrap()
            .traces
        else {
            unreachable!()
        };
        flow.nodes[2].calls.clear();
        assert!(analyze(&forged, &Limits::default()).is_err());
        let mut reused = module;
        let instruction = reused.functions[0].blocks[0].instructions[3].clone();
        let output = ValueId(reused.functions[0].values.len() as u32);
        reused.functions[0]
            .values
            .push(Type::Tuple(vec![Type::Bool, Type::I32]));
        reused.functions[0].blocks[0].instructions.insert(
            4,
            Instruction {
                result: Some(output),
                ..instruction
            },
        );
        assert!(analyze(&reused, &Limits::default()).is_err());
    }
}

fn scoped_helper_with_local_body(kind: CallableKind, borrowed_scalar: bool) -> TypedModule {
    let mut module = scoped_helper(kind);
    if borrowed_scalar {
        let fixture = scoped_fixture(4);
        module.closures[1].captures = fixture.closures[1].captures.clone();
        module.functions[2] = fixture.functions[2].clone();
    }
    let f = &mut module.functions[0];
    f.parameters.truncate(1);
    let contract = f.callable_contract.as_mut().unwrap();
    contract.roles.truncate(1);
    let CallableTraces::Flow(flow) = &mut contract.traces else {
        unreachable!()
    };
    flow.nodes[2].calls.clear();
    let mut preparation = vec![inst(None, InstructionKind::StorageLive(LocalId(1)))];
    let captures = if borrowed_scalar {
        let local = LocalId(f.locals.len() as u32);
        f.locals.push(LocalDecl {
            id: local,
            name: "unrelated".into(),
            ty: Type::I32,
            mutable: false,
            span: s(),
        });
        let number = ValueId(f.values.len() as u32);
        f.values.push(Type::I32);
        let address = ValueId(f.values.len() as u32);
        f.values.push(module.closures[1].captures[0].clone());
        preparation.extend([
            inst(None, InstructionKind::StorageLive(local)),
            inst(Some(number), InstructionKind::Constant(Constant::I32(7))),
            inst(
                None,
                InstructionKind::Store {
                    local,
                    value: number,
                },
            ),
            inst(Some(address), InstructionKind::BorrowLocal { local }),
        ]);
        let at = f.blocks[0].instructions.len() - 1;
        f.blocks[0]
            .instructions
            .insert(at, inst(None, InstructionKind::StorageDead(local)));
        vec![address]
    } else {
        vec![]
    };
    let made = ValueId(f.values.len() as u32);
    f.values.push(Type::Closure(ClosureId(1)));
    preparation.extend([
        inst(
            Some(made),
            InstructionKind::MakeClosure {
                closure: ClosureId(1),
                captures,
            },
        ),
        inst(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: made,
            },
        ),
    ]);
    if kind != CallableKind::FnOnce && !borrowed_scalar {
        f.blocks[0].instructions[1].kind = InstructionKind::Load(LocalId(1));
    }
    f.blocks[0].instructions.splice(1..1, preparation);
    let at = f.blocks[0].instructions.len() - 1;
    f.blocks[0]
        .instructions
        .insert(at, inst(None, InstructionKind::StorageDead(LocalId(1))));
    module
}

#[test]
fn scoped_f_only_helper_tracks_local_concrete_body_without_inventing_a_role() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        for borrowed_scalar in [false, true] {
            let module = scoped_helper_with_local_body(kind, borrowed_scalar);
            let limits = Limits::default();
            let expected = analyze(&module, &limits).unwrap();
            let bytes = zil_ir::wire::encode_module_candidate(
                &module,
                &CandidatePlan::empty_for(&module),
                &limits,
            )
            .unwrap();
            let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
            assert_eq!(analyze(&decoded, &limits).unwrap(), expected);
            let mut forged = module.clone();
            let CallableTraces::Flow(flow) = &mut forged.functions[0]
                .callable_contract
                .as_mut()
                .unwrap()
                .traces
            else {
                unreachable!()
            };
            flow.nodes[2].calls.push(CallEvent {
                parameter: 0,
                kind: CallableKind::FnOnce,
            });
            assert!(analyze(&forged, &limits).is_err());
            let mut reused = module;
            let f = &mut reused.functions[0];
            let at = f.blocks[0]
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::WithForeignCallback { .. }))
                .unwrap();
            let mut second = f.blocks[0].instructions[at].clone();
            second.result = Some(ValueId(f.values.len() as u32));
            f.values.push(Type::Tuple(vec![Type::Bool, Type::I32]));
            f.blocks[0].instructions.insert(at + 1, second);
            assert!(analyze(&reused, &limits).is_err());
        }
    }
}

#[test]
fn scoped_windows_and_flow_spend_the_current_request_and_recover_fresh() {
    for module in [
        scoped_fixture(4),
        scoped_helper(CallableKind::Fn),
        scoped_helper_with_local_body(CallableKind::FnOnce, true),
    ] {
        let expected = analyze(&module, &Limits::default()).unwrap();
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if analyze(
                &module,
                &Limits {
                    max_analysis_steps: mid,
                    ..Limits::default()
                },
            )
            .is_ok()
            {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        let exact = Limits {
            max_analysis_steps: low,
            ..Limits::default()
        };
        for _ in 0..2 {
            assert_eq!(analyze(&module, &exact).unwrap(), expected);
        }
        assert_eq!(
            analyze(
                &module,
                &Limits {
                    max_analysis_steps: low - 1,
                    ..exact
                }
            )
            .unwrap_err()[0]
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
    }
}

#[test]
fn scoped_wrapper_checks_actual_argument_aliases_and_keeps_disjoint_borrowing() {
    for mode in [0, 3, 4, 5] {
        let mut module = scoped_fixture(mode);
        let mut helper = scoped_helper(CallableKind::FnOnce).functions.remove(0);
        helper.id = FunctionId(5);
        let body = &mut helper.callable_contract.as_mut().unwrap().roles[1];
        if matches!(mode, 3..=5) {
            body.hold = vec![0];
            body.call = vec![0];
            if mode != 4 {
                body.writes = vec![0];
            }
        }
        module.functions.push(helper);
        let f = &mut module.functions[0];
        for instruction in &mut f.blocks[0].instructions {
            if let InstructionKind::WithForeignCallback { callback, body, .. } = instruction.kind {
                f.values[instruction.result.unwrap().0 as usize] = Type::I32;
                instruction.kind = InstructionKind::Call {
                    function: FunctionId(5),
                    arguments: vec![callback, body],
                };
            }
        }
        let result = analyze(&module, &Limits::default());
        assert_eq!(result.is_ok(), mode != 3, "mode={mode}: {result:?}");
    }
}
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
fn impl247_owned_signature_does_not_grant_state_array_or_opaque_profiles() {
    let base = owned_callback_fixture(CallableKind::FnOnce, 1);
    for (valid, ty) in [
        (
            false,
            Type::Package {
                name: "State".into(),
                fields: vec![
                    ("token".into(), managed()),
                    ("callback".into(), Type::Closure(ClosureId(0))),
                ],
            },
        ),
        // The original Array1 case only declares an unused empty local. It
        // creates no SSA responsibility and is a valid private carrier slot.
        (
            true,
            Type::Array {
                element: Box::new(Type::Closure(ClosureId(0))),
                length: 1,
            },
        ),
        // A separate Array2 negative retains the public container boundary.
        (
            false,
            Type::Array {
                element: Box::new(Type::Closure(ClosureId(0))),
                length: 2,
            },
        ),
    ] {
        let mut m = base.clone();
        let local = LocalId(m.functions[0].locals.len() as u32);
        m.functions[0].locals.push(LocalDecl {
            id: local,
            name: "unused_container".into(),
            ty,
            mutable: false,
            span: s(),
        });
        let result = analyze(&m, &Limits::default());
        if valid {
            assert!(result.is_ok(), "{result:?}");
        } else {
            let errors = result.unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.message.contains("does not grant a State")),
                "{errors:?}"
            );
        }
        let bytes = zil_ir::wire::encode_module_candidate(
            &m,
            &CandidatePlan::empty_for(&m),
            &Limits::default(),
        )
        .unwrap();
        let raw = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default())
            .unwrap()
            .0;
        assert_eq!(analyze(&raw, &Limits::default()).is_ok(), valid);
    }
    let mut m = base;
    m.functions[0].callable_return_contract = Some(CallableReturnContract {
        source: zil_ir::CallableReturnSource::Fresh,
        closure: ClosureId(0),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
        lending_captures: vec![],
    });
    let errors = analyze(&m, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("does not grant an opaque factory")),
        "{errors:?}"
    );
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

#[test]
fn private_empty_slot_permission_does_not_discharge_an_unstored_owned_ssa() {
    let mut m = owned_callback_fixture(CallableKind::FnOnce, 1);
    let f = &mut m.functions[0];
    let closure = ValueId(f.values.len() as u32);
    let array = ValueId(closure.0 + 1);
    f.values.extend([
        Type::Closure(ClosureId(0)),
        Type::Array {
            element: Box::new(Type::Closure(ClosureId(0))),
            length: 1,
        },
    ]);
    let at = f.blocks[0].instructions.len() - 1;
    f.blocks[0].instructions.splice(
        at..at,
        [
            inst(
                Some(closure),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![],
                },
            ),
            inst(
                Some(array),
                InstructionKind::MakeOwnedArray {
                    elements: vec![closure],
                },
            ),
        ],
    );
    assert!(analyze(&m, &Limits::default()).is_err());
    let bytes = zil_ir::wire::encode_module_candidate(
        &m,
        &CandidatePlan::empty_for(&m),
        &Limits::default(),
    )
    .unwrap();
    let raw = zil_ir::wire::decode_module_candidate(&bytes, &Limits::default())
        .unwrap()
        .0;
    assert!(analyze(&raw, &Limits::default()).is_err());
}
