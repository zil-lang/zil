//! Independent raw/wire Package capture candidates are implemented here.
//! Fixture construction does not call Types or Analyze.
use super::*;

fn entry(result: Type) -> (Body, LocalId) {
    let mut body = Body::new(result);
    body.0.id = FunctionId(1);
    body.0.name = "package_entry".into();
    body.0.environment = Some(ClosureId(0));
    let view = LocalId(0);
    body.0.locals.push(LocalDecl {
        id: view,
        name: "component".into(),
        ty: package(),
        mutable: true,
        span: span(),
    });
    body.op(InstructionKind::BindPackageCapture {
        local: view,
        index: 0,
    });
    (body, view)
}

fn finish_entry(mut body: Body, value: ValueId, kind: CleanupExitKind) -> TypedFunction {
    for local in (1..body.0.locals.len()).rev() {
        body.op(InstructionKind::StorageDead(LocalId(local as u32)));
    }
    body.op(InstructionKind::EndPackageCapture {
        local: LocalId(0),
        index: 0,
    });
    body.op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind,
    });
    body.0.blocks[0].terminator = Terminator::Return(Some(value));
    body.0
}

fn with_entry(kind: CallableKind, function: TypedFunction) -> TypedModule {
    let result_type = function.return_type.clone();
    let mut main = Body::new(result_type.clone());
    let component = main.package_value();
    let closure = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![component],
        },
    );
    let saved = main.storage(Type::Closure(ClosureId(0)));
    main.op(InstructionKind::Store {
        local: saved,
        value: closure,
    });
    let call = match kind {
        CallableKind::Fn => InstructionKind::CallClosureShared {
            callee: saved,
            arguments: vec![],
        },
        CallableKind::FnMut => InstructionKind::CallClosureMut {
            callee: saved,
            arguments: vec![],
        },
        CallableKind::FnOnce => {
            let moved = main.value(
                Type::Closure(ClosureId(0)),
                InstructionKind::TakeLocal { local: saved },
            );
            InstructionKind::CallClosureOnce {
                callee: moved,
                arguments: vec![],
            }
        }
        _ => unreachable!(),
    };
    if kind != CallableKind::FnOnce {
        main.value(result_type.clone(), call.clone());
    }
    let value = main.value(result_type.clone(), call);
    main.op(InstructionKind::DropClosure { local: saved });
    let mut m = module(main.finish(Some(value), CleanupExitKind::Return));
    m.functions.push(function);
    m.closures.push(ClosureDefinition {
        storage_life: None,
        lending_captures: Vec::new(),
        id: ClosureId(0),
        function: FunctionId(1),
        kind,
        captures: vec![package()],
        parameters: vec![],
        result: result_type,
    });
    m
}

fn direct_store_sum() -> Type {
    Type::Enum {
        name: "CaptureChoice935".into(),
        variants: vec![
            (
                "Own".into(),
                vec![Type::Managed(Box::new(Type::OwnedStore {
                    element: Box::new(Type::I32),
                }))],
            ),
            ("Empty".into(), vec![]),
        ],
    }
}

// The selected Empty arm is complete and live even though it has no Managed
// leaf. Its capture flag must still distinguish it from a taken empty slot.
fn direct_store_sum_capture() -> TypedModule {
    let sum = direct_store_sum();
    let mut main = Body::new(Type::I32);
    let initial = main.value(
        sum.clone(),
        InstructionKind::MakeVariant {
            variant: 1,
            fields: vec![],
        },
    );
    let closure = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![initial],
        },
    );
    let saved = main.storage(Type::Closure(ClosureId(0)));
    main.op(InstructionKind::Store {
        local: saved,
        value: closure,
    });
    let result = main.value(
        Type::I32,
        InstructionKind::CallClosureMut {
            callee: saved,
            arguments: vec![],
        },
    );
    main.op(InstructionKind::DropClosure { local: saved });
    let mut entry = Body::new(Type::I32);
    entry.0.id = FunctionId(1);
    entry.0.name = "direct_sum_mutator".into();
    entry.0.environment = Some(ClosureId(0));
    let taken = entry.value(sum.clone(), InstructionKind::TakeCapture { index: 0 });
    let previous = entry.storage(sum.clone());
    entry.op(InstructionKind::Store {
        local: previous,
        value: taken,
    });
    let refill = entry.value(sum.clone(), InstructionKind::TakeLocal { local: previous });
    entry.op(InstructionKind::WriteCapture {
        index: 0,
        value: refill,
    });
    let answer = entry.value(Type::I32, InstructionKind::Constant(Constant::I32(7)));
    let mut module = module(main.finish(Some(result), CleanupExitKind::Return));
    module
        .functions
        .push(entry.finish(Some(answer), CleanupExitKind::Return));
    module.closures.push(ClosureDefinition {
        storage_life: None,
        lending_captures: vec![],
        id: ClosureId(0),
        function: FunctionId(1),
        kind: CallableKind::FnMut,
        captures: vec![sum],
        parameters: vec![],
        result: Type::I32,
    });
    module.resources.push(ResourceDefinition {
        name: crate::dynamic_store::definition_name(&Type::I32),
        payload: Type::OwnedStore {
            element: Box::new(Type::I32),
        },
        teardown: None,
    });
    module
}

