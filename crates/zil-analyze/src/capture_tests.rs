//! Raw caller windows, independent of source inference and Verify.
use super::*;

#[derive(Clone, Copy, Debug)]
enum Write {
    Live,
    AfterLast,
    Disjoint,
    BeforeCapture,
    BeforeFirst,
    IndirectLive,
    IndirectAfterLast,
    IndirectDisjoint,
    HelperLive,
    HelperAfterLast,
    HelperDisjoint,
    BorrowHelperLive,
    BorrowHelperAfterLast,
    BorrowHelperDisjoint,
}

fn fixture(kind: CallableKind, alias: bool, write: Write) -> TypedModule {
    let mut module = super::fixture(kind, false, 0);
    let mut main = Body::new(0);
    let count = main.local(Type::I32);
    let other = main.local(Type::I32);
    let zero = main.constant(0);
    let ten = main.constant(10);
    for local in [count, other] {
        main.op(InstructionKind::Store { local, value: zero });
    }
    let indirect = matches!(
        write,
        Write::IndirectLive
            | Write::IndirectAfterLast
            | Write::IndirectDisjoint
            | Write::HelperLive
            | Write::HelperAfterLast
            | Write::HelperDisjoint
    )
    .then(|| {
        main.value(
            Type::Address {
                mutable: true,
                inner: Box::new(Type::I32),
            },
            InstructionKind::BorrowLocalMut {
                local: if matches!(write, Write::IndirectDisjoint | Write::HelperDisjoint) {
                    other
                } else {
                    count
                },
            },
        )
    });
    let from_parent =
        indirect.is_some() && !matches!(write, Write::IndirectDisjoint | Write::HelperDisjoint);
    let address = Type::Address {
        mutable: kind == CallableKind::FnMut || from_parent,
        inner: Box::new(Type::I32),
    };
    if matches!(write, Write::BeforeCapture) {
        main.op(InstructionKind::Store {
            local: count,
            value: ten,
        });
    }
    let borrowed = if from_parent {
        indirect.unwrap()
    } else {
        main.value(
            address.clone(),
            if kind == CallableKind::FnMut {
                InstructionKind::BorrowLocalMut { local: count }
            } else {
                InstructionKind::BorrowLocal { local: count }
            },
        )
    };
    let captured = if alias {
        let local = main.local(address.clone());
        main.op(InstructionKind::Store {
            local,
            value: borrowed,
        });
        main.value(address.clone(), InstructionKind::Load(local))
    } else {
        borrowed
    };
    let environment = main.local(Type::Closure(ClosureId(0)));
    let closure = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![captured],
        },
    );
    main.op(InstructionKind::Store {
        local: environment,
        value: closure,
    });
    let call = if kind == CallableKind::FnMut {
        InstructionKind::CallClosureMut {
            callee: environment,
            arguments: vec![],
        }
    } else {
        InstructionKind::CallClosureShared {
            callee: environment,
            arguments: vec![],
        }
    };
    if matches!(write, Write::BeforeFirst) {
        main.op(InstructionKind::Store {
            local: count,
            value: ten,
        });
    }
    main.value(Type::I32, call.clone());
    match write {
        Write::Live | Write::Disjoint => main.op(InstructionKind::Store {
            local: if matches!(write, Write::Disjoint) {
                other
            } else {
                count
            },
            value: ten,
        }),
        Write::IndirectLive | Write::IndirectDisjoint => main.op(InstructionKind::WriteAddress {
            address: indirect.unwrap(),
            value: ten,
        }),
        _ => {}
    }
    if matches!(
        write,
        Write::HelperLive
            | Write::HelperDisjoint
            | Write::BorrowHelperLive
            | Write::BorrowHelperDisjoint
    ) {
        let pointer = indirect.unwrap_or_else(|| {
            main.value(
                Type::Address {
                    mutable: true,
                    inner: Box::new(Type::I32),
                },
                InstructionKind::BorrowLocalMut {
                    local: if matches!(write, Write::BorrowHelperDisjoint) {
                        other
                    } else {
                        count
                    },
                },
            )
        });
        main.value(
            Type::I32,
            InstructionKind::Call {
                function: FunctionId(2),
                arguments: vec![pointer],
            },
        );
    }
    let result = main.value(Type::I32, call);
    if matches!(write, Write::AfterLast) {
        main.op(InstructionKind::Store {
            local: count,
            value: ten,
        });
    } else if matches!(write, Write::IndirectAfterLast) {
        main.op(InstructionKind::WriteAddress {
            address: indirect.unwrap(),
            value: ten,
        });
        // The write address itself remains live after a legal delegated write.
        main.value(
            Type::I32,
            InstructionKind::ReadAddress {
                address: indirect.unwrap(),
            },
        );
    } else if matches!(write, Write::HelperAfterLast | Write::BorrowHelperAfterLast) {
        let pointer = indirect.unwrap_or_else(|| {
            main.value(
                Type::Address {
                    mutable: true,
                    inner: Box::new(Type::I32),
                },
                InstructionKind::BorrowLocalMut { local: count },
            )
        });
        main.value(
            Type::I32,
            InstructionKind::Call {
                function: FunctionId(2),
                arguments: vec![pointer],
            },
        );
    }
    if matches!(address, Type::Address { mutable: true, .. }) {
        main.op(InstructionKind::DropClosure { local: environment });
    }
    main.op(InstructionKind::StorageDead(environment));
    module.functions[0] = main.finish(result);
    module.closures[0].captures = vec![address.clone()];
    let mut entry = Body::new(1);
    entry.0.environment = Some(ClosureId(0));
    let read = if kind == CallableKind::Fn {
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        }
    } else {
        address
    };
    let captured = entry.value(read, InstructionKind::ReadCapture { index: 0 });
    let result = entry.value(
        Type::I32,
        InstructionKind::ReadAddress { address: captured },
    );
    if kind == CallableKind::FnMut {
        entry.op(InstructionKind::WriteAddress {
            address: captured,
            value: result,
        });
    }
    module.functions[1] = entry.finish(result);
    let mut helper = Body::new(2);
    let ty = Type::Address {
        mutable: true,
        inner: Box::new(Type::I32),
    };
    helper.0.locals.push(LocalDecl {
        id: LocalId(0),
        name: "target".into(),
        ty: ty.clone(),
        mutable: false,
        span: s(),
    });
    helper.0.parameters.push(LocalId(0));
    let pointer = helper.value(ty, InstructionKind::Load(LocalId(0)));
    let value = helper.constant(10);
    helper.op(InstructionKind::WriteAddress {
        address: pointer,
        value,
    });
    module.functions.push(helper.finish(value));
    module
}

