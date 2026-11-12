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

#[path = "capture_tests.rs"]
mod capture_tests;

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
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        resources: vec![],
        unsupported: vec![],
        closures: vec![
            ClosureDefinition {
                storage_life: None,
                id: ClosureId(0),
                function: FunctionId(1),
                kind: CallableKind::FnMut,
                captures: vec![],
                parameters: vec![Type::I32],
                result: Type::I32,
                lending_captures: vec![],
            },
            ClosureDefinition {
                storage_life: None,
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
    ]
    .into();
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

// This independent raw builder neither imports Verify nor uses a frontend
// ownership certificate. The local body keeps the actual owned capture for
// With's remaining cleanup, including the registration-null outcome.
fn scoped_helper_with_local_owner(package: bool) -> (TypedModule, ValueId) {
    let mut module = scoped_helper_with_local_body(CallableKind::FnOnce, false);
    let capture_type = if package {
        Type::Package {
            name: "LocalOwned".into(),
            fields: vec![("token".into(), managed())],
        }
    } else {
        managed()
    };
    module.resources.push(ResourceDefinition {
        name: "Token".into(),
        payload: payload(),
        teardown: None,
    });
    module.closures[1].captures = vec![capture_type.clone()];
    let f = &mut module.functions[0];
    let number = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    let owner = ValueId(f.values.len() as u32);
    f.values.push(managed());
    let mut preparation = vec![
        inst(Some(number), InstructionKind::Constant(Constant::I32(19))),
        inst(
            Some(owner),
            InstructionKind::MakeManaged {
                fields: vec![number],
            },
        ),
    ];
    let captured = if package {
        let value = ValueId(f.values.len() as u32);
        f.values.push(capture_type.clone());
        preparation.push(inst(
            Some(value),
            InstructionKind::MakePackage {
                fields: vec![owner],
            },
        ));
        value
    } else {
        owner
    };
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
        .unwrap();
    let InstructionKind::MakeClosure { captures, .. } = &mut f.blocks[0].instructions[at].kind
    else {
        unreachable!()
    };
    *captures = vec![captured];
    f.blocks[0].instructions.splice(at..at, preparation);
    if package {
        f.blocks[0].instructions.insert(
            0,
            inst(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        );
        f.blocks[0].instructions.extend([
            inst(None, InstructionKind::StorageDead(LocalId(0))),
            inst(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ]);
        let entry = &mut module.functions[2];
        entry.locals.push(LocalDecl {
            id: LocalId(0),
            name: "owned_view".into(),
            ty: capture_type,
            mutable: true,
            span: s(),
        });
        entry.blocks[0].instructions.insert(
            0,
            inst(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        );
        entry.blocks[0].instructions.insert(
            1,
            inst(
                None,
                InstructionKind::BindPackageCapture {
                    local: LocalId(0),
                    index: 0,
                },
            ),
        );
        entry.blocks[0].instructions.extend([
            inst(
                None,
                InstructionKind::EndPackageCapture {
                    local: LocalId(0),
                    index: 0,
                },
            ),
            inst(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ]);
    }
    (module, captured)
}

fn check_scoped_local_owner(module: &TypedModule, message: Option<&str>) {
    let limits = Limits::default();
    let raw = analyze(module, &limits);
    if let Some(message) = message {
        let errors = raw.as_ref().unwrap_err();
        assert_eq!(errors[0].category, zil_base::ResultCategory::SafeReject);
        assert!(errors[0].message.contains(message), "{errors:?}");
    } else {
        raw.as_ref().unwrap();
    }
    let bytes =
        zil_ir::wire::encode_module_candidate(module, &CandidatePlan::empty_for(module), &limits)
            .unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(analyze(&decoded, &limits), raw);
}

#[test]
fn scoped_local_owner_preparation_preserves_actual_ownership_through_with() {
    for package in [false, true] {
        for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            let (mut module, _) = scoped_helper_with_local_owner(package);
            module.closures[1].kind = kind;
            check_scoped_local_owner(&module, None);
        }
        let (mut leaked, _) = scoped_helper_with_local_owner(package);
        let f = &mut leaked.functions[0];
        let leak = ValueId(f.values.len() as u32);
        f.values.push(managed());
        let InstructionKind::MakeManaged { fields } = f.blocks[0]
            .instructions
            .iter()
            .find(|i| matches!(i.kind, InstructionKind::MakeManaged { .. }))
            .unwrap()
            .kind
            .clone()
        else {
            unreachable!()
        };
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
            .unwrap();
        f.blocks[0].instructions.insert(
            at,
            inst(Some(leak), InstructionKind::MakeManaged { fields }),
        );
        check_scoped_local_owner(
            &leaked,
            Some("normal return leaves a responsibility unpaid"),
        );
    }
    let (mut duplicated, captured) = scoped_helper_with_local_owner(false);
    duplicated.closures[1].captures.push(managed());
    for i in &mut duplicated.functions[0].blocks[0].instructions {
        if let InstructionKind::MakeClosure { captures, .. } = &mut i.kind {
            captures.push(captured);
        }
    }
    check_scoped_local_owner(
        &duplicated,
        Some("SSA responsibility was already consumed or absent"),
    );
}

#[test]
fn scoped_local_owner_body_cannot_hide_the_actual_callback_alias() {
    for package in [false, true] {
        let (mut module, _) = scoped_helper_with_local_owner(package);
        let address = module.functions[0].values[0].clone();
        module.closures[1].captures.push(address.clone());
        let f = &mut module.functions[0];
        let alias = ValueId(f.values.len() as u32);
        f.values.push(address);
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
            .unwrap();
        f.blocks[0].instructions.insert(
            at,
            inst(
                Some(alias),
                InstructionKind::ReborrowAddress {
                    address: ValueId(0),
                    mutable: true,
                },
            ),
        );
        let InstructionKind::MakeClosure { captures, .. } =
            &mut f.blocks[0].instructions[at + 1].kind
        else {
            unreachable!()
        };
        captures.push(alias);
        check_scoped_local_owner(&module, Some("symbolic F"));
    }
}

#[test]
fn ordinary_helper_local_owner_needs_no_with_permission() {
    let mut module = scoped_helper(CallableKind::FnOnce);
    module.resources.push(ResourceDefinition {
        name: "Token".into(),
        payload: payload(),
        teardown: None,
    });
    let contract = module.functions[0].callable_contract.as_mut().unwrap();
    contract.roles.truncate(1);
    contract.traces = CallableTraces::Finite(vec![CallableTrace {
        calls: vec![CallEvent {
            parameter: 0,
            kind: CallableKind::FnMut,
        }],
        exit: CallableExit::Return,
    }]);
    let contract = contract.clone();
    let mut f = Body::new(0);
    f.0.locals.push(module.functions[0].locals[0].clone());
    f.0.parameters.push(LocalId(0));
    f.0.callable_contract = Some(contract);
    let owned = f.local(managed());
    let number = f.constant(19);
    let value = f.value(
        managed(),
        InstructionKind::MakeManaged {
            fields: vec![number],
        },
    );
    f.op(InstructionKind::Store {
        local: owned,
        value,
    });
    let callback = f.value(f.0.locals[0].ty.clone(), InstructionKind::Load(LocalId(0)));
    let result = f.value(
        Type::I32,
        InstructionKind::CallClosureBorrowed {
            callee: callback,
            kind: CallableKind::FnMut,
            arguments: vec![number],
        },
    );
    f.op(InstructionKind::DropManaged { local: owned });
    f.op(InstructionKind::StorageDead(owned));
    module.functions[0] = f.finish(result);
    check_scoped_local_owner(&module, None);
    module.functions[0].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropManaged { .. }));
    check_scoped_local_owner(&module, Some("storage boundary discards an owned carrier"));
}

#[test]
fn scoped_local_owner_work_is_prepaid_and_recovers_fresh() {
    for package in [false, true] {
        let (module, _) = scoped_helper_with_local_owner(package);
        let defaults = Limits::default();
        let mut budget = Budget::new(&defaults);
        let expected = analyze_budgeted(&module, &defaults, &mut budget, None).unwrap();
        let exact = defaults.max_analysis_steps - budget.remaining();
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &defaults,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &defaults).unwrap();
        for input in [&module, &decoded] {
            for steps in [exact, exact - 1, exact] {
                let result = analyze(
                    input,
                    &Limits {
                        max_analysis_steps: steps,
                        ..defaults.clone()
                    },
                );
                if steps == exact {
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
        }
        println!(
            "local owner package={package} Analyze raw=wire N={exact}; N-1 ResourceLimit; fresh equality"
        );
    }
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
            let event = CallEvent {
                parameter: 0,
                kind: CallableKind::FnMut,
            };
            f.callable_contract.as_mut().unwrap().traces = CallableTraces::Flow(CallableFlow {
                entry: 0,
                nodes: vec![
                    CallableFlowNode {
                        calls: vec![],
                        next: CallableFlowNext::Edges(vec![1, 2, 3]),
                    },
                    CallableFlowNode {
                        calls: vec![event],
                        next: CallableFlowNext::Edges(vec![1, 2, 3]),
                    },
                    CallableFlowNode {
                        calls: vec![],
                        next: CallableFlowNext::Edges(vec![4, 3]),
                    },
                    CallableFlowNode {
                        calls: vec![],
                        next: CallableFlowNext::Edges(vec![5, 6, 7]),
                    },
                    CallableFlowNode {
                        calls: vec![event],
                        next: CallableFlowNext::Edges(vec![4, 3]),
                    },
                    CallableFlowNode {
                        calls: vec![event],
                        next: CallableFlowNext::Edges(vec![5, 6, 7]),
                    },
                    CallableFlowNode {
                        calls: vec![],
                        next: CallableFlowNext::Edges(vec![8, 7]),
                    },
                    CallableFlowNode {
                        calls: vec![],
                        next: CallableFlowNext::Exit(CallableExit::Return),
                    },
                    CallableFlowNode {
                        calls: vec![event],
                        next: CallableFlowNext::Edges(vec![8, 7]),
                    },
                ],
            });
            let actual = analyze(&reused, &limits);
            // A concrete Copy body supplies independent copies to the two
            // calls. The consumed generic B role has no such Copy authority.
            if kind != CallableKind::FnOnce && !borrowed_scalar {
                actual.as_ref().unwrap();
            } else {
                assert_eq!(
                    actual.as_ref().unwrap_err()[0].message,
                    "callable instance is moved, absent or cleaned on a reachable path"
                );
            }
            let bytes = zil_ir::wire::encode_module_candidate(
                &reused,
                &CandidatePlan::empty_for(&reused),
                &limits,
            )
            .unwrap();
            let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
            assert_eq!(analyze(&decoded, &limits), actual);
        }
    }
}

#[test]
fn ordinary_once_calls_preserve_concrete_copy_value_semantics() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        let mut module = scoped_fixture(0);
        module.closures[1].kind = kind;
        let f = &mut module.functions[0];
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::WithForeignCallback { .. }))
            .unwrap();
        let instruction = &mut f.blocks[0].instructions[at];
        let InstructionKind::WithForeignCallback { body, .. } = instruction.kind else {
            unreachable!()
        };
        instruction.kind = InstructionKind::CallClosureOnce {
            callee: body,
            arguments: vec![],
        };
        f.values[instruction.result.unwrap().0 as usize] = Type::I32;
        let mut second = instruction.clone();
        second.result = Some(ValueId(f.values.len() as u32));
        f.values.push(Type::I32);
        f.blocks[0].instructions.insert(at + 1, second);
        let limits = Limits::default();
        let actual = analyze(&module, &limits);
        if kind == CallableKind::FnOnce {
            assert_eq!(
                actual.as_ref().unwrap_err()[0].message,
                "resource closure: call uses consumed environment"
            );
        } else {
            actual.as_ref().unwrap();
        }
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(analyze(&decoded, &limits), actual);
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
            storage_life: None,
            call_kind: zil_ir::CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
            id: FunctionId(id),
            name: format!("f{id}"),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::I32,
            result_mode: PassingMode::Infer,
            locals: vec![].into(),
            values: vec![].into(),
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![],
                terminator: Terminator::Return(None),
            }]
            .into(),
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
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        functions: vec![main.finish(result), entry.finish(result_entry)],
        closures: vec![ClosureDefinition {
            storage_life: None,
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
fn owned_callbacks_consume_fresh_arguments_and_return_incoming_owners() {
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
fn owned_callback_arguments_and_results_have_one_responsibility() {
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
fn owned_callback_definition_and_actual_call_are_checked_independently() {
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
fn owned_callback_result_cannot_export_created_freshness() {
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
fn owned_callback_budget_is_module_scoped_and_recovers_fresh() {
    let m = owned_callback_fixture(CallableKind::FnMut, 2);
    let mut budget = Budget::new(&Limits::default());
    let expected = analyze_budgeted(&m, &Limits::default(), &mut budget, None).unwrap();
    let exact = Limits::default().max_analysis_steps - budget.remaining();
    println!("owned callback Analyze N={exact}; N-1 ResourceLimit; fresh equality");
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
fn callback_data_cannot_smuggle_a_callback_state_through_store() {
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
        storage_life: None,
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
fn owned_signature_does_not_grant_state_array_or_opaque_profiles() {
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

// 282: concrete local callable composition in an ordinary symbolic helper.
// These raw values are constructed independently of Types and Verify.
fn local_composition(child: bool, captured: bool) -> TypedModule {
    let mut module = scoped_helper_with_local_body(CallableKind::FnOnce, false);
    let f = &mut module.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::WithForeignCallback { .. }))
        .unwrap();
    let InstructionKind::WithForeignCallback { callback, body, .. } =
        f.blocks[0].instructions[at].kind
    else {
        unreachable!()
    };
    let arg = ValueId(f.values.len() as u32);
    let called_f = ValueId(arg.0 + 1);
    f.values.extend([Type::I32, Type::I32]);
    f.blocks[0].instructions.splice(
        at..at,
        [
            inst(Some(arg), InstructionKind::Constant(Constant::I32(1))),
            inst(
                Some(called_f),
                InstructionKind::CallClosureBorrowed {
                    callee: callback,
                    kind: CallableKind::FnMut,
                    arguments: vec![arg],
                },
            ),
        ],
    );
    let at = at + 2;
    let result = f.blocks[0].instructions[at].result.unwrap();
    f.values[result.0 as usize] = Type::I32;
    let mut actual = body;
    if captured {
        let made = ValueId(f.values.len() as u32);
        f.values.push(Type::Closure(ClosureId(2)));
        f.blocks[0].instructions.insert(
            at,
            inst(
                Some(made),
                InstructionKind::MakeClosure {
                    closure: ClosureId(2),
                    captures: vec![body],
                },
            ),
        );
        actual = made;
        let mut entry = Body::new(5);
        entry.0.environment = Some(ClosureId(2));
        let taken = entry.value(
            Type::Closure(ClosureId(1)),
            InstructionKind::TakeCapture { index: 0 },
        );
        let result = entry.value(
            Type::I32,
            InstructionKind::CallClosureOnce {
                callee: taken,
                arguments: vec![],
            },
        );
        module.functions.push(entry.finish(result));
        module.closures.push(ClosureDefinition {
            storage_life: None,
            id: ClosureId(2),
            function: FunctionId(5),
            captures: vec![Type::Closure(ClosureId(1))],
            parameters: vec![],
            result: Type::I32,
            kind: CallableKind::FnOnce,
            lending_captures: vec![],
        });
    }
    let f = &mut module.functions[0];
    f.blocks[0].instructions[at + usize::from(captured)].kind = if child {
        InstructionKind::Call {
            function: FunctionId(5),
            arguments: vec![actual],
        }
    } else {
        InstructionKind::CallClosureOnce {
            callee: actual,
            arguments: vec![],
        }
    };
    f.blocks[0].instructions.retain(|i| {
        !matches!(
            i.kind,
            InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. }
        )
    });
    f.callable_contract.as_mut().unwrap().traces = CallableTraces::Finite(vec![CallableTrace {
        calls: vec![CallEvent {
            parameter: 0,
            kind: CallableKind::FnMut,
        }],
        exit: CallableExit::Return,
    }]);
    if child {
        assert!(!captured);
        let mut entry = Body::new(5);
        entry.0.locals.push(LocalDecl {
            id: LocalId(0),
            name: "handler".into(),
            ty: Type::Closure(ClosureId(1)),
            mutable: false,
            span: s(),
        });
        entry.0.parameters.push(LocalId(0));
        let actual = entry.value(
            Type::Closure(ClosureId(1)),
            InstructionKind::TakeLocal { local: LocalId(0) },
        );
        let result = entry.value(
            Type::I32,
            InstructionKind::CallClosureOnce {
                callee: actual,
                arguments: vec![],
            },
        );
        entry.0.callable_contract = Some(CallableParameterContract {
            roles: vec![CallableRole {
                life: None,
                parameter: 0,
                closure: ClosureId(1),
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
        module.functions.push(entry.finish(result));
    }
    module
}

fn result_wire(module: &TypedModule, accepted: bool) {
    let limits = Limits::default();
    let actual = analyze(module, &limits);
    assert_eq!(actual.is_ok(), accepted, "{actual:?}");
    let bytes =
        zil_ir::wire::encode_module_candidate(module, &CandidatePlan::empty_for(module), &limits)
            .unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
        .unwrap()
        .0;
    assert_eq!(analyze(&decoded, &limits), actual);
}

#[test]
fn concrete_local_calls_child_and_capture_do_not_invent_f_events_raw_wire() {
    for (child, captured) in [(false, false), (true, false), (false, true)] {
        let module = local_composition(child, captured);
        result_wire(&module, true);
        let mut forged = module.clone();
        let CallableTraces::Finite(paths) = &mut forged.functions[0]
            .callable_contract
            .as_mut()
            .unwrap()
            .traces
        else {
            unreachable!()
        };
        paths[0].calls.clear();
        result_wire(&forged, false);
        let mut invented = module.clone();
        let CallableTraces::Finite(paths) = &mut invented.functions[0]
            .callable_contract
            .as_mut()
            .unwrap()
            .traces
        else {
            unreachable!()
        };
        paths[0].calls.push(CallEvent {
            parameter: 0,
            kind: CallableKind::FnOnce,
        });
        result_wire(&invented, false);
    }
}

#[test]
fn concrete_local_rejects_actual_f_capture_and_local_reuse_raw_wire() {
    let mut hidden = local_composition(false, true);
    let f = &mut hidden.functions[0];
    let callback = ValueId(0);
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                InstructionKind::MakeClosure {
                    closure: ClosureId(2),
                    ..
                }
            )
        })
        .unwrap();
    let InstructionKind::MakeClosure { captures, .. } = &mut f.blocks[0].instructions[at].kind
    else {
        unreachable!()
    };
    *captures = vec![callback];
    hidden.closures[2].captures = vec![f.values[callback.0 as usize].clone()];
    result_wire(&hidden, false);

    let mut reused = local_composition(true, false);
    let f = &mut reused.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                InstructionKind::Call {
                    function: FunctionId(5),
                    ..
                }
            )
        })
        .unwrap();
    let mut second = f.blocks[0].instructions[at].clone();
    second.result = Some(ValueId(f.values.len() as u32));
    f.values.push(Type::I32);
    f.blocks[0].instructions.insert(at + 1, second);
    result_wire(&reused, false);

    let mut upgraded = local_composition(true, false);
    upgraded.functions[5]
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[0]
        .kind = CallableKind::Fn;
    result_wire(&upgraded, false);
}

#[test]
fn concrete_local_work_is_prepaid_n_minus_one_and_fresh_raw_wire() {
    for module in [
        local_composition(false, false),
        local_composition(true, false),
        local_composition(false, true),
    ] {
        let defaults = Limits::default();
        let mut budget = Budget::new(&defaults);
        let expected = analyze_budgeted(&module, &defaults, &mut budget, None).unwrap();
        let exact = defaults.max_analysis_steps - budget.remaining();
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &defaults,
        )
        .unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&bytes, &defaults)
            .unwrap()
            .0;
        for input in [&module, &decoded] {
            for steps in [exact, exact - 1, exact] {
                let actual = analyze(
                    input,
                    &Limits {
                        max_analysis_steps: steps,
                        ..defaults.clone()
                    },
                );
                if steps == exact {
                    assert_eq!(actual.unwrap(), expected);
                } else {
                    assert_eq!(
                        actual.unwrap_err()[0].category,
                        zil_base::ResultCategory::ResourceLimit
                    );
                }
            }
        }
        println!(
            "282 {} raw=Wire N={exact}; N-1 ResourceLimit; fresh equality",
            module.functions.len()
        );
    }
}

#[test]
fn concrete_local_child_mixed_roles_preserve_actual_f_order_raw_wire() {
    let mut module = local_composition(true, false);
    let callback_type = module.functions[0].locals[0].ty.clone();
    let f = &mut module.functions[0];
    let call = f.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| {
            matches!(
                i.kind,
                InstructionKind::Call {
                    function: FunctionId(5),
                    ..
                }
            )
        })
        .unwrap();
    let InstructionKind::Call { arguments, .. } = &mut call.kind else {
        unreachable!()
    };
    arguments.insert(0, ValueId(0));
    let CallableTraces::Finite(paths) = &mut f.callable_contract.as_mut().unwrap().traces else {
        unreachable!()
    };
    paths[0].calls.push(CallEvent {
        parameter: 0,
        kind: CallableKind::FnMut,
    });
    let mut entry = Body::new(5);
    entry.0.locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "f".into(),
            ty: callback_type.clone(),
            mutable: false,
            span: s(),
        },
        LocalDecl {
            id: LocalId(1),
            name: "h".into(),
            ty: Type::Closure(ClosureId(1)),
            mutable: false,
            span: s(),
        },
    ]
    .into();
    entry.0.parameters = vec![LocalId(0), LocalId(1)];
    let callback = entry.value(callback_type, InstructionKind::Load(LocalId(0)));
    let number = entry.constant(1);
    entry.value(
        Type::I32,
        InstructionKind::CallClosureBorrowed {
            callee: callback,
            kind: CallableKind::FnMut,
            arguments: vec![number],
        },
    );
    let handler = entry.value(
        Type::Closure(ClosureId(1)),
        InstructionKind::TakeLocal { local: LocalId(1) },
    );
    let result = entry.value(
        Type::I32,
        InstructionKind::CallClosureOnce {
            callee: handler,
            arguments: vec![],
        },
    );
    let mut roles = module.functions[0]
        .callable_contract
        .as_ref()
        .unwrap()
        .roles
        .clone();
    let mut h = module.functions[5]
        .callable_contract
        .as_ref()
        .unwrap()
        .roles[0]
        .clone();
    h.parameter = 1;
    roles.push(h);
    entry.0.callable_contract = Some(CallableParameterContract {
        roles,
        traces: CallableTraces::Finite(vec![CallableTrace {
            calls: vec![
                CallEvent {
                    parameter: 0,
                    kind: CallableKind::FnMut,
                },
                CallEvent {
                    parameter: 1,
                    kind: CallableKind::FnOnce,
                },
            ],
            exit: CallableExit::Return,
        }]),
    });
    module.functions[5] = entry.finish(result);
    result_wire(&module, true);
    let CallableTraces::Finite(paths) = &mut module.functions[0]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
    else {
        unreachable!()
    };
    paths[0].calls.pop();
    result_wire(&module, false);
}