fn direct_sum_rejected(module: TypedModule, code: &str, message: &str) {
    let (raw, wire) = check_both(module);
    for result in [raw, wire] {
        let errors = result.unwrap_err();
        assert_eq!(errors[0].code, code, "{errors:?}");
        assert_eq!(errors[0].message, message, "{errors:?}");
    }
}

fn direct_store_sum_error_exit() -> TypedModule {
    let mut module = direct_store_sum_capture();
    let result_type = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(Type::I32),
    };
    let main = &mut module.functions[0];
    let call = main.blocks[0]
        .instructions
        .iter()
        .find_map(|i| matches!(i.kind, InstructionKind::CallClosureMut { .. }).then_some(i.result))
        .flatten()
        .unwrap();
    main.values[call.0 as usize] = result_type.clone();
    let returned = ValueId(main.values.len() as u32);
    main.values.push(Type::I32);
    let at = main.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
        .unwrap();
    main.blocks[0].instructions.insert(
        at,
        ins(Some(returned), InstructionKind::Constant(Constant::I32(7))),
    );
    main.blocks[0].terminator = Terminator::Return(Some(returned));

    let entry = &mut module.functions[1];
    entry.return_type = result_type.clone();
    let code = ValueId(2);
    let error = ValueId(entry.values.len() as u32);
    entry.values.push(result_type.clone());
    let at = entry.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::ScopeExit { .. }))
        .unwrap();
    entry.blocks[0].instructions.insert(
        at,
        ins(
            Some(error),
            InstructionKind::MakeErrorUnion {
                is_error: true,
                value: code,
            },
        ),
    );
    let InstructionKind::ScopeExit { kind, .. } = &mut entry.blocks[0].instructions[at + 1].kind
    else {
        unreachable!()
    };
    *kind = CleanupExitKind::Error;
    entry.blocks[0].terminator = Terminator::Return(Some(error));
    module.closures[0].result = result_type;
    module
}

#[test]
fn direct_store_sum_fnmut_error_exit_requires_exact_refill() {
    accepted(direct_store_sum_error_exit());
    let mut missing = direct_store_sum_error_exit();
    let entry = &mut missing.functions[1];
    let take = entry.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::TakeLocal { .. }))
        .unwrap();
    take.result = None;
    take.kind = InstructionKind::DropEnum { local: LocalId(0) };
    entry.values.remove(1);
    for instruction in &mut entry.blocks[0].instructions {
        if instruction.result == Some(ValueId(2)) {
            instruction.result = Some(ValueId(1));
        } else if instruction.result == Some(ValueId(3)) {
            instruction.result = Some(ValueId(2));
        }
        if let InstructionKind::MakeErrorUnion { value, .. } = &mut instruction.kind {
            *value = ValueId(1);
        }
    }
    entry.blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::WriteCapture { .. }));
    entry.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    direct_sum_rejected(
        missing,
        "E-VERIFY-SUM",
        "Fn/FnMut return leaves an unfilled direct Sum capture",
    );
}

#[test]
fn direct_store_sum_capture_refill_distinguishes_empty_arm_from_empty_slot() {
    accepted(direct_store_sum_capture());
    let mut missing = direct_store_sum_capture();
    let entry = &mut missing.functions[1];
    let op = entry.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::TakeLocal { .. }))
        .unwrap();
    op.result = None;
    op.kind = InstructionKind::DropEnum { local: LocalId(0) };
    entry.values.remove(1);
    entry.blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    for instruction in &mut entry.blocks[0].instructions {
        if instruction.result == Some(ValueId(2)) {
            instruction.result = Some(ValueId(1));
        }
    }
    entry.blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::WriteCapture { .. }));
    direct_sum_rejected(
        missing,
        "E-VERIFY-SUM",
        "Fn/FnMut return leaves an unfilled direct Sum capture",
    );
}

