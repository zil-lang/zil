use super::*;
use crate::place_replacements::fixture;
use zil_base::ResultCategory;
fn plan(f: &TypedFunction) -> Plan {
    let mut b = Budget::new(&Limits::default());
    b.sum_functions = vec![true; f.id.0 as usize + 1];
    Plan::new(f, &mut b).unwrap().unwrap()
}
#[test]
fn initial_sum_templates_produce_independent_tables_and_cow_trees() {
    let module = fixture::module();
    let f = &module.functions[0];
    let plan = plan(f);
    let mut b = Budget::new(&Limits::default());
    let seed = plan.initial(&module, f, &mut b).unwrap();
    let mut changed = plan.initial(&module, f, &mut b).unwrap();
    edit(
        changed
            .values
            .get_mut(&ValueId(0), &mut b)
            .unwrap()
            .unwrap(),
        &mut b,
    )
    .unwrap()
    .missing = false;
    if let Node::Leaf(birth) = &mut edit(changed.locals.get_mut(&LocalId(1)).unwrap(), &mut b)
        .unwrap()
        .node
    {
        birth.insert(Birth::Created);
    } else {
        panic!("actual Managed parameter is a leaf")
    }
    assert_ne!(seed, changed);
    let again = plan.initial(&module, f, &mut b).unwrap();
    assert_eq!(seed, again);
    assert!(Arc::ptr_eq(
        seed.values.get(&ValueId(1)).unwrap(),
        again.values.get(&ValueId(1)).unwrap()
    ));
}
#[test]
fn values_only_initial_templates_charge_the_actual_ssa_table_copy() {
    let mut module = fixture::module();
    let f = &mut module.functions[0];
    f.locals.clear();
    f.parameters.clear();
    f.values = vec![
        Type::I64,
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(Type::I64),
        },
    ];
    f.blocks[0].instructions = vec![
        fixture::instruction(
            Some(ValueId(0)),
            InstructionKind::Constant(Constant::I64(7)),
        ),
        fixture::instruction(
            Some(ValueId(1)),
            InstructionKind::MakeErrorUnion {
                is_error: false,
                value: ValueId(0),
            },
        ),
    ];
    let plan = plan(f);
    let f = &module.functions[0];
    let seed = plan
        .initial(&module, f, &mut Budget::new(&Limits::default()))
        .unwrap();
    assert!(seed.locals.is_empty() && seed.captures.is_empty() && !seed.values.is_empty());
    let errors = plan
        .initial(
            &module,
            f,
            &mut Budget::new(&Limits {
                max_analysis_steps: 1,
                ..Limits::default()
            }),
        )
        .unwrap_err();
    assert!(
        errors
            .iter()
            .all(|e| e.category == ResultCategory::ResourceLimit)
    );
    assert_eq!(
        seed,
        plan.initial(&module, f, &mut Budget::new(&Limits::default()))
            .unwrap()
    );
}
#[test]
fn selected_union_arms_still_require_an_actual_result_identity() {
    let mut module = fixture::module();
    let f = &mut module.functions[0];
    f.locals.clear();
    f.parameters.clear();
    f.blocks[0].instructions.clear();
    f.values = vec![
        Type::ErrorUnion {
            error: Box::new(Type::I32),
            result: Box::new(Type::I64),
        },
        Type::I64,
    ];
    let plan = plan(f);
    let f = &module.functions[0];
    let tree = Arc::new(Tree {
        missing: false,
        node: Node::Copy,
    });
    for present in [false, true] {
        for result in [Some(ValueId(1)), None, Some(ValueId(99))] {
            let mut keyed = BTreeMap::new();
            keyed.insert(
                ValueId(0),
                Arc::new(Tree {
                    missing: false,
                    node: Node::Sum(if present {
                        BTreeMap::from([(0, vec![tree.clone()])])
                    } else {
                        BTreeMap::new()
                    }),
                }),
            );
            keyed.insert(
                ValueId(1),
                Arc::new(Tree {
                    missing: true,
                    node: Node::Copy,
                }),
            );
            let mut facts = Facts {
                values: ssa_values::Values::from_keyed(
                    2,
                    keyed,
                    &mut Budget::new(&Limits::default()),
                )
                .unwrap(),
                ..Facts::default()
            };
            let i = fixture::instruction(
                result,
                InstructionKind::ReadErrorUnion {
                    value: ValueId(0),
                    is_error: false,
                },
            );
            let answer = plan.apply(
                &module,
                f,
                &i,
                &mut facts,
                true,
                &mut Budget::new(&Limits::default()),
            );
            if result == Some(ValueId(1)) {
                answer.unwrap();
                assert_eq!(facts.values.get(&ValueId(1)), Some(&tree));
            } else {
                let errors = answer.unwrap_err();
                assert!(
                    errors
                        .iter()
                        .any(|d| d.message.contains("union result absent")),
                    "{errors:?}"
                );
            }
        }
    }
}