#[test]
fn concrete_local_once_formal_keeps_declared_consumption_raw_wire() {
    let mut module = local_composition(false, false);
    let f = &mut module.functions[0];
    f.locals[0].ty = Type::Closure(ClosureId(0));
    f.values[0] = f.locals[0].ty.clone();
    f.blocks[0].instructions[0].kind = InstructionKind::TakeLocal { local: LocalId(0) };
    // Locate by producer, rather than relying on the local construction prefix.
    let call = f.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::CallClosureBorrowed { .. }))
        .unwrap();
    let InstructionKind::CallClosureBorrowed {
        callee, arguments, ..
    } = &call.kind
    else {
        unreachable!()
    };
    call.kind = InstructionKind::CallClosureOnce {
        callee: *callee,
        arguments: arguments.clone(),
    };
    let role = &mut f.callable_contract.as_mut().unwrap().roles[0];
    role.kind = CallableKind::FnOnce;
    role.cleanup = CallableCleanup::Owned;
    let CallableTraces::Finite(paths) = &mut f.callable_contract.as_mut().unwrap().traces else {
        unreachable!()
    };
    paths[0].calls[0].kind = CallableKind::FnOnce;
    result_wire(&module, true);
    let f = &mut module.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                InstructionKind::CallClosureOnce {
                    callee: ValueId(0),
                    ..
                }
            )
        })
        .unwrap();
    let mut twice = f.blocks[0].instructions[at].clone();
    twice.result = Some(ValueId(f.values.len() as u32));
    f.values.push(Type::I32);
    f.blocks[0].instructions.insert(at + 1, twice);
    result_wire(&module, false);
}