#[test]
fn direct_store_sum_capture_rejects_copy_take_twice_and_live_refill() {
    let mut copied = direct_store_sum_capture();
    copied.functions[1].blocks[0].instructions[1].kind = InstructionKind::ReadCapture { index: 0 };
    direct_sum_rejected(
        copied,
        "E-VERIFY-CAPTURE",
        "owned captures require their explicit resource or Package view operations, never copying ReadCapture",
    );

    let mut twice = direct_store_sum_capture();
    let entry = &mut twice.functions[1];
    let second = ValueId(entry.values.len() as u32);
    entry.values.push(direct_store_sum());
    entry.blocks[0].instructions.insert(
        2,
        ins(Some(second), InstructionKind::TakeCapture { index: 0 }),
    );
    direct_sum_rejected(
        twice,
        "E-VERIFY-SUM",
        "direct Sum capture was already transferred",
    );

    let mut live_write = direct_store_sum_capture();
    let entry = &mut live_write.functions[1];
    let fresh = ValueId(entry.values.len() as u32);
    entry.values.push(direct_store_sum());
    entry.blocks[0].instructions.insert(
        1,
        ins(
            Some(fresh),
            InstructionKind::MakeVariant {
                variant: 1,
                fields: vec![],
            },
        ),
    );
    entry.blocks[0].instructions.insert(
        2,
        ins(
            None,
            InstructionKind::WriteCapture {
                index: 0,
                value: fresh,
            },
        ),
    );
    direct_sum_rejected(
        live_write,
        "E-VERIFY-SUM",
        "direct Sum capture refill would overwrite an active owner",
    );

    let mut shared = direct_store_sum_capture();
    shared.closures[0].kind = CallableKind::Fn;
    let call = shared.functions[0].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::CallClosureMut { .. }))
        .unwrap();
    call.kind = InstructionKind::CallClosureShared {
        callee: LocalId(0),
        arguments: vec![],
    };
    direct_sum_rejected(
        shared,
        "E-VERIFY-CLOSURE-CALL",
        "moving an owned capture requires a Consume entry",
    );

    let mut wrong_index = direct_store_sum_capture();
    let write = wrong_index.functions[1].blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::WriteCapture { .. }))
        .unwrap();
    if let InstructionKind::WriteCapture { index, .. } = &mut write.kind {
        *index = 1;
    }
    direct_sum_rejected(wrong_index, "E-VERIFY-CAPTURE", "capture field is absent");

    let mut spent_value = direct_store_sum_capture();
    let entry = &mut spent_value.functions[1];
    let second_write = entry.blocks[0]
        .instructions
        .iter()
        .find(|i| matches!(i.kind, InstructionKind::WriteCapture { .. }))
        .unwrap()
        .clone();
    let at = entry.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::WriteCapture { .. }))
        .unwrap();
    entry.blocks[0].instructions.insert(at + 1, second_write);
    direct_sum_rejected(
        spent_value,
        "E-VERIFY-SUM",
        "direct Sum capture refill would overwrite an active owner",
    );
}