#[test]
fn ordinary_capture_windows_reject_caller_writes_until_the_actual_last_use() {
    for kind in [CallableKind::Fn, CallableKind::FnMut] {
        for alias in [false, true] {
            for write in [
                Write::Live,
                Write::AfterLast,
                Write::Disjoint,
                Write::BeforeCapture,
                Write::BeforeFirst,
                Write::IndirectLive,
                Write::IndirectAfterLast,
                Write::IndirectDisjoint,
                Write::HelperLive,
                Write::HelperAfterLast,
                Write::HelperDisjoint,
                Write::BorrowHelperLive,
                Write::BorrowHelperAfterLast,
                Write::BorrowHelperDisjoint,
            ] {
                let module = fixture(kind, alias, write);
                let limits = Limits::default();
                let actual = analyze(&module, &limits);
                if matches!(
                    write,
                    Write::Live
                        | Write::BeforeFirst
                        | Write::IndirectLive
                        | Write::HelperLive
                        | Write::BorrowHelperLive
                ) {
                    assert_eq!(
                        actual.as_ref().unwrap_err()[0].code,
                        "E-BORROW-CONFLICT",
                        "{kind:?} alias={alias} {write:?}"
                    );
                } else {
                    actual
                        .as_ref()
                        .unwrap_or_else(|e| panic!("{kind:?} alias={alias} {write:?}: {e:?}"));
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
    }
}

#[test]
fn ordinary_capture_window_budget_is_request_local_exact_and_fresh() {
    for write in [
        Write::Live,
        Write::AfterLast,
        Write::IndirectLive,
        Write::IndirectAfterLast,
        Write::HelperLive,
        Write::HelperAfterLast,
        Write::BorrowHelperLive,
        Write::BorrowHelperAfterLast,
    ] {
        let module = fixture(CallableKind::Fn, true, write);
        let defaults = Limits::default();
        let mut budget = Budget::new(&defaults);
        let expected = analyze_budgeted(&module, &defaults, &mut budget, None);
        let exact = defaults.max_analysis_steps - budget.remaining();
        if matches!(write, Write::AfterLast) {
            assert_eq!(exact, 1291, "ordinary scalar capture keeps its old work");
        }
        let limits = Limits {
            max_analysis_steps: exact,
            ..defaults
        };
        assert_eq!(analyze(&module, &limits), expected);
        assert_eq!(
            analyze(
                &module,
                &Limits {
                    max_analysis_steps: exact - 1,
                    ..limits
                }
            )
            .unwrap_err()[0]
                .category,
            zil_base::ResultCategory::ResourceLimit
        );
        assert_eq!(analyze(&module, &limits), expected);
    }
}

#[test]
fn borrowed_environment_calls_delegate_the_position_without_writing_its_dependencies() {
    for kind in [CallableKind::Fn, CallableKind::FnMut] {
        let mut module = fixture(kind, false, Write::BeforeCapture);
        // Replace scalar update with a real nonescaping callable helper.
        let mut helper = Body::new(2);
        let address = Type::Address {
            mutable: true,
            inner: Box::new(Type::Closure(ClosureId(0))),
        };
        helper.0.locals.push(LocalDecl {
            id: LocalId(0),
            name: "f".into(),
            ty: address.clone(),
            mutable: false,
            span: s(),
        });
        helper.0.parameters.push(LocalId(0));
        let callee = helper.value(address.clone(), InstructionKind::Load(LocalId(0)));
        let result = helper.value(
            Type::I32,
            InstructionKind::CallClosureBorrowed {
                callee,
                kind: CallableKind::FnMut,
                arguments: vec![],
            },
        );
        helper.0.callable_contract = Some(CallableParameterContract {
            roles: vec![CallableRole {
                life: None,
                parameter: 0,
                closure: ClosureId(0),
                kind: CallableKind::FnMut,
                hold: vec![0],
                call: vec![0],
                writes: if kind == CallableKind::FnMut {
                    vec![0]
                } else {
                    vec![]
                },
                cleanup: CallableCleanup::Borrowed,
            }],
            traces: CallableTraces::Finite(vec![CallableTrace {
                calls: vec![CallEvent {
                    parameter: 0,
                    kind: CallableKind::FnMut,
                }],
                exit: CallableExit::Return,
            }]),
        });
        module.functions[2] = helper.finish(result);
        let main = &mut module.functions[0];
        let mut instructions = vec![];
        for mut instruction in std::mem::take(&mut main.blocks[0].instructions) {
            if let InstructionKind::CallClosureShared { callee, .. }
            | InstructionKind::CallClosureMut { callee, .. } = instruction.kind
            {
                let pointer = ValueId(main.values.len() as u32);
                main.values.push(address.clone());
                instructions.push(inst(
                    Some(pointer),
                    InstructionKind::BorrowLocalMut { local: callee },
                ));
                instruction.kind = InstructionKind::Call {
                    function: FunctionId(2),
                    arguments: vec![pointer],
                };
            }
            instructions.push(instruction);
        }
        main.blocks[0].instructions = instructions;
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

fn field_callback_argument(same_field: bool, after_last: bool) -> TypedModule {
    let mut module = super::fixture(CallableKind::Fn, false, 0);
    let package = Type::Package {
        name: "Fields".into(),
        fields: vec![
            ("token".into(), managed()),
            ("x".into(), Type::I32),
            ("y".into(), Type::I32),
        ],
    };
    let shared = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    let exclusive = Type::Address {
        mutable: true,
        inner: Box::new(Type::I32),
    };
    let mut main = Body::new(0);
    main.op(InstructionKind::ScopeEnter { scope: ScopeId(0) });
    let owner = main.local(package.clone());
    let one = main.constant(1);
    let token = main.value(
        managed(),
        InstructionKind::MakeManaged { fields: vec![one] },
    );
    let made = main.value(
        package,
        InstructionKind::MakePackage {
            fields: vec![token, one, one],
        },
    );
    main.op(InstructionKind::Store {
        local: owner,
        value: made,
    });
    let captured = main.value(
        shared.clone(),
        InstructionKind::BorrowPackageField {
            local: owner,
            index: 1,
            mutable: false,
        },
    );
    let environment = main.local(Type::Closure(ClosureId(0)));
    let closure = main.value(
        Type::Closure(ClosureId(0)),
        InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![captured],
        },
    );
    main.op(InstructionKind::Store {
        local: environment,
        value: closure,
    });
    let argument = main.value(
        exclusive.clone(),
        InstructionKind::BorrowPackageField {
            local: owner,
            index: if same_field && !after_last { 1 } else { 2 },
            mutable: true,
        },
    );
    let result = main.value(
        Type::I32,
        InstructionKind::CallClosureShared {
            callee: environment,
            arguments: vec![argument],
        },
    );
    if after_last {
        let address = main.value(
            exclusive.clone(),
            InstructionKind::BorrowPackageField {
                local: owner,
                index: 1,
                mutable: true,
            },
        );
        main.op(InstructionKind::WriteAddress {
            address,
            value: one,
        });
    }
    main.op(InstructionKind::StorageDead(environment));
    main.op(InstructionKind::DropPackage { local: owner });
    main.op(InstructionKind::StorageDead(owner));
    main.op(InstructionKind::ScopeExit {
        scope: ScopeId(0),
        kind: CleanupExitKind::Return,
    });
    module.functions[0] = main.finish(result);

    let mut entry = Body::new(1);
    entry.0.environment = Some(ClosureId(0));
    entry.0.locals.push(LocalDecl {
        id: LocalId(0),
        name: "p".into(),
        ty: exclusive.clone(),
        mutable: false,
        span: s(),
    });
    entry.0.parameters.push(LocalId(0));
    let argument = entry.value(exclusive.clone(), InstructionKind::Load(LocalId(0)));
    let old = entry.value(
        Type::I32,
        InstructionKind::ReadAddress { address: argument },
    );
    let one = entry.constant(1);
    let incremented = entry.value(
        Type::I32,
        InstructionKind::Binary {
            op: BinaryOp::Add,
            left: old,
            right: one,
        },
    );
    entry.op(InstructionKind::WriteAddress {
        address: argument,
        value: incremented,
    });
    let captured = entry.value(shared.clone(), InstructionKind::ReadCapture { index: 0 });
    let read = entry.value(
        Type::I32,
        InstructionKind::ReadAddress { address: captured },
    );
    let sum = entry.value(
        Type::I32,
        InstructionKind::Binary {
            op: BinaryOp::Add,
            left: read,
            right: incremented,
        },
    );
    module.functions[1] = entry.finish(sum);
    module.closures[0].captures = vec![shared];
    module.closures[0].parameters = vec![exclusive];
    module
}

#[test]
fn captured_copy_fields_keep_actual_siblings_and_whole_parent_windows_raw_wire() {
    for (same, after_last) in [(false, false), (true, false), (true, true)] {
        let module = field_callback_argument(same, after_last);
        let limits = Limits::default();
        let expected = analyze(&module, &limits);
        if same && !after_last {
            assert_eq!(expected.as_ref().unwrap_err()[0].code, "E-BORROW-CONFLICT");
        } else {
            let plan = expected.as_ref().unwrap();
            for instruction in &module.functions[0].blocks[0].instructions {
                if matches!(instruction.kind, InstructionKind::BorrowPackageField { .. }) {
                    let value = instruction.result.unwrap();
                    assert_eq!(
                        plan.functions[0].holds[value.0 as usize].roots,
                        vec![HoldRoot::Local(LocalId(0))]
                    );
                }
            }
        }
        let bytes = zil_ir::wire::encode_module_candidate(
            &module,
            &CandidatePlan::empty_for(&module),
            &limits,
        )
        .unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&bytes, &limits)
            .unwrap()
            .0;
        assert_eq!(analyze(&decoded, &limits), expected);
    }
    let mut whole = field_callback_argument(false, false);
    let f = &mut whole.functions[0];
    let at = f.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::CallClosureShared { .. }))
        .unwrap();
    let value = ValueId(f.values.len() as u32);
    f.values.push(f.locals[0].ty.clone());
    f.blocks[0].instructions.insert(
        at,
        inst(
            Some(value),
            InstructionKind::TakeLocal { local: LocalId(0) },
        ),
    );
    f.blocks[0].instructions.insert(
        at + 1,
        inst(
            None,
            InstructionKind::Store {
                local: LocalId(0),
                value,
            },
        ),
    );
    let defaults = Limits::default();
    let bytes =
        zil_ir::wire::encode_module_candidate(&whole, &CandidatePlan::empty_for(&whole), &defaults)
            .unwrap();
    let decoded = zil_ir::wire::decode_module_candidate(&bytes, &defaults)
        .unwrap()
        .0;
    for input in [&whole, &decoded] {
        assert_eq!(
            analyze(input, &defaults).unwrap_err()[0].code,
            "E-BORROW-CONFLICT"
        );
    }
}

#[test]
fn captured_copy_field_work_is_exact_and_fresh_raw_wire() {
    for (same, after_last) in [(false, false), (true, false), (true, true)] {
        let module = field_callback_argument(same, after_last);
        let defaults = Limits::default();
        let mut budget = Budget::new(&defaults);
        let expected = analyze_budgeted(&module, &defaults, &mut budget, None);
        if same && !after_last {
            assert_eq!(expected.as_ref().unwrap_err()[0].code, "E-BORROW-CONFLICT");
        } else {
            expected.as_ref().unwrap();
        }
        let exact = defaults.max_analysis_steps - budget.remaining();
        assert_eq!(
            exact,
            match (same, after_last) {
                (false, false) => 2387,
                (true, false) => 2202,
                (_, true) => 2560,
            }
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
                    assert_eq!(actual, expected);
                } else {
                    assert_eq!(
                        actual.unwrap_err()[0].category,
                        zil_base::ResultCategory::ResourceLimit
                    );
                }
            }
        }
        println!(
            "288 copy-field same={same} after-last={after_last}: N={exact} raw/Wire N-1/fresh"
        );
    }
}