#[test]
fn concrete_local_unknown_and_cfg_mixed_capture_sources_fail_raw_wire() {
    let mut unknown = local_composition(false, true);
    let f = &mut unknown.functions[0];
    let made = f.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| {
            matches!(
                i.kind,
                InstructionKind::MakeClosure {
                    closure: ClosureId(1),
                    ..
                }
            )
        })
        .unwrap();
    made.kind = InstructionKind::ReadCapture { index: 0 };
    let error = analyze(&unknown, &Limits::default()).unwrap_err();
    assert_eq!(
        error[0].message,
        "local environment capture has a symbolic, mixed or unknown callable source"
    );
    result_wire(&unknown, false);

    let mut mixed = local_composition(false, true);
    let f = &mut mixed.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::TakeLocal { local: LocalId(1) }))
        .unwrap();
    let tail = f.blocks[0].instructions.split_off(at);
    let original_exit = f.blocks[0].terminator.clone();
    let condition = ValueId(f.values.len() as u32);
    let unknown = ValueId(condition.0 + 1);
    f.values.extend([Type::Bool, Type::Closure(ClosureId(1))]);
    f.blocks[0].instructions.push(inst(
        Some(condition),
        InstructionKind::Constant(Constant::Bool(true)),
    ));
    f.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    f.blocks.extend([
        BasicBlock {
            id: BlockId(1),
            instructions: vec![],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![
                inst(Some(unknown), InstructionKind::ReadCapture { index: 0 }),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(1),
                        value: unknown,
                    },
                ),
            ],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: tail,
            terminator: original_exit,
        },
    ]);
    let error = analyze(&mixed, &Limits::default()).unwrap_err();
    assert_eq!(
        error[0].message,
        "local environment capture has a symbolic, mixed or unknown callable source"
    );
    result_wire(&mixed, false);
}