#[test]
fn direct_store_sum_capture_has_exact_fresh_raw_wire_budget() {
    let module = direct_store_sum_capture();
    let plan = resource_tests::plan(&module);
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    for wire in [false, true] {
        let check = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if wire {
                verify_module_bytes(&bytes, &module.target, &limits)
            } else {
                verify_module(module.clone(), plan.clone(), &module.target, &limits)
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        check(high).unwrap();
        while low < high {
            let mid = low + (high - low) / 2;
            if check(mid).is_ok() {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert!(low > 0);
        check(low).unwrap();
        assert_eq!(
            check(low - 1).unwrap_err()[0].category,
            zil_base::ResultCategory::ResourceLimit
        );
        check(low).unwrap();
    }
}

fn read_candidate(kind: CallableKind) -> TypedModule {
    let (mut body, local) = entry(Type::I32);
    let d = body.value(
        desc(),
        InstructionKind::DescribePackageField { local, index: 0 },
    );
    let value = body.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    with_entry(kind, finish_entry(body, value, CleanupExitKind::Return))
}

fn replace_candidate(kind: CallableKind) -> TypedModule {
    let (mut body, local) = entry(Type::I32);
    let replacement = body.token(67);
    body.op(InstructionKind::DropPackageField { local, index: 0 });
    body.op(InstructionKind::StorePackageField {
        local,
        index: 0,
        value: replacement,
    });
    let value = body.value(
        Type::I32,
        InstructionKind::ReadPackageField { local, index: 2 },
    );
    body.op(InstructionKind::StorePackageField {
        local,
        index: 2,
        value,
    });
    with_entry(kind, finish_entry(body, value, CleanupExitKind::Return))
}

fn partial_candidate(kind: CallableKind) -> TypedModule {
    let (mut body, local) = entry(Type::I32);
    let taken = body.value(
        owned(),
        InstructionKind::TakePackageField { local, index: 0 },
    );
    let destination = body.storage(owned());
    body.op(InstructionKind::Store {
        local: destination,
        value: taken,
    });
    let value = body.value(
        Type::I32,
        InstructionKind::ReadPackageField { local, index: 2 },
    );
    body.op(InstructionKind::DropManaged { local: destination });
    with_entry(kind, finish_entry(body, value, CleanupExitKind::Return))
}

#[test]
fn package_captures_are_owned_readonly_repeatable_or_consuming() {
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        accepted(read_candidate(kind));
    }
    accepted(replace_candidate(CallableKind::FnMut));
    accepted(replace_candidate(CallableKind::FnOnce));
    accepted(partial_candidate(CallableKind::FnOnce));
    rejected(
        partial_candidate(CallableKind::FnMut),
        "does not preserve every owned field",
    );
}

#[test]
fn package_capture_cannot_be_implicitly_copied_or_taken_without_view() {
    let mut m = read_candidate(CallableKind::Fn);
    let f = &mut m.functions[0];
    let value = ValueId(f.values.len() as u32);
    f.values.push(Type::Closure(ClosureId(0)));
    f.blocks[0]
        .instructions
        .insert(10, ins(Some(value), InstructionKind::Load(LocalId(0))));
    rejected(m, "never copying Load");
    let mut m = read_candidate(CallableKind::FnOnce);
    m.functions[1].blocks[0].instructions[2].kind = InstructionKind::ReadCapture { index: 0 };
    m.functions[1].values[0] = package();
    rejected(m, "never copying ReadCapture");
    let mut m = read_candidate(CallableKind::FnOnce);
    m.functions[1].blocks[0].instructions[2].kind = InstructionKind::TakeCapture { index: 0 };
    m.functions[1].values[0] = package();
    rejected(m, "Managed field");
}

#[test]
fn unique_binding_rejects_duplicate_capture_and_duplicate_view() {
    for same_local in [true, false] {
        let mut m = read_candidate(CallableKind::Fn);
        let f = &mut m.functions[1];
        let local = if same_local {
            LocalId(0)
        } else {
            let mut declaration = f.locals[0].clone();
            declaration.id = LocalId(1);
            f.locals.push(declaration);
            LocalId(1)
        };
        f.blocks[0].instructions.insert(
            2,
            ins(
                None,
                InstructionKind::BindPackageCapture { local, index: 0 },
            ),
        );
        rejected(m, "one-to-one and unique");
    }
}

#[test]
fn bindings_reject_wrong_indices_types_and_orphan_environment() {
    let mut m = read_candidate(CallableKind::Fn);
    m.functions[1].blocks[0].instructions[1].kind = InstructionKind::BindPackageCapture {
        local: LocalId(0),
        index: 99,
    };
    rejected(m, "outside the environment");
    let mut m = read_candidate(CallableKind::Fn);
    m.functions[1].locals[0].ty = owned();
    rejected(m, "exactly match");
    let mut m = read_candidate(CallableKind::Fn);
    m.functions[1].environment = None;
    rejected(m, "one-to-one");
    let mut m = read_candidate(CallableKind::Fn);
    m.functions[0].blocks[0].instructions.insert(
        1,
        ins(
            None,
            InstructionKind::BindPackageCapture {
                local: LocalId(0),
                index: 0,
            },
        ),
    );
    rejected(m, "actual closure environment");
}

#[test]
fn package_capture_needs_unused_binding_contiguous_prefix_and_no_entry_edges() {
    let mut m = read_candidate(CallableKind::Fn);
    let f = &mut m.functions[1];
    f.blocks[0].instructions = vec![
        ins(None, InstructionKind::ScopeEnter { scope: ScopeId(0) }),
        ins(
            Some(ValueId(0)),
            InstructionKind::Constant(Constant::I32(1)),
        ),
        ins(
            None,
            InstructionKind::ScopeExit {
                scope: ScopeId(0),
                kind: CleanupExitKind::Return,
            },
        ),
    ];
    f.values = vec![Type::I32].into();
    f.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    rejected(m, "every Package capture requires");
    let mut m = read_candidate(CallableKind::Fn);
    let f = &mut m.functions[1];
    let value = ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    f.blocks[0].instructions.insert(
        1,
        ins(Some(value), InstructionKind::Constant(Constant::I32(0))),
    );
    rejected(m, "every Package capture requires");
    let mut m = read_candidate(CallableKind::Fn);
    m.functions[1].blocks[0].terminator = Terminator::Jump(BlockId(0));
    rejected(m, "incoming CFG edges");
}

#[test]
fn view_ordinary_storage_reset_duplicate_end_and_missing_end_are_rejected() {
    for kind in [
        InstructionKind::StorageLive(LocalId(0)),
        InstructionKind::StorageDead(LocalId(0)),
    ] {
        let mut m = read_candidate(CallableKind::Fn);
        let f = &mut m.functions[1];
        let position = f.blocks[0].instructions.len() - 2;
        f.blocks[0].instructions.insert(position, ins(None, kind));
        rejected(m, "cannot use ordinary StorageLive or StorageDead");
    }
    let mut m = read_candidate(CallableKind::Fn);
    let f = &mut m.functions[1];
    let position = f.blocks[0].instructions.len() - 1;
    f.blocks[0].instructions.insert(
        position,
        ins(
            None,
            InstructionKind::EndPackageCapture {
                local: LocalId(0),
                index: 0,
            },
        ),
    );
    rejected(m, "no active view");
    let mut m = read_candidate(CallableKind::Fn);
    m.functions[1].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::EndPackageCapture { .. }));
    rejected(m, "scope exit leaves live storage");
}

#[test]
fn end_must_match_binding_and_cannot_be_followed_by_direct_or_desc_access() {
    let mut m = read_candidate(CallableKind::Fn);
    let f = &mut m.functions[1];
    let mut other = f.locals[0].clone();
    other.id = LocalId(1);
    f.locals.push(other);
    let position = f.blocks[0].instructions.len() - 2;
    f.blocks[0].instructions[position].kind = InstructionKind::EndPackageCapture {
        local: LocalId(1),
        index: 0,
    };
    rejected(m, "match the unique capture binding");
    for after_description in [false, true] {
        let mut m = read_candidate(CallableKind::Fn);
        let f = &mut m.functions[1];
        let position = f.blocks[0].instructions.len() - 2;
        let end = f.blocks[0].instructions.remove(position);
        f.blocks[0]
            .instructions
            .insert(if after_description { 3 } else { 2 }, end);
        rejected(
            m,
            if after_description {
                "invalidate a live description"
            } else {
                "before initialization"
            },
        );
    }
}

