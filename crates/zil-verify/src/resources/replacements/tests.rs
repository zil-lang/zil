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
    module.functions = vec![caller];
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
