use super::*;

fn event() -> Type {
    Type::Package {
        name: "Event".into(),
        fields: vec![("token".into(), owned())],
    }
}

fn reply() -> Type {
    Type::Package {
        name: "Reply".into(),
        fields: vec![("token".into(), owned())],
    }
}

fn finish_scoped(mut body: Body, result: ValueId) -> Body {
    body.f.blocks[0].instructions.insert(
        0,
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
    );
    for index in (0..body.f.locals.len()).rev() {
        body.operation(InstructionKind::StorageDead(LocalId(index as u32)));
    }
    body.operation(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    body.finish(result)
}

fn dispatch() -> Raw {
    let mut helper = Body::new(0);
    helper.f.return_type = reply();
    let callback = helper.local(env(), true);
    let data = helper.local(event(), true);
    let f = helper.value(
        env(),
        InstructionKind::TakeLocal { local: callback },
        vec![],
    );
    let argument = helper.value(event(), InstructionKind::TakeLocal { local: data }, vec![]);
    let output = helper.value(
        reply(),
        InstructionKind::CallClosureOnce {
            callee: f,
            arguments: vec![argument],
        },
        vec![],
    );
    helper.operation(InstructionKind::DropPackage { local: data });
    helper.operation(InstructionKind::DropClosure { local: callback });
    helper.f.callable_contract = Some(CallableParameterContract {
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
    let helper = finish_scoped(helper, output);

    let mut entry = Body::new(1);
    entry.f.environment = Some(ClosureId(0));
    entry.f.return_type = reply();
    let input = entry.local(event(), true);
    let token = entry.value(
        owned(),
        InstructionKind::TakePackageField {
            local: input,
            index: 0,
        },
        vec![],
    );
    let response = entry.value(
        reply(),
        InstructionKind::MakePackage {
            fields: vec![token],
        },
        vec![],
    );
    entry.operation(InstructionKind::DropPackage { local: input });
    let entry = finish_scoped(entry, response);

    let mut caller = Body::new(2);
    caller.f.return_type = reply();
    let n = caller.number(7);
    let token = caller.value(
        owned(),
        InstructionKind::MakeManaged { fields: vec![n] },
        vec![],
    );
    let data = caller.value(
        event(),
        InstructionKind::MakePackage {
            fields: vec![token],
        },
        vec![],
    );
    let f = caller.value(
        env(),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![],
        },
        vec![],
    );
    let response = caller.value(
        reply(),
        InstructionKind::Call {
            function: FunctionId(0),
            arguments: vec![f, data],
        },
        vec![],
    );
    let caller = finish_scoped(caller, response);

    let mut raw = Raw {
        module: TypedModule {
            dyn_declarations: Vec::new(),
            dyn_interfaces: Vec::new(),
            dyn_vtables: Vec::new(),
            life_declarations: Vec::new(),
            life_instances: Vec::new(),
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::of(b"owned callback raw and wire"),
            target: TargetProfile::host_llvm(),
            functions: vec![],
            closures: vec![ClosureDefinition {
                storage_life: None,
                id: ClosureId(0),
                function: FunctionId(1),
                captures: vec![],
                parameters: vec![event()],
                result: reply(),
                kind: CallableKind::FnOnce,
                lending_captures: vec![],
            }],
            resources: vec![ResourceDefinition {
                name: "Token".into(),
                payload: payload(),
                teardown: None,
            }],
            unsupported: vec![],
        },
        holds: BTreeMap::new(),
    };
    raw.push(helper);
    raw.push(entry);
    raw.push(caller);
    raw
}

#[test]
fn event_to_owned_reply_via_public_once_dispatch_raw_and_wire() {
    dispatch().accept();
}

#[test]
fn owning_dispatch_rejects_forged_role_and_data_signature_raw_and_wire() {
    let mut raw = dispatch();
    raw.module.functions[0]
        .callable_contract
        .as_mut()
        .unwrap()
        .roles[0]
        .kind = CallableKind::Fn;
    raw.reject("one by-value FnOnce role");

    let mut raw = dispatch();
    raw.module.functions[0].locals[1].ty = reply();
    raw.reject("instruction result presence or type is incorrect");

    let mut raw = dispatch();
    // Borrowed inputs can accompany an owned result, but the concrete entry
    // must still match the entire declared signature. This mutation leaves
    // that entry unchanged, so it must fail the independent identity check.
    raw.module.closures[0]
        .parameters
        .push(address(Type::I32, false));
    raw.reject("closure entry signature differs from its definition");
}

#[test]
fn owning_callback_argument_is_consumed_once_raw_and_wire() {
    let mut raw = dispatch();
    let helper = &mut raw.module.functions[0];
    let result = ValueId(helper.values.len() as u32);
    helper.values.push(reply());
    let call = helper.blocks[0]
        .instructions
        .iter()
        .position(|instruction| matches!(instruction.kind, InstructionKind::CallClosureOnce { .. }))
        .unwrap();
    helper.blocks[0].instructions.insert(
        call + 1,
        op(
            Some(result),
            InstructionKind::CallClosureOnce {
                callee: ValueId(0),
                arguments: vec![ValueId(1)],
            },
        ),
    );
    raw.reject("consumes a callback role more than once");
}

#[test]
fn unused_owned_signature_cannot_hide_callback_in_store_element_raw_and_wire() {
    let mut raw = dispatch();
    raw.module.closures[0].result = Type::Package {
        name: "BadReply".into(),
        fields: vec![(
            "callbacks".into(),
            Type::Managed(Box::new(Type::OwnedStore {
                element: Box::new(Type::Package {
                    name: "CallbackItem".into(),
                    fields: vec![("callback".into(), env())],
                }),
            })),
        )],
    };
    raw.reject("cannot contain a concrete or opaque callback");
}

fn stored_event() -> Type {
    Type::Managed(Box::new(Type::OwnedStore {
        element: Box::new(event()),
    }))
}

#[test]
fn owning_result_does_not_expand_shared_product_or_store_capture_profiles() {
    let mut raw = dispatch();
    raw.module.closures[0].captures = vec![address(event(), false)];
    raw.reject("shared Product captures require a finite Copy result");

    let mut raw = dispatch();
    raw.module.closures[0].captures = vec![stored_event()];
    raw.reject("captured Store requires a finite Copy or Unit result");

    let mut raw = dispatch();
    raw.module.closures[0].captures = vec![stored_event()];
    raw.module.closures[0].result = Type::I32;
    raw.module.closures.push(ClosureDefinition {
        storage_life: None,
        id: ClosureId(1),
        function: FunctionId(2),
        captures: vec![env()],
        parameters: vec![],
        result: reply(),
        kind: CallableKind::FnOnce,
        lending_captures: vec![],
    });
    raw.reject("captured Store requires a finite Copy or Unit result");
}

#[test]
fn callback_result_cannot_export_callee_created_resource_as_a_create_result() {
    let mut raw = dispatch();
    let caller = &mut raw.module.functions[2];
    caller.return_type = owned();
    caller.result_mode = PassingMode::Create;
    let local = LocalId(0);
    caller.locals.push(LocalDecl {
        id: local,
        name: "response".into(),
        ty: reply(),
        mutable: true,
        span: span(),
    });
    let token = ValueId(caller.values.len() as u32);
    caller.values.push(owned());
    let instructions = &mut caller.blocks[0].instructions;
    let exit = instructions
        .iter()
        .position(|instruction| matches!(instruction.kind, InstructionKind::ScopeExit { .. }))
        .unwrap();
    instructions.splice(
        exit..exit,
        [
            op(None, InstructionKind::StorageLive(local)),
            op(
                None,
                InstructionKind::Store {
                    local,
                    value: ValueId(4),
                },
            ),
            op(
                Some(token),
                InstructionKind::TakePackageField { local, index: 0 },
            ),
            op(None, InstructionKind::DropPackage { local }),
            op(None, InstructionKind::StorageDead(local)),
        ],
    );
    caller.blocks[0].terminator = Terminator::Return(Some(token));
    raw.reject("create result is not independently proven fresh");
}

fn callback_state() -> Type {
    Type::Package {
        name: "State".into(),
        fields: vec![("token".into(), owned()), ("callback".into(), env())],
    }
}

#[test]
fn owned_callback_signature_does_not_extend_state_array_store_or_opaque_factory() {
    let state = callback_state();
    let mut raw = dispatch();
    raw.module.functions[2].locals.push(LocalDecl {
        id: LocalId(0),
        name: "unused_state".into(),
        ty: state.clone(),
        mutable: true,
        span: span(),
    });
    raw.reject("container and opaque callback profiles do not admit owning data signatures");

    let mut raw = dispatch();
    raw.module.functions[2].locals.push(LocalDecl {
        id: LocalId(0),
        name: "public_array".into(),
        ty: Type::Array {
            element: Box::new(env()),
            length: 1,
        },
        mutable: false,
        span: span(),
    });
    raw.module.functions[2].parameters.push(LocalId(0));
    raw.reject("container and opaque callback profiles do not admit owning data signatures");

    let mut raw = dispatch();
    raw.module.functions[2].return_type = Type::Array {
        element: Box::new(env()),
        length: 1,
    };
    raw.reject("container and opaque callback profiles do not admit owning data signatures");

    let mut raw = dispatch();
    raw.module.functions[2].locals.push(LocalDecl {
        id: LocalId(0),
        name: "unused_store".into(),
        ty: Type::Managed(Box::new(Type::OwnedStore {
            element: Box::new(state),
        })),
        mutable: true,
        span: span(),
    });
    raw.reject("container and opaque callback profiles do not admit owning data signatures");

    let mut raw = dispatch();
    let store_element = callback_state();
    raw.module.resources.push(ResourceDefinition {
        name: dynamic_store::definition_name(&store_element),
        payload: Type::OwnedStore {
            element: Box::new(store_element),
        },
        teardown: None,
    });
    raw.reject("container and opaque callback profiles do not admit owning data signatures");

    let mut raw = dispatch();
    raw.module.functions[2].return_type = env();
    raw.module.functions[2].callable_return_contract = Some(CallableReturnContract {
        capture_sources: Some(Vec::new()),
        source: zil_ir::CallableReturnSource::Fresh,
        closure: ClosureId(0),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
        lending_captures: vec![],
    });
    // Owned callback data signatures are admitted for actual opaque factories.
    // This forged body still returns Reply instead of its claimed Closure.
    raw.reject("E-VERIFY-RETURN");
}

#[test]
fn declared_but_uninitialized_private_slot_is_allowed_raw_wire() {
    let mut raw = dispatch();
    raw.module.functions[2].locals.push(LocalDecl {
        id: LocalId(0),
        name: "unused_array".into(),
        ty: Type::Array {
            element: Box::new(env()),
            length: 1,
        },
        mutable: true,
        span: span(),
    });
    raw.accept();
}

fn slot_through_reused_local(replace: bool) -> Raw {
    let mut raw = dispatch();
    let mut keep = Body::new(0);
    keep.f.return_type = env();
    let f = keep.local(env(), true);
    let value = keep.value(env(), InstructionKind::TakeLocal { local: f }, vec![]);
    keep.f.callable_contract = Some(CallableParameterContract {
        roles: vec![CallableRole {
            life: None,
            parameter: 0,
            closure: ClosureId(0),
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
    keep.f.callable_return_contract = Some(CallableReturnContract {
        capture_sources: Some(Vec::new()),
        source: CallableReturnSource::Parameter(0),
        lending_captures: vec![],
        closure: ClosureId(0),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
    });
    raw.module.functions[0] = finish_scoped(keep, value).f;
    let mut caller = Body::new(2);
    let slot_type = Type::Array {
        element: Box::new(env()),
        length: 1,
    };
    let slot = caller.local(slot_type.clone(), false);
    let bridge = caller.local(env(), false);
    let original = caller.value(
        env(),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![],
        },
        vec![],
    );
    let array = caller.value(
        slot_type,
        InstructionKind::MakeOwnedArray {
            elements: vec![original],
        },
        vec![],
    );
    caller.operation(InstructionKind::Store {
        local: slot,
        value: array,
    });
    let taken = caller.value(
        env(),
        InstructionKind::TakeOwnedArrayElement {
            local: slot,
            index: 0,
        },
        vec![],
    );
    caller.operation(InstructionKind::Store {
        local: bridge,
        value: taken,
    });
    if replace {
        caller.operation(InstructionKind::DropClosure { local: bridge });
        let other = caller.value(
            env(),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![],
            },
            vec![],
        );
        caller.operation(InstructionKind::Store {
            local: bridge,
            value: other,
        });
    }
    let actual = caller.value(env(), InstructionKind::TakeLocal { local: bridge }, vec![]);
    let result = caller.value(
        env(),
        InstructionKind::Call {
            function: FunctionId(0),
            arguments: vec![actual],
        },
        vec![],
    );
    caller.operation(InstructionKind::Store {
        local: bridge,
        value: result,
    });
    caller.operation(InstructionKind::DropClosure { local: bridge });
    caller.operation(InstructionKind::DropOwnedArray { local: slot });
    let zero = caller.number(0);
    raw.module.functions[2] = finish_scoped(caller, zero).f;
    raw
}

#[test]
fn private_slot_can_discharge_a_and_forward_fresh_b_raw_wire() {
    slot_through_reused_local(false).accept();
    slot_through_reused_local(true).accept();
}

#[test]
fn private_slot_responsibility_survives_correlated_branches_raw_wire() {
    let mut raw = slot_through_reused_local(false);
    let f = &mut raw.module.functions[2];
    let split = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::TakeLocal { local: LocalId(1) }))
        .unwrap();
    let mut tail = f.blocks[0].instructions.split_off(split);
    let exit = f.blocks[0].terminator.clone();
    let flag_local = LocalId(f.locals.len() as u32);
    f.locals.push(LocalDecl {
        id: flag_local,
        name: "flag".into(),
        ty: Type::Bool,
        mutable: false,
        span: span(),
    });
    f.parameters.push(flag_local);
    let cleanup = tail
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
        .unwrap();
    tail.insert(cleanup, op(None, InstructionKind::StorageDead(flag_local)));
    let flag = ValueId(f.values.len() as u32);
    f.values.push(Type::Bool);
    f.blocks[0]
        .instructions
        .push(op(Some(flag), InstructionKind::Load(flag_local)));
    f.blocks[0].terminator = Terminator::Branch {
        condition: flag,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    let replacement = |f: &mut TypedFunction| {
        let fresh = ValueId(f.values.len() as u32);
        f.values.push(env());
        vec![
            op(None, InstructionKind::DropClosure { local: LocalId(1) }),
            op(
                Some(fresh),
                InstructionKind::MakeClosure {
                    closure: ClosureId(0),
                    captures: vec![],
                },
            ),
            op(
                None,
                InstructionKind::Store {
                    local: LocalId(1),
                    value: fresh,
                },
            ),
        ]
    };
    let b = replacement(f);
    let c = replacement(f);
    f.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: b,
        terminator: Terminator::Jump(BlockId(3)),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: vec![],
        terminator: Terminator::Jump(BlockId(3)),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(3),
        instructions: vec![],
        terminator: Terminator::Branch {
            condition: flag,
            then_block: BlockId(4),
            else_block: BlockId(5),
        },
    });
    f.blocks.push(BasicBlock {
        id: BlockId(4),
        instructions: vec![],
        terminator: Terminator::Jump(BlockId(6)),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(5),
        instructions: c,
        terminator: Terminator::Jump(BlockId(6)),
    });
    f.blocks.push(BasicBlock {
        id: BlockId(6),
        instructions: tail,
        terminator: exit,
    });
    // flag=true replaces A at block 1; flag=false replaces A at block 5.
    // Both paths discharge A and give the new live B/C to keep. This is
    // valid private-slot responsibility despite no A -> keep connection.
    raw.accept();
}

#[test]
fn closure_body_private_slot_can_discharge_without_a_forwarding_call_raw_wire() {
    let mut raw = slot_through_reused_local(false);
    let body = &mut raw.module.functions[2];
    let stop = body.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::TakeLocal { local: LocalId(1) }))
        .unwrap();
    body.blocks[0].instructions.truncate(stop);
    body.values.truncate(3);
    body.values.push(Type::I32);
    body.blocks[0].instructions.extend([
        op(None, InstructionKind::DropClosure { local: LocalId(1) }),
        op(None, InstructionKind::DropOwnedArray { local: LocalId(0) }),
        op(
            Some(ValueId(3)),
            InstructionKind::Constant(Constant::I32(0)),
        ),
        op(None, InstructionKind::StorageDead(LocalId(1))),
        op(None, InstructionKind::StorageDead(LocalId(0))),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ]);
    body.blocks[0].terminator = Terminator::Return(Some(ValueId(3)));
    body.environment = Some(ClosureId(1));
    raw.module.closures.push(ClosureDefinition {
        storage_life: None,
        id: ClosureId(1),
        function: FunctionId(2),
        captures: vec![],
        parameters: vec![],
        result: Type::I32,
        kind: CallableKind::Fn,
        lending_captures: vec![],
    });
    raw.accept();
}