#[test]
fn shared_mode_rejects_copy_field_payload_and_responsibility_writes() {
    rejected(
        replace_candidate(CallableKind::Fn),
        "shared Fn entry cannot write or consume",
    );
    rejected(
        partial_candidate(CallableKind::Fn),
        "shared Fn entry cannot write or consume",
    );
    let mut m = read_candidate(CallableKind::Fn);
    m.functions[1].blocks[0].instructions.insert(
        4,
        ins(
            None,
            InstructionKind::WriteDesc {
                desc: ValueId(0),
                index: 0,
                value: ValueId(1),
            },
        ),
    );
    rejected(m, "shared Fn entry cannot write an owned resource capture");
    let (mut body, local) = entry(Type::I32);
    let value = body.value(
        Type::I32,
        InstructionKind::ReadPackageField { local, index: 2 },
    );
    body.op(InstructionKind::StorePackageField {
        local,
        index: 2,
        value,
    });
    rejected(
        with_entry(
            CallableKind::Fn,
            finish_entry(body, value, CleanupExitKind::Return),
        ),
        "shared Fn entry cannot write or consume",
    );
}

fn indirect_write(nested: bool) -> TypedModule {
    let mut m = read_candidate(CallableKind::Fn);
    let mut helper = Body::new(Type::I32);
    helper.0.id = FunctionId(2);
    helper.0.name = "writes_description".into();
    let d = if nested {
        helper.0.environment = Some(ClosureId(1));
        helper.value(desc(), InstructionKind::ReadCapture { index: 0 })
    } else {
        let parameter = helper.parameter(desc());
        helper.value(desc(), InstructionKind::Load(parameter))
    };
    let n = helper.value(Type::I32, InstructionKind::ReadDesc { desc: d, index: 0 });
    helper.op(InstructionKind::WriteDesc {
        desc: d,
        index: 0,
        value: n,
    });
    m.functions
        .push(helper.finish(Some(n), CleanupExitKind::Return));
    if nested {
        m.closures.push(ClosureDefinition {
            storage_life: None,
            lending_captures: Vec::new(),
            id: ClosureId(1),
            function: FunctionId(2),
            kind: CallableKind::Fn,
            captures: vec![desc()],
            parameters: vec![],
            result: Type::I32,
        });
        let f = &mut m.functions[1];
        let value = ValueId(f.values.len() as u32);
        f.values.push(Type::Closure(ClosureId(1)));
        f.blocks[0].instructions.insert(
            3,
            ins(
                Some(value),
                InstructionKind::MakeClosure {
                    closure: ClosureId(1),
                    captures: vec![ValueId(0)],
                },
            ),
        );
        f.blocks[0].instructions[4].kind = InstructionKind::CallClosure {
            callee: value,
            arguments: vec![],
        };
    } else {
        m.functions[1].blocks[0].instructions[3].kind = InstructionKind::Call {
            function: FunctionId(2),
            arguments: vec![ValueId(0)],
        };
    }
    m
}

#[test]
fn shared_package_desc_cannot_write_through_callee_or_nested_closure() {
    for nested in [false, true] {
        rejected(
            indirect_write(nested),
            "shared Fn entry cannot write an owned resource capture",
        );
    }
}

#[test]
fn exclusive_whole_transfer_restore_and_explicit_replace_preserve_environment() {
    let (mut body, local) = entry(Type::I32);
    let value = body.value(
        Type::I32,
        InstructionKind::ReadPackageField { local, index: 2 },
    );
    let moved = body.value(package(), InstructionKind::TakeLocal { local });
    body.op(InstructionKind::Store {
        local,
        value: moved,
    });
    accepted(with_entry(
        CallableKind::FnMut,
        finish_entry(body, value, CleanupExitKind::Return),
    ));
    let (mut body, local) = entry(Type::I32);
    let ordinary = body.storage(owned());
    let token = body.token(70);
    body.op(InstructionKind::Store {
        local: ordinary,
        value: token,
    });
    body.op(InstructionKind::DropPackage { local });
    let replacement = body.package_value();
    body.op(InstructionKind::Store {
        local,
        value: replacement,
    });
    let value = body.value(
        Type::I32,
        InstructionKind::ReadPackageField { local, index: 2 },
    );
    // An env view never becomes a newer ordinary local that blocks this drop.
    body.op(InstructionKind::DropManaged { local: ordinary });
    accepted(with_entry(
        CallableKind::FnMut,
        finish_entry(body, value, CleanupExitKind::Return),
    ));
}

#[test]
fn consume_whole_moved_view_can_end_but_exclusive_cannot_return_it() {
    let (mut body, local) = entry(Type::I32);
    let value = body.value(
        Type::I32,
        InstructionKind::ReadPackageField { local, index: 2 },
    );
    let moved = body.value(package(), InstructionKind::TakeLocal { local });
    let destination = body.storage(package());
    body.op(InstructionKind::Store {
        local: destination,
        value: moved,
    });
    body.op(InstructionKind::DropPackage { local: destination });
    let function = finish_entry(body, value, CleanupExitKind::Return);
    accepted(with_entry(CallableKind::FnOnce, function.clone()));
    rejected(
        with_entry(CallableKind::FnMut, function),
        "does not preserve every owned field",
    );
}

