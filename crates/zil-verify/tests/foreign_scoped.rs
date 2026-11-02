//! Independent current raw/Wire trust-boundary tests, not source/native evidence.
#[path = "support/foreign_scoped.rs"]
mod fixture;
use zil_base::{Limits, ResultCategory};
use zil_ir::*;
fn check(module: TypedModule, plan: CandidatePlan, expected: Option<&str>) {
    check_category(module, plan, expected, ResultCategory::SafeReject)
}
fn check_category(
    module: TypedModule,
    mut plan: CandidatePlan,
    expected: Option<&str>,
    category: ResultCategory,
) {
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let target = module.target.clone();
    let raw = zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits);
    let wire = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let decoded = zil_verify::verify_module_bytes(&wire, &target, &limits);
    for result in [raw, decoded] {
        if let Some(code) = expected {
            let errors = result.unwrap_err();
            assert_eq!(errors[0].code, code, "{errors:?}");
            assert_eq!(errors[0].category, category);
        } else {
            result.unwrap();
        }
    }
}
#[test]
fn actual_mutable_environment_and_consumed_body_pass_raw_and_wire() {
    let (m, p) = fixture::fixture();
    check(m, p, None)
}
#[test]
fn callback_role_and_signature_are_rebuilt_from_declarations() {
    for change in 0..4 {
        let (mut m, p) = fixture::fixture();
        match change {
            0 => {
                m.functions[0].foreign.as_mut().unwrap().scoped_callback =
                    Some(ScopedCallbackRole::Close)
            }
            1 => m.functions[1].foreign.as_mut().unwrap().scoped_callback = None,
            2 => m.functions[0].return_type = Type::I32,
            3 => m.functions[1].foreign.as_mut().unwrap().static_callbacks = false,
            _ => unreachable!(),
        }
        check(m, p, Some("E-VERIFY-FOREIGN"));
    }
}
#[test]
fn missing_actual_unsafe_region_is_rejected() {
    let (mut m, p) = fixture::fixture();
    if let InstructionKind::WithForeignCallback { region, .. } =
        &mut m.functions[4].blocks[0].instructions[6].kind
    {
        *region = 8
    }
    check(m, p, Some("E-VERIFY-UNSAFE"));
}
#[test]
fn once_callback_and_shared_callback_address_are_rejected() {
    let (mut m, p) = fixture::fixture();
    m.closures[0].kind = CallableKind::FnOnce;
    check(m, p, Some("E-VERIFY-SCOPED-CALLBACK"));
    let (mut m, p) = fixture::fixture();
    if let Type::Address { mutable, .. } = &mut m.functions[4].values[1] {
        *mutable = false
    };
    m.functions[4].blocks[0].instructions[3].kind =
        InstructionKind::BorrowLocal { local: LocalId(0) };
    check(m, p, Some("E-VERIFY-SCOPED-CALLBACK"));
}
#[test]
fn body_responsibility_cannot_be_consumed_twice() {
    let (mut m, mut p) = fixture::fixture();
    m.functions[4].blocks[0].instructions.insert(
        7,
        fixture::instruction(
            None,
            InstructionKind::WithForeignCallback {
                register: FunctionId(0),
                close: FunctionId(1),
                callback: ValueId(1),
                body: ValueId(2),
                region: 7,
            },
        ),
    );
    m.functions[4]
        .values
        .push(Type::Tuple(vec![Type::Bool, Type::I32]));
    m.functions[4].blocks[0].instructions[7].result = Some(ValueId(4));
    p.functions[4].holds.push(ValueHold {
        value: ValueId(4),
        roots: vec![],
    });
    check(m, p, Some("E-VERIFY-RESOURCE"));
}
#[test]
fn body_can_borrow_unrelated_scalar_and_distinct_same_typed_closure() {
    let (m, p) = fixture::borrowed_scalar_body();
    check(m, p, None);
    let (m, p) = fixture::borrowed_closure_body(false);
    check(m, p, None);
}
#[test]
fn body_cannot_capture_a_reborrow_of_the_actual_callback_position() {
    let (m, p) = fixture::borrowed_closure_body(true);
    check(m, p, Some("E-BORROW-CONFLICT"));
}
#[test]
fn independent_two_role_wrapper_reconstructs_optional_once_and_repeated_mut_calls() {
    let (m, p) = fixture::wrapper();
    check(m, p, None);
}
#[test]
fn f_only_wrapper_constructs_actual_concrete_with_body() {
    for through_local in [false, true] {
        let (m, p) = fixture::local_body_wrapper(through_local);
        check(m, p, None);
    }
    for kind in [CallableKind::Fn, CallableKind::FnMut] {
        let (mut m, mut p) = fixture::local_body_wrapper(true);
        m.closures[1].kind = kind;
        m.functions[4].blocks[0].instructions[4].kind = InstructionKind::Load(LocalId(1));
        p.functions[4].blocks[0].initialized_out.push(LocalId(1));
        check(m, p, None);
    }
}
#[test]
fn concrete_local_owners_reach_with_and_remaining_cleanup_without_role_claims() {
    for package in [false, true] {
        for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            let (mut m, p) = fixture::local_owned_body(package);
            m.closures[1].kind = kind;
            check(m, p, None);
        }
    }
}
#[test]
fn factory_body_uses_checked_return_and_actual_local_value_without_fake_role() {
    for package in [false, true] {
        for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
            let (mut m, p) = fixture::factory_body(package);
            m.closures[1].kind = kind;
            m.functions[5]
                .callable_return_contract
                .as_mut()
                .unwrap()
                .kind = kind;
            check(m, p, None);
        }
    }
}
#[test]
fn factory_body_cannot_substitute_a_forged_return_contract_or_parameter_event() {
    for change in 0..5 {
        let (mut m, p) = fixture::factory_body(true);
        match change {
            0 => m.functions[5].callable_return_contract = None,
            1 => m.functions[5]
                .callable_return_contract
                .as_mut()
                .unwrap()
                .cleanup
                .clear(),
            2 => {
                m.functions[5]
                    .callable_return_contract
                    .as_mut()
                    .unwrap()
                    .kind = CallableKind::Fn
            }
            3 => {
                m.functions[5]
                    .callable_return_contract
                    .as_mut()
                    .unwrap()
                    .source = CallableReturnSource::Parameter(0)
            }
            4 => {
                let CallableTraces::Flow(flow) =
                    &mut m.functions[4].callable_contract.as_mut().unwrap().traces
                else {
                    unreachable!()
                };
                flow.nodes[2].calls.push(CallEvent {
                    parameter: 0,
                    kind: CallableKind::FnOnce,
                });
            }
            _ => unreachable!(),
        }
        check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
    }
}
#[test]
fn factory_body_same_type_new_instance_does_not_revive_consumed_actual_body() {
    for package in [false, true] {
        for consume_old in [false, true] {
            let (mut m, mut p) = fixture::factory_body(package);
            let f = &mut m.functions[4];
            f.locals[1].mutable = true;
            let owner = ValueId(f.values.len() as u32);
            f.values.push(f.values[5].clone());
            let mut preparation = vec![fixture::instruction(
                Some(owner.0),
                InstructionKind::MakeManaged {
                    fields: vec![ValueId(4)],
                },
            )];
            let argument = if package {
                let value = ValueId(f.values.len() as u32);
                f.values.push(f.values[6].clone());
                preparation.push(fixture::instruction(
                    Some(value.0),
                    InstructionKind::MakePackage {
                        fields: vec![owner],
                    },
                ));
                value
            } else {
                owner
            };
            let fresh = ValueId(f.values.len() as u32);
            f.values.push(Type::Closure(ClosureId(1)));
            let consumed = ValueId(f.values.len() as u32);
            f.values.push(Type::I32);
            preparation.extend([
                fixture::instruction(
                    Some(fresh.0),
                    InstructionKind::Call {
                        function: FunctionId(5),
                        arguments: vec![argument],
                    },
                ),
                fixture::instruction(
                    Some(consumed.0),
                    if consume_old {
                        InstructionKind::CallClosureOnce {
                            callee: ValueId(3),
                            arguments: vec![],
                        }
                    } else {
                        InstructionKind::Constant(Constant::I32(0))
                    },
                ),
                fixture::instruction(
                    None,
                    InstructionKind::Store {
                        local: LocalId(1),
                        value: fresh,
                    },
                ),
                fixture::instruction(None, InstructionKind::DropClosure { local: LocalId(1) }),
            ]);
            let at = f.blocks[0]
                .instructions
                .iter()
                .position(|i| matches!(i.kind, InstructionKind::UnsafeEnter { .. }))
                .unwrap();
            f.blocks[0].instructions.splice(at..at, preparation);
            for value in p.functions[4].holds.len()..f.values.len() {
                p.functions[4].holds.push(ValueHold {
                    value: ValueId(value as u32),
                    roots: vec![],
                });
            }
            check(m, p, consume_old.then_some("E-VERIFY-RESOURCE"));
        }
    }
}
#[test]
fn ordinary_helper_local_owner_needs_no_with_permission() {
    let (m, p) = fixture::ordinary_local_owner();
    check(m, p, None);
    let (mut m, p) = fixture::ordinary_local_owner();
    m.functions[4].blocks[0]
        .instructions
        .retain(|i| !matches!(i.kind, InstructionKind::DropManaged { .. }));
    check(m, p, Some("E-VERIFY-RESOURCE"));
}
#[test]
fn local_owned_preparation_leaks_and_duplicate_captures_reach_resource_kernel() {
    for package in [false, true] {
        let (mut m, mut p) = fixture::local_owned_body(package);
        let f = &mut m.functions[4];
        let leak = ValueId(f.values.len() as u32);
        f.values.push(f.values[5].clone());
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
            .unwrap();
        f.blocks[0].instructions.insert(
            at,
            fixture::instruction(
                Some(leak.0),
                InstructionKind::MakeManaged {
                    fields: vec![ValueId(4)],
                },
            ),
        );
        p.functions[4].holds.push(ValueHold {
            value: leak,
            roots: vec![],
        });
        check(m, p, Some("E-VERIFY-RESOURCE"));
    }
    let (mut m, p) = fixture::local_owned_body(false);
    let captured = m.closures[1].captures[0].clone();
    m.closures[1].captures.push(captured);
    for instruction in &mut m.functions[4].blocks[0].instructions {
        if let InstructionKind::MakeClosure { captures, .. } = &mut instruction.kind {
            captures.push(ValueId(5));
        }
    }
    check(m, p, Some("E-VERIFY-RESOURCE"));
}
#[test]
fn local_owned_body_still_cannot_capture_the_actual_f_root() {
    for package in [false, true] {
        let (mut m, mut p) = fixture::local_owned_body(package);
        let address = m.functions[4].values[0].clone();
        m.closures[1].captures.push(address.clone());
        let f = &mut m.functions[4];
        let alias = ValueId(f.values.len() as u32);
        f.values.push(address);
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::MakeClosure { .. }))
            .unwrap();
        f.blocks[0].instructions.insert(
            at,
            fixture::instruction(
                Some(alias.0),
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
        p.functions[4].holds.push(ValueHold {
            value: alias,
            roots: vec![HoldRoot::Parameter(0)],
        });
        for value in [1, 3] {
            p.functions[4].holds[value].roots = vec![HoldRoot::Parameter(0)];
        }
        // This helper captures its supplied role. The role-origin audit now
        // rejects it before ordinary alias checking; direct With aliases below
        // still exercise the independent borrow conflict path.
        check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
    }
}
#[test]
fn local_body_actual_capture_roots_keep_callback_exclusion_and_disjoint_borrows() {
    let (m, p) = fixture::local_body_capture_wrapper(false);
    check(m, p, None);
    let (m, p) = fixture::local_body_capture_wrapper(true);
    check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
    let (mut m, mut p) = fixture::local_body_wrapper(true);
    m.functions[4]
        .values
        .push(Type::Tuple(vec![Type::Bool, Type::I32]));
    let mut second = m.functions[4].blocks[0].instructions[6].clone();
    second.result = Some(ValueId(4));
    m.functions[4].blocks[0].instructions.insert(7, second);
    p.functions[4].holds.push(ValueHold {
        value: ValueId(4),
        roots: vec![],
    });
    // Give the two actual scopes their correct parameter-only flow so that
    // the independently checked once responsibility, not a forged claim,
    // rejects the repeated use of the same concrete body value.
    let flow = [
        (false, vec![1, 2, 3]),
        (true, vec![1, 2, 3]),
        (false, vec![4, 3]),
        (false, vec![5, 6, 7]),
        (true, vec![4, 3]),
        (true, vec![5, 6, 7]),
        (false, vec![8, 7]),
        (false, vec![]),
        (true, vec![8, 7]),
    ];
    m.functions[4].callable_contract.as_mut().unwrap().traces =
        CallableTraces::Flow(CallableFlow {
            entry: 0,
            nodes: flow
                .into_iter()
                .map(|(called, edges)| CallableFlowNode {
                    calls: if called {
                        vec![CallEvent {
                            parameter: 0,
                            kind: CallableKind::FnMut,
                        }]
                    } else {
                        vec![]
                    },
                    next: if edges.is_empty() {
                        CallableFlowNext::Exit(CallableExit::Return)
                    } else {
                        CallableFlowNext::Edges(edges)
                    },
                })
                .collect(),
        });
    check(m, p, Some("E-VERIFY-RESOURCE"));
}
#[test]
fn local_body_does_not_create_a_parameter_event_or_erase_unused_responsibility() {
    let (mut m, p) = fixture::local_body_wrapper(true);
    let CallableTraces::Flow(flow) = &mut m.functions[4].callable_contract.as_mut().unwrap().traces
    else {
        unreachable!()
    };
    flow.nodes[2].calls.push(CallEvent {
        parameter: 0,
        kind: CallableKind::FnOnce,
    });
    check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
    let (mut m, mut p) = fixture::local_body_wrapper(false);
    m.functions[4].values.push(Type::Closure(ClosureId(1)));
    m.functions[4].blocks[0].instructions.insert(
        1,
        fixture::instruction(
            Some(3),
            InstructionKind::MakeClosure {
                closure: ClosureId(1),
                captures: vec![],
            },
        ),
    );
    p.functions[4].holds.push(ValueHold {
        value: ValueId(3),
        roots: vec![],
    });
    check(m, p, Some("E-VERIFY-RESOURCE"));
}
#[test]
fn local_handler_capture_rebuilds_actual_origin_instead_of_the_closure_id() {
    for package in [false, true] {
        let (m, p) = fixture::captured_local_handler(package);
        check(m, p, None);
    }
    let (m, p) = fixture::role_capturing_local_handler();
    check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
}
#[test]
fn local_handler_child_keeps_real_outer_f_event_and_checks_child_role() {
    let (m, p) = fixture::local_child_handler();
    check(m, p, None);
    let (mut m, p) = fixture::local_child_handler();
    let CallableTraces::Finite(paths) =
        &mut m.functions[4].callable_contract.as_mut().unwrap().traces
    else {
        unreachable!()
    };
    paths[0].calls.clear();
    check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
    let (mut m, p) = fixture::local_child_handler();
    m.functions[5].callable_contract.as_mut().unwrap().roles[0].kind = CallableKind::Fn;
    check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
}
#[test]
fn factory_handler_child_uses_only_checked_fresh_actual_result_without_with() {
    let (m, p) = fixture::factory_child_handler();
    check(m, p, None);
    let (mut m, p) = fixture::factory_child_handler();
    m.functions[6]
        .callable_return_contract
        .as_mut()
        .unwrap()
        .source = CallableReturnSource::Parameter(0);
    check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
}
#[test]
fn local_handler_cannot_hide_supplied_roles_in_unopened_aggregate_captures() {
    for union in [false, true] {
        let (m, p) = fixture::wrapped_role_capture(union);
        check_category(m, p, Some("E-UNSUPPORTED"), ResultCategory::Unsupported);
    }
}
#[test]
fn with_local_body_cannot_hide_a_supplied_fn_once_role() {
    for twice in [false, true] {
        let (m, p) = fixture::supplied_body_hidden_in_with(twice);
        check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
    }
}
#[test]
fn managed_package_and_fresh_factory_cannot_launder_a_supplied_role() {
    // Only the callback field's actual value changes; nominal type, real
    // Managed preparation, factory contract and double-call body all match.
    let (m, p) = fixture::role_hidden_through_factory(false);
    check(m, p, None);
    let (m, p) = fixture::role_hidden_through_factory(true);
    check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
}
#[test]
fn wrapper_keeps_original_kind_and_rejects_forged_trace() {
    let (mut m, p) = fixture::wrapper();
    m.functions[4].callable_contract.as_mut().unwrap().roles[0].kind = CallableKind::FnOnce;
    check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
    let (mut m, p) = fixture::wrapper();
    let CallableTraces::Flow(flow) = &mut m.functions[4].callable_contract.as_mut().unwrap().traces
    else {
        unreachable!()
    };
    flow.nodes[2].calls.clear();
    check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
}
#[test]
fn declared_fn_and_fnmut_body_keep_actual_once_consumption() {
    for kind in [CallableKind::Fn, CallableKind::FnMut] {
        let (mut m, p) = fixture::wrapper();
        m.closures[1].kind = kind;
        m.functions[4].callable_contract.as_mut().unwrap().roles[1].kind = kind;
        check(m, p, None);
    }
    let (mut m, p) = fixture::wrapper();
    m.closures[0].kind = CallableKind::Fn;
    m.functions[4].callable_contract.as_mut().unwrap().roles[0].kind = CallableKind::Fn;
    check(m, p, None);
}
#[test]
fn preparation_cannot_read_or_move_the_exclusively_held_callback() {
    for moving in [false, true] {
        let (mut m, mut p) = fixture::fixture();
        m.functions[4].values.push(Type::Closure(ClosureId(0)));
        m.functions[4].blocks[0].instructions.insert(
            4,
            fixture::instruction(
                Some(4),
                if moving {
                    InstructionKind::TakeLocal { local: LocalId(0) }
                } else {
                    InstructionKind::Load(LocalId(0))
                },
            ),
        );
        p.functions[4].holds.push(ValueHold {
            value: ValueId(4),
            roots: vec![],
        });
        if moving {
            p.functions[4].blocks[0].initialized_out.clear();
        }
        check(m, p, Some("E-BORROW-CONFLICT"));
    }
    let (mut m, mut p) = fixture::fixture();
    m.functions[4].values.push(Type::Closure(ClosureId(0)));
    m.functions[4].blocks[0].instructions.insert(
        3,
        fixture::instruction(Some(4), InstructionKind::Load(LocalId(0))),
    );
    p.functions[4].holds.push(ValueHold {
        value: ValueId(4),
        roots: vec![],
    });
    check(m, p, None);
}
#[test]
fn scoped_raw_and_wire_work_have_exact_fresh_boundaries() {
    for (name, (m, p)) in [
        ("direct", fixture::fixture()),
        ("borrowed", fixture::borrowed_scalar_body()),
        ("wrapper", fixture::wrapper()),
        ("local_body_wrapper", fixture::local_body_wrapper(true)),
    ] {
        exact_work_boundary(name, m, p);
    }
}

