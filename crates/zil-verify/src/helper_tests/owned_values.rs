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
            schema_version: SCHEMA_VERSION,
            module_id: ModuleId(0),
            source_fingerprint: Fingerprint::of(b"owned callback raw and wire"),
            target: TargetProfile::host_llvm(),
            functions: vec![],
            closures: vec![ClosureDefinition {
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
    raw.module.closures[0]
        .parameters
        .push(address(Type::I32, false));
    raw.reject("non-Copy type");
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
        name: "unused_array".into(),
        ty: Type::Array {
            element: Box::new(env()),
            length: 1,
        },
        mutable: true,
        span: span(),
    });
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
        source: zil_ir::CallableReturnSource::Fresh,
        closure: ClosureId(0),
        kind: CallableKind::FnOnce,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![],
        lending_captures: vec![],
    });
    raw.reject("container and opaque callback profiles do not admit owning data signatures");
}