#[test]
fn exclusive_error_return_must_also_preserve_full_environment() {
    let result_type = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(Type::I32),
    };
    let (mut body, local) = entry(result_type.clone());
    body.op(InstructionKind::DropPackageField { local, index: 0 });
    let code = body.value(Type::I32, InstructionKind::Constant(Constant::I32(7)));
    let error = body.value(
        result_type,
        InstructionKind::MakeErrorUnion {
            is_error: true,
            value: code,
        },
    );
    let function = finish_entry(body, error, CleanupExitKind::Error);
    accepted(with_entry(CallableKind::FnOnce, function.clone()));
    rejected(
        with_entry(CallableKind::FnMut, function),
        "does not preserve every owned field",
    );
}

#[test]
fn trap_after_partial_exclusive_update_has_no_return_cleanup_promise() {
    let (mut body, local) = entry(Type::I32);
    body.op(InstructionKind::DropPackageField { local, index: 0 });
    accepted(with_entry(CallableKind::FnMut, body.0));
}

#[test]
fn stale_capture_description_cannot_revive_after_replace() {
    let (mut body, local) = entry(Type::I32);
    let old = body.value(
        desc(),
        InstructionKind::DescribePackageField { local, index: 0 },
    );
    body.op(InstructionKind::DropPackageField { local, index: 0 });
    let replacement = body.token(71);
    body.op(InstructionKind::StorePackageField {
        local,
        index: 0,
        value: replacement,
    });
    let value = body.value(
        Type::I32,
        InstructionKind::ReadDesc {
            desc: old,
            index: 0,
        },
    );
    rejected(
        with_entry(
            CallableKind::FnMut,
            finish_entry(body, value, CleanupExitKind::Return),
        ),
        "invalidate a live description",
    );
}

#[test]
fn complete_package_factory_input_and_owned_cleanup_contract_are_checked() {
    let mut m = read_candidate(CallableKind::FnMut);
    let mut factory = Body::new(Type::Closure(ClosureId(0)));
    let input = factory.parameter(package());
    let moved = factory.value(package(), InstructionKind::TakeLocal { local: input });
    let closure = factory.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![moved],
        },
    );
    factory.0.callable_return_contract = Some(CallableReturnContract {
        source: zil_ir::CallableReturnSource::Fresh,
        lending_captures: Vec::new(),
        closure: ClosureId(0),
        kind: CallableKind::FnMut,
        hold: vec![],
        call: vec![],
        writes: vec![],
        cleanup: vec![0],
    });
    m.functions[0] = factory.finish(Some(closure), CleanupExitKind::Return);
    accepted(m.clone());
    m.functions[0]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .cleanup
        .clear();
    rejected(m, "cleanup must name every owned capture");
}

#[test]
fn capture_bind_has_no_result_and_claimed_initialization_is_untrusted() {
    let mut m = read_candidate(CallableKind::Fn);
    m.functions[1].blocks[0].instructions[1].result = Some(ValueId(2));
    m.functions[1].values.push(package());
    rejected(m, "result");
    let m = read_candidate(CallableKind::Fn);
    let mut plan = resource_tests::plan(&m);
    plan.functions[1].blocks[0].initialized_out.push(LocalId(0));
    let bytes = zil_ir::wire::encode_module_candidate(&m, &plan, &Limits::default()).unwrap();
    for result in [
        verify_module(m.clone(), plan, &m.target, &Limits::default()),
        verify_module_bytes(&bytes, &m.target, &Limits::default()),
    ] {
        let errors = result.unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.code == "E-VERIFY-INITIALIZATION-CLAIM"),
            "{errors:?}"
        );
    }
}