#[test]
fn local_owned_preparation_raw_wire_work_has_exact_fresh_boundaries() {
    for (name, (m, p)) in [
        ("local_managed", fixture::local_owned_body(false)),
        ("local_package", fixture::local_owned_body(true)),
        ("ordinary_local_owner", fixture::ordinary_local_owner()),
    ] {
        exact_work_boundary(name, m, p);
    }
}
#[test]
fn factory_body_raw_wire_work_has_exact_fresh_boundaries() {
    for (name, (m, p)) in [
        ("factory_managed_body", fixture::factory_body(false)),
        ("factory_package_body", fixture::factory_body(true)),
    ] {
        exact_work_boundary(name, m, p);
    }
}
#[test]
fn local_handler_raw_wire_work_has_exact_fresh_boundaries() {
    for (name, (m, p)) in [
        (
            "captured_local_handler",
            fixture::captured_local_handler(true),
        ),
        ("local_child_handler", fixture::local_child_handler()),
        ("factory_child_handler", fixture::factory_child_handler()),
        (
            "wrapped_fresh_factory",
            fixture::role_hidden_through_factory(false),
        ),
    ] {
        exact_work_boundary(name, m, p);
    }
}
fn exact_work_boundary(name: &str, m: TypedModule, mut p: CandidatePlan) {
    p.module_fingerprint = m.fingerprint();
    let bytes = wire::encode_module_candidate(&m, &p, &Limits::default()).unwrap();
    for wire in [false, true] {
        let run = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..Limits::default()
            };
            if wire {
                zil_verify::verify_module_bytes(&bytes, &m.target, &limits)
            } else {
                zil_verify::verify_module(m.clone(), p.clone(), &m.target, &limits)
            }
        };
        let (mut low, mut high) = (0, Limits::default().max_analysis_steps);
        let expected = run(high).unwrap().fingerprint();
        while low + 1 < high {
            let mid = low + (high - low) / 2;
            match run(mid) {
                Ok(v) => {
                    assert_eq!(v.fingerprint(), expected);
                    high = mid
                }
                Err(e) => {
                    assert!(
                        e.iter()
                            .all(|d| d.category == ResultCategory::ResourceLimit),
                        "{e:?}"
                    );
                    low = mid
                }
            }
        }
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        assert!(
            run(high - 1)
                .unwrap_err()
                .iter()
                .all(|d| d.category == ResultCategory::ResourceLimit)
        );
        assert_eq!(run(high).unwrap().fingerprint(), expected);
        println!("scoped {name} wire={wire} N={high}; N-1=ResourceLimit; fresh N=Pass");
    }
}
#[test]
fn copy_actual_body_cannot_repeat_or_cycle_its_symbolic_consumption() {
    for cycle in [false, true] {
        let (mut m, mut p) = fixture::wrapper();
        m.closures[1].kind = CallableKind::Fn;
        m.functions[4].callable_contract.as_mut().unwrap().roles[1].kind = CallableKind::Fn;
        if cycle {
            let tail = m.functions[4].blocks[0].instructions.split_off(2);
            m.functions[4].blocks[0].terminator = Terminator::Jump(BlockId(1));
            m.functions[4].blocks.push(BasicBlock {
                id: BlockId(1),
                instructions: tail,
                terminator: Terminator::Jump(BlockId(1)),
            });
            p.functions[4].blocks.push(BlockPlan {
                block: BlockId(1),
                initialized_in: vec![LocalId(0)],
                initialized_out: vec![LocalId(0)],
            });
        } else {
            m.functions[4]
                .values
                .push(Type::Tuple(vec![Type::Bool, Type::I32]));
            let mut second = m.functions[4].blocks[0].instructions[3].clone();
            second.result = Some(ValueId(3));
            m.functions[4].blocks[0].instructions.insert(4, second);
            p.functions[4].holds.push(ValueHold {
                value: ValueId(3),
                roots: vec![],
            });
        }
        check(m, p, Some("E-VERIFY-CALLABLE-CONTRACT"));
    }
}
