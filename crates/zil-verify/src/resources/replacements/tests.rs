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