#[test]
fn capture_responsibility_budget_has_exact_n_and_n_minus_one_boundary() {
    let m = replace_candidate(CallableKind::FnMut);
    let plan = resource_tests::plan(&m);
    let bytes = zil_ir::wire::encode_module_candidate(&m, &plan, &Limits::default()).unwrap();
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    while low < high {
        let middle = low + (high - low) / 2;
        let limits = Limits {
            max_analysis_steps: middle,
            ..Limits::default()
        };
        if verify_module(m.clone(), plan.clone(), &m.target, &limits).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(low > 0);
    eprintln!(
        "Package capture exact raw/wire analysis budget: N={low}, N-1={}",
        low - 1
    );
    for steps in [low - 1, low] {
        let limits = Limits {
            max_analysis_steps: steps,
            ..Limits::default()
        };
        for result in [
            verify_module(m.clone(), plan.clone(), &m.target, &limits),
            verify_module_bytes(&bytes, &m.target, &limits),
        ] {
            if steps == low {
                result.unwrap();
            } else {
                assert_eq!(
                    result.unwrap_err()[0].category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
    }
}

fn branch_candidate(kind: CallableKind, restore_error: bool, join: bool) -> TypedModule {
    let result_type = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(Type::I32),
    };
    let (mut body, local) = entry(result_type.clone());
    let condition = body.value(Type::Bool, InstructionKind::Constant(Constant::Bool(true)));
    let code = body.value(Type::I32, InstructionKind::Constant(Constant::I32(8)));
    let success = ValueId(body.0.values.len() as u32);
    body.0.values.push(result_type.clone());
    let error = ValueId(body.0.values.len() as u32);
    body.0.values.push(result_type);
    let mut ok = vec![ins(
        Some(success),
        InstructionKind::MakeErrorUnion {
            is_error: false,
            value: code,
        },
    )];
    let mut failed = vec![ins(
        None,
        InstructionKind::DropPackageField { local, index: 0 },
    )];
    if restore_error {
        let resource = ValueId(body.0.values.len() as u32);
        body.0.values.push(owned());
        failed.push(ins(
            Some(resource),
            InstructionKind::MakeManaged { fields: vec![code] },
        ));
        failed.push(ins(
            None,
            InstructionKind::StorePackageField {
                local,
                index: 0,
                value: resource,
            },
        ));
    }
    failed.push(ins(
        Some(error),
        InstructionKind::MakeErrorUnion {
            is_error: true,
            value: code,
        },
    ));
    body.0.blocks[0].terminator = Terminator::Branch {
        condition,
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    if join {
        // Both values are deliberately unused: a fresh success at the join
        // observes whether LIVE|DONE can falsely become definitely LIVE.
        body.0.blocks.push(BasicBlock {
            id: BlockId(1),
            instructions: ok,
            terminator: Terminator::Jump(BlockId(3)),
        });
        body.0.blocks.push(BasicBlock {
            id: BlockId(2),
            instructions: failed,
            terminator: Terminator::Jump(BlockId(3)),
        });
        let result = ValueId(body.0.values.len() as u32);
        body.0.values.push(body.0.return_type.clone());
        body.0.blocks.push(BasicBlock {
            id: BlockId(3),
            instructions: vec![
                ins(
                    Some(result),
                    InstructionKind::MakeErrorUnion {
                        is_error: false,
                        value: code,
                    },
                ),
                ins(None, InstructionKind::EndPackageCapture { local, index: 0 }),
                ins(
                    None,
                    InstructionKind::ScopeExit {
                        scope: ScopeId(0),
                        kind: CleanupExitKind::Return,
                    },
                ),
            ],
            terminator: Terminator::Return(Some(result)),
        });
    } else {
        ok.extend([
            ins(None, InstructionKind::EndPackageCapture { local, index: 0 }),
            ins(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ]);
        failed.extend([
            ins(None, InstructionKind::EndPackageCapture { local, index: 0 }),
            ins(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Error,
                },
            ),
        ]);
        body.0.blocks.push(BasicBlock {
            id: BlockId(1),
            instructions: ok,
            terminator: Terminator::Return(Some(success)),
        });
        body.0.blocks.push(BasicBlock {
            id: BlockId(2),
            instructions: failed,
            terminator: Terminator::Return(Some(error)),
        });
    }
    with_entry(kind, body.0)
}

#[test]
fn every_error_branch_and_join_must_restore_exclusive_fields() {
    for join in [false, true] {
        accepted(branch_candidate(CallableKind::FnMut, true, join));
        accepted(branch_candidate(CallableKind::FnOnce, false, join));
        rejected(
            branch_candidate(CallableKind::FnMut, false, join),
            "does not preserve every owned field",
        );
    }
}

#[test]
fn view_activity_must_agree_at_a_cfg_join() {
    let mut m = branch_candidate(CallableKind::FnMut, true, true);
    m.functions[1].blocks[1].instructions.push(ins(
        None,
        InstructionKind::EndPackageCapture {
            local: LocalId(0),
            index: 0,
        },
    ));
    rejected(m, "incompatible cleanup scopes or storage ownership");
}

#[test]
fn ended_nested_description_alias_is_rejected_even_when_outer_mode_is_exclusive() {
    let mut m = indirect_write(true);
    m.closures[0].kind = CallableKind::FnMut;
    for instruction in &mut m.functions[0].blocks[0].instructions {
        if let InstructionKind::CallClosureShared { callee, arguments } = &instruction.kind {
            instruction.kind = InstructionKind::CallClosureMut {
                callee: *callee,
                arguments: arguments.clone(),
            };
        }
    }
    let f = &mut m.functions[1];
    let end_position = f.blocks[0].instructions.len() - 2;
    let end = f.blocks[0].instructions.remove(end_position);
    f.blocks[0].instructions.insert(4, end);
    rejected(m, "invalidate a live description");
}

#[test]
fn binding_after_prefix_unreachable_and_nonentry_are_rejected() {
    let mut m = read_candidate(CallableKind::Fn);
    let f = &mut m.functions[1];
    f.blocks[0].instructions.insert(
        4,
        ins(
            None,
            InstructionKind::BindPackageCapture {
                local: LocalId(0),
                index: 0,
            },
        ),
    );
    rejected(m, "consecutive entry prefix");
    let mut m = read_candidate(CallableKind::Fn);
    m.functions[1].blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![ins(
            None,
            InstructionKind::BindPackageCapture {
                local: LocalId(0),
                index: 0,
            },
        )],
        terminator: Terminator::Trap,
    });
    rejected(m, "consecutive entry prefix");
}

#[test]
fn package_view_addresses_and_immutable_rebinding_remain_closed() {
    let mut m = read_candidate(CallableKind::FnMut);
    let f = &mut m.functions[1];
    let address = ValueId(f.values.len() as u32);
    f.values.push(Type::Address {
        mutable: false,
        inner: Box::new(package()),
    });
    f.blocks[0].instructions.insert(
        2,
        ins(
            Some(address),
            InstructionKind::BorrowLocal { local: LocalId(0) },
        ),
    );
    rejected(m, "Package capture views cannot expose a local address");
    let (mut body, local) = entry(Type::I32);
    body.0.locals[0].mutable = false;
    let result = body.value(
        Type::I32,
        InstructionKind::ReadPackageField { local, index: 2 },
    );
    let moved = body.value(package(), InstructionKind::TakeLocal { local });
    body.op(InstructionKind::Store {
        local,
        value: moved,
    });
    rejected(
        with_entry(
            CallableKind::FnMut,
            finish_entry(body, result, CleanupExitKind::Return),
        ),
        "immutable",
    );
}

#[test]
fn mixed_capture_indices_keep_distinct_package_roots_and_managed_debts() {
    let (mut body, first) = entry(Type::I32);
    body.0.blocks[0].instructions[1].kind = InstructionKind::BindPackageCapture {
        local: first,
        index: 1,
    };
    let second = LocalId(1);
    let mut declaration = body.0.locals[0].clone();
    declaration.id = second;
    body.0.locals.push(declaration);
    body.op(InstructionKind::BindPackageCapture {
        local: second,
        index: 3,
    });
    let first_desc = body.value(
        desc(),
        InstructionKind::DescribePackageField {
            local: first,
            index: 0,
        },
    );
    body.op(InstructionKind::DropPackageField {
        local: second,
        index: 0,
    });
    let replacement = body.token(77);
    body.op(InstructionKind::StorePackageField {
        local: second,
        index: 0,
        value: replacement,
    });
    let managed_desc = body.value(desc(), InstructionKind::DescribeCapture { index: 2 });
    body.value(
        Type::I32,
        InstructionKind::ReadDesc {
            desc: managed_desc,
            index: 0,
        },
    );
    let value = body.value(
        Type::I32,
        InstructionKind::ReadDesc {
            desc: first_desc,
            index: 0,
        },
    );
    body.op(InstructionKind::EndPackageCapture {
        local: second,
        index: 3,
    });
    body.op(InstructionKind::EndPackageCapture {
        local: first,
        index: 1,
    });
    body.op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    body.0.blocks[0].terminator = Terminator::Return(Some(value));
    let mut main = Body::new(Type::I32);
    let scalar = main.value(Type::I32, InstructionKind::Constant(Constant::I32(1)));
    let package_a = main.package_value();
    let managed = main.token(78);
    let package_b = main.package_value();
    let closure = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![scalar, package_a, managed, package_b],
        },
    );
    let local = main.storage(Type::Closure(ClosureId(0)));
    main.op(InstructionKind::Store {
        local,
        value: closure,
    });
    let result = main.value(
        Type::I32,
        InstructionKind::CallClosureMut {
            callee: local,
            arguments: vec![],
        },
    );
    main.op(InstructionKind::DropClosure { local });
    let mut m = module(main.finish(Some(result), CleanupExitKind::Return));
    m.functions.push(body.0);
    m.closures.push(ClosureDefinition {
        storage_life: None,
        lending_captures: Vec::new(),
        id: ClosureId(0),
        function: FunctionId(1),
        kind: CallableKind::FnMut,
        captures: vec![Type::I32, package(), owned(), package()],
        parameters: vec![],
        result: Type::I32,
    });
    accepted(m.clone());
    m.functions[1].blocks[0].instructions[2].kind = InstructionKind::BindPackageCapture {
        local: first,
        index: 3,
    };
    rejected(m, "before initialization");
}