#[test]
fn private_array_ssa_without_a_local_carrier_is_rejected_raw_wire() {
    let mut raw = dispatch();
    let caller = &mut raw.module.functions[2];
    caller.return_type = Type::I32;
    caller.locals.clear();
    caller.values = vec![
        env(),
        Type::Array {
            element: Box::new(env()),
            length: 1,
        },
        Type::I32,
    ];
    caller.blocks[0].instructions = vec![
        op(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        op(
            Some(ValueId(0)),
            InstructionKind::MakeClosure {
                closure: ClosureId(0),
                captures: vec![],
            },
        ),
        op(
            Some(ValueId(1)),
            InstructionKind::MakeOwnedArray {
                elements: vec![ValueId(0)],
            },
        ),
        op(
            Some(ValueId(2)),
            InstructionKind::Constant(Constant::I32(0)),
        ),
        op(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ];
    caller.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    raw.reject("private callback array must have an actual local carrier");
}

#[test]
fn private_slot_raw_wire_budget_has_exact_fresh_boundary() {
    let raw = slot_through_reused_local(true);
    let plan = raw.plan();
    let bytes =
        zil_ir::wire::encode_module_candidate(&raw.module, &plan, &Limits::default()).unwrap();
    for wire in [false, true] {
        let run = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if wire {
                verify_module_bytes(&bytes, &raw.module.target, &limits)
            } else {
                verify_module(
                    raw.module.clone(),
                    plan.clone(),
                    &raw.module.target,
                    &limits,
                )
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap().fingerprint();
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match run(middle) {
                Ok(verified) => {
                    assert_eq!(verified.fingerprint(), expected);
                    high = middle;
                }
                Err(errors) => {
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.category == zil_base::ResultCategory::ResourceLimit),
                        "{errors:?}"
                    );
                    low = middle;
                }
            }
        }
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|e| e.category == zil_base::ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        println!("private-slot254 wire={wire} N={high}; N-1=ResourceLimit; fresh N=Pass");
    }
}
