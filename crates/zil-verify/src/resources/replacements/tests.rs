use super::*;
use crate::place_replacements::fixture;
fn graphs(module: &TypedModule) -> Vec<FunctionGraph> {
    module
        .functions
        .iter()
        .map(|_| FunctionGraph {
            successors: vec![vec![]],
            predecessors: vec![vec![]],
            reachable: vec![true],
            package_views: Default::default(),
            role_data_uses: vec![],
        })
        .collect()
}
#[test]
fn independent_commit_debt_and_content_keep_incoming_distinct_from_fresh_through_two_calls() {
    for fresh in [false, true] {
        let module = fixture::module_with_relay_and_caller(fresh);
        let graphs = graphs(&module);
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        budget.replacements = true;
        budget.sums = true;
        budget.head_descriptions = true;
        budget.function_count = module.functions.len();
        budget.sum_functions = vec![true; module.functions.len()];
        let result = check_module(&module, &graphs, &mut budget);
        if fresh {
            result.unwrap();
        } else {
            assert!(
                result
                    .unwrap_err()
                    .message
                    .contains("create result is not independently proven fresh")
            );
        }
    }
}
#[test]
fn independent_reserve_does_not_trust_a_forged_mutable_ssa_parent() {
    let mut module = fixture::module();
    let Type::Address { mutable, .. } = &mut module.functions[0].locals[0].ty else {
        unreachable!()
    };
    *mutable = false;
    let graphs = graphs(&module);
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    budget.replacements = true;
    budget.sums = true;
    budget.function_count = module.functions.len();
    let error = sums::check_module(&module, &graphs, &mut budget)
        .err()
        .unwrap();
    assert!(
        error.message.contains("shared actual source") || error.message.contains("actual mutable"),
        "{error:?}"
    );
}