#[test]
fn exclusive_loop_restores_persistent_fields_without_rebinding_entry() {
    let (mut body, local) = entry(Type::I32);
    let condition = body.value(Type::Bool, InstructionKind::Constant(Constant::Bool(false)));
    let code = body.value(Type::I32, InstructionKind::Constant(Constant::I32(80)));
    let replacement = ValueId(body.0.values.len() as u32);
    body.0.values.push(owned());
    body.0.blocks[0].terminator = Terminator::Jump(BlockId(1));
    body.0.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: vec![
            ins(None, InstructionKind::DropPackageField { local, index: 0 }),
            ins(
                Some(replacement),
                InstructionKind::MakeManaged { fields: vec![code] },
            ),
            ins(
                None,
                InstructionKind::StorePackageField {
                    local,
                    index: 0,
                    value: replacement,
                },
            ),
        ],
        terminator: Terminator::Branch {
            condition,
            then_block: BlockId(1),
            else_block: BlockId(2),
        },
    });
    body.0.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: vec![
            ins(None, InstructionKind::EndPackageCapture { local, index: 0 }),
            ins(
                None,
                InstructionKind::ScopeExit {
                    scope: ScopeId(0),
                    kind: CleanupExitKind::Return,
                },
            ),
        ],
        terminator: Terminator::Return(Some(code)),
    });
    accepted(with_entry(CallableKind::FnMut, body.0.clone()));
    body.0.blocks[1].instructions.push(ins(
        None,
        InstructionKind::EndPackageCapture { local, index: 0 },
    ));
    rejected(
        with_entry(CallableKind::FnMut, body.0),
        "before initialization",
    );
}