// Actual initial/copied forward Slot cells and Origin snapshot/helper-table
// cells, measured independently for the unchanged fixtures. See the scoped
// accounting for joint regressions. Prior reservations were
// one unit per cell; the dynamic actual-role representation pays its bytes.
fn callable_storage_delta(state_cells: usize, origin_cells: usize) -> usize {
    type Origin = crate::helpers::Origin;
    state_cells * (std::mem::size_of::<(Origin, u8, bool)>() - 1)
        + origin_cells * (std::mem::size_of::<Origin>() - 1)
}

#[test]
fn concrete_local_legacy_cost_controls() {
    for (name, module, old_cost, state_cells, origin_cells) in [
        ("ordinary", fixture(CallableKind::Fn, false, 1), 1548, 0, 0),
        (
            "helper-no-local",
            scoped_helper(CallableKind::Fn),
            1219,
            12,
            6,
        ),
        (
            "scoped-local-borrow",
            scoped_helper_with_local_body(CallableKind::FnOnce, true),
            // Includes 279's additional actual captured-address window checks.
            2798,
            20,
            20,
        ),
    ] {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        analyze_budgeted(&module, &limits, &mut budget, None).unwrap();
        let exact = limits.max_analysis_steps - budget.remaining();
        let delta = callable_storage_delta(state_cells, origin_cells);
        assert_eq!(exact, old_cost + delta, "{name}");
        if delta != 0 {
            assert_eq!(
                analyze(
                    &module,
                    &Limits {
                        max_analysis_steps: old_cost,
                        ..limits.clone()
                    }
                )
                .unwrap_err()[0]
                    .category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
        for steps in [exact, exact - 1, exact] {
            let actual = analyze(
                &module,
                &Limits {
                    max_analysis_steps: steps,
                    ..limits.clone()
                },
            );
            if steps == exact {
                actual.unwrap();
            } else {
                assert_eq!(
                    actual.unwrap_err()[0].category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
        println!("282 legacy {name} N={exact}");
    }
}

fn same_closure_distinct_roots() -> TypedModule {
    let mut module = scoped_fixture(0);
    module.closures[1].kind = CallableKind::Fn;
    let mut f = Body::new(0);
    f.0.locals.push(LocalDecl {
        id: LocalId(0),
        name: "formal".into(),
        ty: Type::Closure(ClosureId(1)),
        mutable: false,
        span: s(),
    });
    f.0.parameters.push(LocalId(0));
    let formal = f.value(
        Type::Closure(ClosureId(1)),
        InstructionKind::TakeLocal { local: LocalId(0) },
    );
    f.value(
        Type::I32,
        InstructionKind::CallClosureOnce {
            callee: formal,
            arguments: vec![],
        },
    );
    let concrete = f.value(
        Type::Closure(ClosureId(1)),
        InstructionKind::MakeClosure {
            closure: ClosureId(1),
            captures: vec![],
        },
    );
    let result = f.value(
        Type::I32,
        InstructionKind::CallClosureOnce {
            callee: concrete,
            arguments: vec![],
        },
    );
    f.0.callable_contract = Some(CallableParameterContract {
        roles: vec![CallableRole {
            life: None,
            parameter: 0,
            closure: ClosureId(1),
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
    module.functions[0] = f.finish(result);
    module
}

#[test]
fn concrete_local_same_closure_id_never_revives_or_merges_actual_roots_raw_wire() {
    let module = same_closure_distinct_roots();
    result_wire(&module, true);
    let mut reused = module.clone();
    reused.functions[0].blocks[0].instructions[3].kind = InstructionKind::CallClosureOnce {
        callee: ValueId(0),
        arguments: vec![],
    };
    result_wire(&reused, false);

    // Both branches store the exact same ClosureId. Only one alternative is
    // the incoming symbolic F; the other is this construction's actual value.
    let mut mixed = module;
    let f = &mut mixed.functions[0];
    f.locals.push(LocalDecl {
        id: LocalId(1),
        name: "selected".into(),
        ty: Type::Closure(ClosureId(1)),
        mutable: true,
        span: s(),
    });
    f.values.push(Type::Bool);
    let mut prefix = f.blocks[0].instructions.clone();
    prefix.remove(1); // Do not consume the formal before selecting a branch.
    prefix.truncate(2); // Take formal, then construct concrete.
    prefix.push(inst(None, InstructionKind::StorageLive(LocalId(1))));
    prefix.push(inst(
        Some(ValueId(4)),
        InstructionKind::Constant(Constant::Bool(true)),
    ));
    f.values.push(Type::Closure(ClosureId(1)));
    f.blocks = vec![
        BasicBlock {
            id: BlockId(0),
            instructions: prefix,
            terminator: Terminator::Branch {
                condition: ValueId(4),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        BasicBlock {
            id: BlockId(1),
            instructions: vec![inst(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(0),
                },
            )],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(2),
            instructions: vec![inst(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: ValueId(2),
                },
            )],
            terminator: Terminator::Jump(BlockId(3)),
        },
        BasicBlock {
            id: BlockId(3),
            instructions: vec![
                inst(
                    Some(ValueId(5)),
                    InstructionKind::TakeLocal { local: LocalId(1) },
                ),
                inst(
                    Some(ValueId(3)),
                    InstructionKind::CallClosureOnce {
                        callee: ValueId(5),
                        arguments: vec![],
                    },
                ),
                inst(None, InstructionKind::StorageDead(LocalId(1))),
            ],
            terminator: Terminator::Return(Some(ValueId(3))),
        },
    ]
    .into();
    let error = analyze(&mixed, &Limits::default()).unwrap_err();
    assert_eq!(
        error[0].message,
        "callable instance source is missing or mixed at this CFG position"
    );
    result_wire(&mixed, false);
}

// 282 follow-up: no With and no MakeClosure in the symbolic helper itself.
fn factory_only_composition() -> TypedModule {
    let mut module = local_composition(true, false);
    let f = &mut module.functions[0];
    let construction = f.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| {
            matches!(
                i.kind,
                InstructionKind::MakeClosure {
                    closure: ClosureId(1),
                    ..
                }
            )
        })
        .unwrap();
    construction.kind = InstructionKind::Call {
        function: FunctionId(6),
        arguments: vec![],
    };
    let mut factory = Body::new(6);
    factory.0.return_type = Type::Closure(ClosureId(1));
    let result = factory.value(
        Type::Closure(ClosureId(1)),
        InstructionKind::MakeClosure {
            closure: ClosureId(1),
            captures: vec![],
        },
    );
    factory.0.callable_return_contract = Some(CallableReturnContract {
        source: CallableReturnSource::Fresh,
        closure: ClosureId(1),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
        lending_captures: vec![],
    });
    let mut factory = factory.finish(result);
    factory.return_type = Type::Closure(ClosureId(1));
    module.functions.push(factory);
    module
}

#[test]
fn concrete_factory_only_actual_fresh_contract_is_required_raw_wire() {
    let module = factory_only_composition();
    assert!(
        !module.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                InstructionKind::MakeClosure { .. } | InstructionKind::WithForeignCallback { .. }
            ))
    );
    result_wire(&module, true);
    let mut unknown = module.clone();
    unknown.functions[6].callable_return_contract = None;
    result_wire(&unknown, false);
    let mut parameter = module.clone();
    parameter.functions[6]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .source = CallableReturnSource::Parameter(0);
    result_wire(&parameter, false);
    let mut wrong_result = module.clone();
    let result = wrong_result.functions[0].blocks[0]
        .instructions
        .iter()
        .find_map(|i| {
            matches!(
                i.kind,
                InstructionKind::Call {
                    function: FunctionId(6),
                    ..
                }
            )
            .then_some(i.result)
            .flatten()
        })
        .unwrap();
    wrong_result.functions[0].values[result.0 as usize] = Type::Closure(ClosureId(0));
    result_wire(&wrong_result, false);
    let mut wrong_claim = module.clone();
    wrong_claim.functions[6]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .closure = ClosureId(0);
    result_wire(&wrong_claim, false);
    let mut reused = module;
    let f = &mut reused.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                InstructionKind::Call {
                    function: FunctionId(5),
                    ..
                }
            )
        })
        .unwrap();
    let mut second = f.blocks[0].instructions[at].clone();
    second.result = Some(ValueId(f.values.len() as u32));
    f.values.push(Type::I32);
    f.blocks[0].instructions.insert(at + 1, second);
    result_wire(&reused, false);
}

#[test]
fn concrete_factory_only_shared_allowance_n_minus_one_fresh_raw_wire() {
    let module = factory_only_composition();
    let defaults = Limits::default();
    let mut budget = Budget::new(&defaults);
    let expected = analyze_budgeted(&module, &defaults, &mut budget, None).unwrap();
    let exact = defaults.max_analysis_steps - budget.remaining();
    assert_eq!(exact, 2073 + callable_storage_delta(18, 30));
    assert_eq!(
        analyze(
            &module,
            &Limits {
                max_analysis_steps: 2073,
                ..defaults.clone()
            }
        )
        .unwrap_err()[0]
            .category,
        zil_base::ResultCategory::ResourceLimit
    );
    let bytes = zil_ir::wire::encode_module_candidate(
        &module,
        &CandidatePlan::empty_for(&module),
        &defaults,
    )
    .unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &defaults)
        .unwrap()
        .0;
    for input in [&module, &decoded] {
        for steps in [exact, exact - 1, exact] {
            let actual = analyze(
                input,
                &Limits {
                    max_analysis_steps: steps,
                    ..defaults.clone()
                },
            );
            if steps == exact {
                assert_eq!(actual.unwrap(), expected);
            } else {
                assert_eq!(
                    actual.unwrap_err()[0].category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
    }
    println!("282 factory-only raw=Wire N={exact}; N-1 ResourceLimit; fresh equality");
}

fn parameter_forwarded_local() -> TypedModule {
    let mut module = same_closure_distinct_roots();
    let mut factory = factory_only_composition().functions[6].clone();
    factory.id = FunctionId(5);
    factory.callable_return_contract.as_mut().unwrap().kind = CallableKind::Fn;
    module.functions.push(factory);
    let mut keep = Body::new(6);
    keep.0.locals.push(module.functions[0].locals[0].clone());
    keep.0.parameters.push(LocalId(0));
    keep.0.return_type = Type::Closure(ClosureId(1));
    keep.0.callable_return_contract = Some(CallableReturnContract {
        source: CallableReturnSource::Parameter(0),
        closure: ClosureId(1),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
        lending_captures: vec![],
    });
    keep.0.callable_contract = Some(CallableParameterContract {
        roles: vec![CallableRole {
            life: None,
            parameter: 0,
            closure: ClosureId(1),
            kind: CallableKind::FnOnce,
            hold: vec![],
            call: vec![],
            writes: vec![],
            cleanup: CallableCleanup::Returned,
        }],
        traces: CallableTraces::Finite(vec![CallableTrace {
            calls: vec![],
            exit: CallableExit::Return,
        }]),
    });
    let returned = keep.value(
        Type::Closure(ClosureId(1)),
        InstructionKind::TakeLocal { local: LocalId(0) },
    );
    let mut keep = keep.finish(returned);
    keep.return_type = Type::Closure(ClosureId(1));
    module.functions.push(keep);
    let f = &mut module.functions[0];
    f.blocks[0].instructions[2].kind = InstructionKind::Call {
        function: FunctionId(5),
        arguments: vec![],
    };
    f.values.push(Type::Closure(ClosureId(1)));
    f.blocks[0].instructions.insert(
        3,
        inst(
            Some(ValueId(4)),
            InstructionKind::Call {
                function: FunctionId(6),
                arguments: vec![ValueId(2)],
            },
        ),
    );
    f.blocks[0].instructions[4].kind = InstructionKind::CallClosureOnce {
        callee: ValueId(4),
        arguments: vec![],
    };
    module
}

#[test]
fn concrete_factory_only_valid_parameter_result_keeps_actual_source_raw_wire() {
    let module = parameter_forwarded_local();
    result_wire(&module, true);
    let mut symbolic = module.clone();
    symbolic.functions[0].blocks[0].instructions.remove(1);
    let InstructionKind::Call { arguments, .. } =
        &mut symbolic.functions[0].blocks[0].instructions[2].kind
    else {
        unreachable!()
    };
    arguments[0] = ValueId(0);
    // A valid Parameter result of the actual F is still F, even though an
    // independent public Fresh result has the identical ClosureId.
    result_wire(&symbolic, true);
    let mut erased_event = symbolic;
    let CallableTraces::Finite(paths) = &mut erased_event.functions[0]
        .callable_contract
        .as_mut()
        .unwrap()
        .traces
    else {
        unreachable!()
    };
    paths[0].calls.clear();
    let error = analyze(&erased_event, &Limits::default()).unwrap_err();
    println!("282 valid Parameter erased F event: {:?}", error[0]);
    assert_eq!(error[0].phase, Phase::Analyze);
    assert_eq!(error[0].code, "E-ANALYZE-IR");
    assert_eq!(
        error[0].message,
        "public helper trace claim differs from its CFG"
    );
    result_wire(&erased_event, false);

    let mut consumed = module;
    let InstructionKind::Call { arguments, .. } =
        &mut consumed.functions[0].blocks[0].instructions[3].kind
    else {
        unreachable!()
    };
    arguments[0] = ValueId(0);
    let error = analyze(&consumed, &Limits::default()).unwrap_err();
    println!("282 valid Parameter consumed F argument: {:?}", error[0]);
    assert_eq!(error[0].phase, Phase::Analyze);
    assert_eq!(error[0].code, "E-ANALYZE-IR");
    assert_eq!(
        error[0].message,
        "callable instance is moved, absent or cleaned on a reachable path"
    );
    result_wire(&consumed, false);
}

fn supplied_body_hidden_in_with(twice: bool) -> TypedModule {
    let mut module = scoped_helper(CallableKind::Fn);
    let mut entry = Body::new(5);
    entry.0.environment = Some(ClosureId(2));
    let hidden = entry.value(
        Type::Closure(ClosureId(1)),
        InstructionKind::TakeCapture { index: 0 },
    );
    let mut result = entry.value(
        Type::I32,
        InstructionKind::CallClosure {
            callee: hidden,
            arguments: vec![],
        },
    );
    if twice {
        result = entry.value(
            Type::I32,
            InstructionKind::CallClosure {
                callee: hidden,
                arguments: vec![],
            },
        );
    }
    module.functions.push(entry.finish(result));
    module.closures.push(ClosureDefinition {
        storage_life: None,
        id: ClosureId(2),
        function: FunctionId(5),
        kind: CallableKind::FnOnce,
        captures: vec![Type::Closure(ClosureId(1))],
        parameters: vec![],
        result: Type::I32,
        lending_captures: vec![],
    });
    let f = &mut module.functions[0];
    let made = ValueId(f.values.len() as u32);
    f.values.push(Type::Closure(ClosureId(2)));
    f.blocks[0].instructions.insert(
        2,
        inst(
            Some(made),
            InstructionKind::MakeClosure {
                closure: ClosureId(2),
                captures: vec![ValueId(1)],
            },
        ),
    );
    for instruction in &mut f.blocks[0].instructions {
        if let InstructionKind::WithForeignCallback { body, .. } = &mut instruction.kind {
            *body = made;
        }
    }
    let CallableTraces::Flow(flow) = &mut f.callable_contract.as_mut().unwrap().traces else {
        unreachable!()
    };
    flow.nodes[2].calls.clear();
    module
}

#[test]
fn concrete_local_with_body_cannot_hide_actual_supplied_role_raw_wire() {
    result_wire(&scoped_helper(CallableKind::Fn), true);
    for twice in [false, true] {
        let module = supplied_body_hidden_in_with(twice);
        let error = analyze(&module, &Limits::default()).unwrap_err();
        assert_eq!(error[0].phase, Phase::Analyze);
        assert_eq!(error[0].code, "E-ANALYZE-IR");
        assert_eq!(
            error[0].message,
            "forwarded symbolic F cannot be hidden in a new local environment"
        );
        result_wire(&module, false);
    }
}

fn wrapped_factory_body(supplied: bool) -> TypedModule {
    let mut module = supplied_body_hidden_in_with(true);
    let package = Type::Package {
        name: "Hidden".into(),
        fields: vec![
            ("body".into(), Type::Closure(ClosureId(1))),
            ("owner".into(), managed()),
        ],
    };
    module.resources.push(ResourceDefinition {
        name: "Token".into(),
        payload: payload(),
        teardown: None,
    });
    let mut factory = Body::new(6);
    factory.0.locals.push(LocalDecl {
        id: LocalId(0),
        name: "input".into(),
        ty: package.clone(),
        mutable: true,
        span: s(),
    });
    factory.0.parameters.push(LocalId(0));
    factory.0.return_type = Type::Closure(ClosureId(2));
    factory.0.callable_return_contract = Some(CallableReturnContract {
        source: CallableReturnSource::Fresh,
        closure: ClosureId(2),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![0],
        lending_captures: vec![],
    });
    factory.op(InstructionKind::ScopeEnter { scope: ScopeId(0) });
    let body = factory.value(
        Type::Closure(ClosureId(1)),
        InstructionKind::TakePackageField {
            local: LocalId(0),
            index: 0,
        },
    );
    let result = factory.value(
        Type::Closure(ClosureId(2)),
        InstructionKind::MakeClosure {
            closure: ClosureId(2),
            captures: vec![body],
        },
    );
    factory.op(InstructionKind::DropPackage { local: LocalId(0) });
    factory.op(InstructionKind::StorageDead(LocalId(0)));
    factory.op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    module.functions.push(factory.finish(result));
    let f = &mut module.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| {
            matches!(
                i.kind,
                InstructionKind::MakeClosure {
                    closure: ClosureId(2),
                    ..
                }
            )
        })
        .unwrap();
    let mut preparation = Vec::new();
    let body = if supplied {
        ValueId(1)
    } else {
        let body = ValueId(f.values.len() as u32);
        f.values.push(Type::Closure(ClosureId(1)));
        preparation.push(inst(
            Some(body),
            InstructionKind::MakeClosure {
                closure: ClosureId(1),
                captures: vec![],
            },
        ));
        body
    };
    let number = ValueId(f.values.len() as u32);
    let owner = ValueId(number.0 + 1);
    let wrapped = ValueId(number.0 + 2);
    f.values.extend([Type::I32, managed(), package]);
    preparation.extend([
        inst(Some(number), InstructionKind::Constant(Constant::I32(1))),
        inst(
            Some(owner),
            InstructionKind::MakeManaged {
                fields: vec![number],
            },
        ),
        inst(
            Some(wrapped),
            InstructionKind::MakePackage {
                fields: vec![body, owner],
            },
        ),
    ]);
    f.blocks[0].instructions[at].kind = InstructionKind::Call {
        function: FunctionId(6),
        arguments: vec![wrapped],
    };
    f.blocks[0].instructions.splice(at..at, preparation);
    f.blocks[0].instructions.insert(
        0,
        inst(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
    );
    f.blocks[0].instructions.extend([
        inst(None, InstructionKind::StorageDead(LocalId(1))),
        inst(None, InstructionKind::StorageDead(LocalId(0))),
        inst(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ]);
    module
}

#[test]
fn concrete_factory_only_wrapped_factory_never_launders_actual_role_raw_wire() {
    for scoped in [true, false] {
        for supplied in [false, true] {
            let mut module = wrapped_factory_body(supplied);
            if !scoped {
                let f = &mut module.functions[0];
                for instruction in &mut f.blocks[0].instructions {
                    if let InstructionKind::WithForeignCallback { body, .. } = instruction.kind {
                        f.values[instruction.result.unwrap().0 as usize] = Type::I32;
                        instruction.kind = InstructionKind::CallClosureOnce {
                            callee: body,
                            arguments: vec![],
                        };
                    }
                }
                f.blocks[0].instructions.retain(|i| {
                    !matches!(
                        i.kind,
                        InstructionKind::UnsafeEnter { .. } | InstructionKind::UnsafeExit { .. }
                    )
                });
                f.callable_contract.as_mut().unwrap().traces =
                    CallableTraces::Finite(vec![CallableTrace {
                        calls: vec![],
                        exit: CallableExit::Return,
                    }]);
            }
            if supplied {
                let error = analyze(&module, &Limits::default()).unwrap_err();
                assert_eq!(error[0].phase, Phase::Analyze);
                assert_eq!(error[0].code, "E-ANALYZE-IR");
                assert_eq!(
                    error[0].message,
                    "symbolic F enters an unsupported value operation"
                );
            }
            result_wire(&module, !supplied);
        }
    }
}

fn copy_fn_observation(alias: bool) -> TypedModule {
    let mut module = same_closure_distinct_roots();
    let f = &mut module.functions[0];
    f.blocks[0].instructions[0].kind = InstructionKind::Load(LocalId(0));
    f.blocks[0].instructions[1].kind = InstructionKind::CallClosure {
        callee: ValueId(0),
        arguments: vec![],
    };
    let claim = f.callable_contract.as_mut().unwrap();
    claim.roles[0].kind = CallableKind::Fn;
    let CallableTraces::Finite(paths) = &mut claim.traces else {
        unreachable!()
    };
    paths[0].calls[0].kind = CallableKind::Fn;
    if alias {
        f.locals.push(LocalDecl {
            id: LocalId(1),
            name: "observation".into(),
            ty: Type::Closure(ClosureId(1)),
            mutable: false,
            span: s(),
        });
        f.values.push(Type::Closure(ClosureId(1)));
        f.blocks[0].instructions.splice(
            1..1,
            [
                inst(None, InstructionKind::StorageLive(LocalId(1))),
                inst(
                    None,
                    InstructionKind::Store {
                        local: LocalId(1),
                        value: ValueId(0),
                    },
                ),
                inst(Some(ValueId(4)), InstructionKind::Load(LocalId(1))),
            ],
        );
        f.blocks[0].instructions[4].kind = InstructionKind::CallClosure {
            callee: ValueId(4),
            arguments: vec![],
        };
        f.blocks[0]
            .instructions
            .push(inst(None, InstructionKind::StorageDead(LocalId(1))));
    }
    module
}

#[test]
fn concrete_local_copy_fn_observations_never_become_transferable_roles_raw_wire() {
    for alias in [false, true] {
        let module = copy_fn_observation(alias);
        result_wire(&module, true);
        let mut once = module;
        once.functions[0].callable_contract.as_mut().unwrap().roles[0].kind = CallableKind::FnOnce;
        result_wire(&once, false);
    }
    for (mut hidden, message) in [
        (
            wrapped_factory_body(true),
            "symbolic F enters an unsupported value operation",
        ),
        (
            supplied_body_hidden_in_with(true),
            "a symbolic F observation cannot be copied into a transferred value",
        ),
    ] {
        let f = &mut hidden.functions[0];
        f.callable_contract.as_mut().unwrap().roles[1].kind = CallableKind::Fn;
        f.blocks[0]
            .instructions
            .iter_mut()
            .find(|i| matches!(i.kind, InstructionKind::TakeLocal { local: LocalId(1) }))
            .unwrap()
            .kind = InstructionKind::Load(LocalId(1));
        let error = analyze(&hidden, &Limits::default()).unwrap_err();
        assert_eq!(error[0].message, message);
        result_wire(&hidden, false);
    }
    let mut transferred = parameter_forwarded_local();
    let f = &mut transferred.functions[0];
    f.blocks[0].instructions[0].kind = InstructionKind::Load(LocalId(0));
    f.blocks[0].instructions.remove(1);
    f.callable_contract.as_mut().unwrap().roles[0].kind = CallableKind::Fn;
    let CallableTraces::Finite(paths) = &mut f.callable_contract.as_mut().unwrap().traces else {
        unreachable!()
    };
    paths[0].calls.clear();
    let InstructionKind::Call { arguments, .. } = &mut f.blocks[0].instructions[2].kind else {
        unreachable!()
    };
    arguments[0] = ValueId(0);
    let error = analyze(&transferred, &Limits::default()).unwrap_err();
    assert_eq!(
        error[0].message,
        "a symbolic F observation cannot be copied into a transferred value"
    );
    result_wire(&transferred, false);
}

#[test]
fn concrete_local_copy_fn_observation_budget_raw_wire() {
    for alias in [false, true] {
        let module = copy_fn_observation(alias);
        let defaults = Limits::default();
        let mut budget = Budget::new(&defaults);
        let expected = analyze_budgeted(&module, &defaults, &mut budget, None).unwrap();
        let exact = defaults.max_analysis_steps - budget.remaining();
        let (old_cost, state_cells, origin_cells) =
            if alias { (855, 14, 21) } else { (726, 10, 15) };
        assert_eq!(
            exact,
            old_cost + callable_storage_delta(state_cells, origin_cells)
        );
        assert_eq!(
            analyze(
                &module,
                &Limits {
                    max_analysis_steps: old_cost,
                    ..defaults.clone()
                }
            )
            .unwrap_err()[0]
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
        let bytes = zil_ir::wire::encode_module_candidate(&module, &expected, &defaults).unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&bytes, &defaults)
            .unwrap()
            .0;
        for input in [&module, &decoded] {
            for steps in [exact, exact - 1, exact] {
                let actual = analyze(
                    input,
                    &Limits {
                        max_analysis_steps: steps,
                        ..defaults.clone()
                    },
                );
                if steps == exact {
                    assert_eq!(actual.unwrap(), expected);
                } else {
                    assert_eq!(
                        actual.unwrap_err()[0].category,
                        zil_base::ResultCategory::ResourceLimit
                    );
                }
            }
        }
        println!(
            "282 Copy Fn observation alias={alias}: raw=Wire N={exact}; N-1 ResourceLimit; fresh equality"
        );
    }
}