#[test]
fn replacement_nested_descriptor_boundary_checks_unused_address_and_value_edges() {
    // Component representation test, not a source-level acceptance witness.
    // Source nested transport is still independently closed by Analyze.
    for address in [false, true] {
        for description in [false, true] {
            let mut module = fixture::module();
            let captured = if description {
                Type::Desc(Box::new(module.resources[0].payload.clone()))
            } else {
                Type::I32
            };
            module.closures = vec![ClosureDefinition {
                storage_life: None,
                lending_captures: vec![],
                id: ClosureId(0),
                function: FunctionId(0),
                kind: CallableKind::Fn,
                captures: vec![captured],
                parameters: vec![],
                result: Type::I32,
            }];
            let inner = Type::Closure(ClosureId(0));
            let edge = if address {
                Type::Address {
                    mutable: false,
                    inner: Box::new(inner),
                }
            } else {
                inner
            };
            module.closures.push(ClosureDefinition {
                storage_life: None,
                lending_captures: vec![],
                id: ClosureId(1),
                function: FunctionId(0),
                kind: CallableKind::Fn,
                captures: vec![edge],
                parameters: vec![],
                result: Type::I32,
            });
            let limits = Limits::default();
            let result = nested_descriptor_boundary(&module, &mut Budget::new(&limits));
            if description {
                assert!(
                    result
                        .unwrap_err()
                        .message
                        .contains("transitive carrier bridge")
                );
            } else {
                result.unwrap();
            }
            let low = Limits {
                max_analysis_steps: 0,
                ..limits
            };
            assert_eq!(
                nested_descriptor_boundary(&module, &mut Budget::new(&low))
                    .unwrap_err()
                    .category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
}

// Raw component only: Source acceptance and actual loan/pin authorization are
// separately exercised by the real producer/Driver controls.
fn whole_package(fresh: bool) -> TypedModule {
    let mut module = fixture::module_with_relay_and_caller(fresh);
    let mut caller = module.functions.pop().unwrap();
    caller.id = FunctionId(0);
    let package = caller.locals[1].ty.clone();
    caller.values.push(package);
    let at = caller.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    caller.blocks[0].instructions.splice(
        at..=at,
        [
            fixture::instruction(
                None,
                InstructionKind::ReservePlaceAddress {
                    reservation: ReservationId(71),
                    address: ValueId(3),
                    path: vec![],
                },
            ),
            fixture::instruction(
                Some(ValueId(6)),
                InstructionKind::MakePackage {
                    fields: vec![ValueId(4), ValueId(0)],
                },
            ),
            fixture::instruction(
                None,
                InstructionKind::CommitPlaceReplace {
                    reservation: ReservationId(71),
                    value: ValueId(6),
                },
            ),
        ],
    );
    module.functions = vec![caller].into();
    module
}
fn whole_check(module: &TypedModule) -> Check<()> {
    let graphs = graphs(module);
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    budget.replacements = true;
    budget.sums = true;
    budget.head_descriptions = true;
    budget.function_count = module.functions.len();
    budget.sum_functions = vec![true; module.functions.len()];
    check_module(module, &graphs, &mut budget)
}

// Independent resource/Content component witnesses. The separately preserved
// .zil files and Driver review exercise real Source formation and public Wire.
fn whole_helper(fresh: bool) -> TypedModule {
    let mut module = fixture::module_with_relay_and_caller(fresh);
    let mut caller = module.functions.pop().unwrap();
    let mut helper = module.functions.remove(0);
    let package = caller.locals[1].ty.clone();
    helper.values.extend([Type::I32, package]);
    helper.blocks[0].instructions = vec![
        fixture::instruction(Some(ValueId(0)), InstructionKind::Load(LocalId(0))),
        fixture::instruction(
            None,
            InstructionKind::ReservePlaceAddress {
                reservation: ReservationId(71),
                address: ValueId(0),
                path: vec![],
            },
        ),
        fixture::instruction(
            Some(ValueId(2)),
            InstructionKind::Constant(Constant::I32(66)),
        ),
        fixture::instruction(
            Some(ValueId(1)),
            if fresh {
                InstructionKind::MakeManaged {
                    fields: vec![ValueId(2)],
                }
            } else {
                InstructionKind::TakeLocal { local: LocalId(1) }
            },
        ),
        fixture::instruction(
            Some(ValueId(3)),
            InstructionKind::MakePackage {
                fields: vec![ValueId(1), ValueId(2)],
            },
        ),
        fixture::instruction(
            None,
            InstructionKind::CommitPlaceReplace {
                reservation: ReservationId(71),
                value: ValueId(3),
            },
        ),
    ];
    caller.id = FunctionId(1);
    if fresh {
        helper.parameters = vec![LocalId(0)];
        // The destination leaf is incoming, so Fresh requires this helper's
        // real Commit. An already-Fresh destination cannot mask a bad proof.
        caller.parameters = vec![LocalId(0)];
        caller.blocks[0].instructions[1].kind = InstructionKind::TakeLocal { local: LocalId(0) };
        caller.blocks[0].instructions.remove(5);
    }
    let call = caller.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    call.kind = InstructionKind::Call {
        function: FunctionId(0),
        arguments: if fresh {
            vec![ValueId(3)]
        } else {
            vec![ValueId(3), ValueId(4)]
        },
    };
    module.functions = vec![helper, caller].into();
    module
}

#[test]
fn whole_helper_requires_actual_must_commit_and_preserves_rhs_leaf_origin() {
    whole_check(&whole_helper(true)).unwrap();
    let error = whole_check(&whole_helper(false)).unwrap_err();
    assert!(
        error
            .message
            .contains("create result is not independently proven fresh"),
        "{error:?}"
    );
}

#[test]
fn whole_helper_caller_shell_and_old_fields_must_still_be_live() {
    let mut module = whole_helper(true);
    let caller = &mut module.functions[1];
    let at = caller.blocks[0]
        .instructions
        .iter()
        .position(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap();
    caller.blocks[0].instructions.insert(
        at,
        fixture::instruction(
            None,
            InstructionKind::DropPackageField {
                local: LocalId(1),
                index: 0,
            },
        ),
    );
    let error = whole_check(&module).unwrap_err();
    assert!(
        error.message.contains("every old field responsibility"),
        "{error:?}"
    );
}

#[test]
fn whole_helper_broader_or_conditional_cfg_does_not_export_fresh() {
    let mut module = whole_helper(true);
    let helper = &mut module.functions[0];
    let tail = helper.blocks[0].instructions.split_off(0);
    helper.values.push(Type::Bool);
    helper.blocks[0].instructions.push(fixture::instruction(
        Some(ValueId(4)),
        InstructionKind::Constant(Constant::Bool(false)),
    ));
    helper.blocks[0].terminator = Terminator::Branch {
        condition: ValueId(4),
        then_block: BlockId(1),
        else_block: BlockId(2),
    };
    helper.blocks.push(BasicBlock {
        id: BlockId(1),
        instructions: tail,
        terminator: Terminator::Return(None),
    });
    helper.blocks.push(BasicBlock {
        id: BlockId(2),
        instructions: vec![],
        terminator: Terminator::Return(None),
    });
    let mut graphs = graphs(&module);
    graphs[0].successors = vec![vec![1, 2], vec![], vec![]];
    graphs[0].predecessors = vec![vec![], vec![0], vec![0]];
    graphs[0].reachable = vec![true; 3];
    let limits = Limits::default();
    let mut budget = Budget::new(&limits);
    budget.replacements = true;
    budget.sums = true;
    budget.head_descriptions = true;
    budget.function_count = module.functions.len();
    budget.sum_functions = vec![true; module.functions.len()];
    let error = check_module(&module, &graphs, &mut budget).unwrap_err();
    assert!(
        error
            .message
            .contains("create result is not independently proven fresh"),
        "{error:?}"
    );
}

fn whole_helper_copy_return(error_union: bool) -> TypedModule {
    let mut module = whole_helper(true);
    let helper = &mut module.functions[0];
    if error_union {
        let error = Type::Enum {
            name: "Fault".into(),
            variants: vec![("Bad".into(), vec![])],
        };
        helper.return_type = Type::ErrorUnion {
            error: Box::new(error.clone()),
            result: Box::new(Type::I32),
        };
        helper.values.extend([error, helper.return_type.clone()]);
        helper.blocks[0].instructions.extend([
            fixture::instruction(
                Some(ValueId(4)),
                InstructionKind::MakeVariant {
                    variant: 0,
                    fields: vec![],
                },
            ),
            fixture::instruction(
                Some(ValueId(5)),
                InstructionKind::MakeErrorUnion {
                    is_error: false,
                    value: ValueId(2),
                },
            ),
        ]);
        helper.blocks[0].terminator = Terminator::Return(Some(ValueId(5)));
    } else {
        helper.return_type = Type::I32;
        helper.blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    }
    let returned = helper.return_type.clone();
    let caller = &mut module.functions[1];
    caller.result_mode = PassingMode::Give;
    let output = ValueId(caller.values.len() as u32);
    caller.values.push(returned);
    caller.blocks[0]
        .instructions
        .iter_mut()
        .find(|i| matches!(i.kind, InstructionKind::Call { .. }))
        .unwrap()
        .result = Some(output);
    module
}

#[test]
fn nonunit_whole_helper_actual_commit_preserves_access_without_must_fresh() {
    for error_union in [false, true] {
        let mut module = whole_helper_copy_return(error_union);
        whole_check(&module).unwrap();
        module.functions[1].result_mode = PassingMode::Create;
        let error = whole_check(&module).unwrap_err();
        assert!(
            error
                .message
                .contains("create result is not independently proven fresh"),
            "{error:?}"
        );
    }
}

#[test]
fn error_whole_helper_abort_retains_old_debt_and_cleans_only_prepared_temporary() {
    let mut module = whole_helper_copy_return(true);
    let helper = &mut module.functions[0];
    // A real prepared resource exists before failure. It is cleaned on Abort;
    // no Package is published and no old destination debt can be consumed.
    helper.blocks[0].instructions.truncate(4);
    helper.blocks[0].instructions.extend([
        fixture::instruction(
            None,
            InstructionKind::Store {
                local: LocalId(1),
                value: ValueId(1),
            },
        ),
        fixture::instruction(None, InstructionKind::DropManaged { local: LocalId(1) }),
        fixture::instruction(
            None,
            InstructionKind::AbortPlaceReservation {
                reservation: ReservationId(71),
            },
        ),
        fixture::instruction(
            Some(ValueId(4)),
            InstructionKind::MakeVariant {
                variant: 0,
                fields: vec![],
            },
        ),
        fixture::instruction(
            Some(ValueId(5)),
            InstructionKind::MakeErrorUnion {
                is_error: true,
                value: ValueId(4),
            },
        ),
    ]);
    whole_check(&module).unwrap();
    let mut forged_create = module.clone();
    forged_create.functions[1].result_mode = PassingMode::Create;
    assert!(
        whole_check(&forged_create)
            .unwrap_err()
            .message
            .contains("fresh")
    );
    for omit_abort in [false, true] {
        let mut bad = module.clone();
        bad.functions[0].blocks[0].instructions.retain(|i| {
            if omit_abort {
                !matches!(i.kind, InstructionKind::AbortPlaceReservation { .. })
            } else {
                !matches!(i.kind, InstructionKind::DropManaged { .. })
            }
        });
        let error = whole_check(&bad).unwrap_err();
        assert_eq!(
            error.code,
            if omit_abort {
                "E-VERIFY-PLACE-REPLACE"
            } else {
                "E-VERIFY-RESOURCE"
            },
            "{error:?}"
        );
    }
}
#[test]
fn whole_package_transfers_actual_field_provenance_without_making_shell_fresh() {
    for fresh in [true, false] {
        let result = whole_check(&whole_package(fresh));
        if fresh {
            result.unwrap();
        } else {
            let error = result.unwrap_err();
            assert!(
                error
                    .message
                    .contains("create result is not independently proven fresh"),
                "{error:?}"
            );
        }
    }
}
#[test]
fn whole_package_actual_fresh_rhs_replaces_incoming_old_instance() {
    let mut module = whole_package(true);
    let f = &mut module.functions[0];
    f.parameters = vec![LocalId(0)];
    f.blocks[0].instructions[1].kind = InstructionKind::TakeLocal { local: LocalId(0) };
    whole_check(&module).unwrap();
}
#[test]
fn whole_package_needs_its_original_live_field_and_consumes_rhs_once() {
    for partial in [true, false] {
        let mut module = whole_package(true);
        let f = &mut module.functions[0];
        let at = f.blocks[0]
            .instructions
            .iter()
            .position(|i| matches!(i.kind, InstructionKind::CommitPlaceReplace { .. }))
            .unwrap();
        if partial {
            f.blocks[0].instructions.insert(
                at,
                fixture::instruction(
                    None,
                    InstructionKind::DropPackageField {
                        local: LocalId(1),
                        index: 0,
                    },
                ),
            );
        } else {
            f.values.push(f.values[3].clone());
            f.blocks[0].instructions.splice(
                at + 1..at + 1,
                [
                    fixture::instruction(
                        Some(ValueId(7)),
                        InstructionKind::BorrowLocalMut { local: LocalId(1) },
                    ),
                    fixture::instruction(
                        None,
                        InstructionKind::ReservePlaceAddress {
                            reservation: ReservationId(72),
                            address: ValueId(7),
                            path: vec![],
                        },
                    ),
                    fixture::instruction(
                        None,
                        InstructionKind::CommitPlaceReplace {
                            reservation: ReservationId(72),
                            value: ValueId(6),
                        },
                    ),
                ],
            );
        }
        let error = whole_check(&module).unwrap_err();
        assert!(
            error.message.contains(if partial {
                "old field live"
            } else {
                "consumed before Commit"
            }),
            "{error:?}"
        );
    }
}
