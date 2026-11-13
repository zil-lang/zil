use super::*;

fn allowance() -> CtDeclaredBudget {
    CtDeclaredBudget {
        max_steps: 100_000,
        max_call_depth: 128,
        max_live_cells: 100_000,
        max_temporary_bytes: 1_000_000,
        max_generated_nodes: 100_000,
        max_instances: 100_000,
        max_analysis_work: 100_000,
    }
}
fn input() -> CtTypeRef {
    CtTypeRef {
        scoped_catalogue: Some(CtTypeCatalogue {
            stage: CtStageIdentity {
                source_fingerprint: Fingerprint::of(b"source"),
                epoch: 7,
                committed_generation_fingerprint: Fingerprint::of(b"generation"),
                declaration_inventory_fingerprint: Fingerprint::of(b"declarations"),
            },
            // Deliberately unused descriptor storage: transporting the plain
            // root must not erase it or claim its semantic admission.
            types: vec![
                CtDynamicTypeNode::Named("complete".into()),
                CtDynamicTypeNode::Managed(Box::new(CtDynamicTypeId(0))),
            ],
            interfaces: Vec::new(),
        }),
        nodes: vec![CtTypeNode::Primitive(CtScalarTy::I32)],
        root: CtTypeNodeId(0),
        ambient_classes: Vec::new(),
    }
}

#[test]
fn a_single_descriptor_namespace_survives_plain_composition() {
    let original = input();
    let plain = CtTypeRef {
        scoped_catalogue: None,
        nodes: vec![CtTypeNode::Primitive(CtScalarTy::F64)],
        root: CtTypeNodeId(0),
        ambient_classes: Vec::new(),
    };
    let args = [CtValue::Type(original.clone()), CtValue::Type(plain)];
    let mut budget = CtExecutionBudget::new(allowance());
    let result = construct_type(&CtTypeConstructor::Tuple, &args, &mut budget).unwrap();
    assert_eq!(result.scoped_catalogue, original.scoped_catalogue);
    assert_eq!(
        result.nodes,
        vec![
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Primitive(CtScalarTy::F64),
            CtTypeNode::Tuple(vec![CtTypeNodeId(0), CtTypeNodeId(1)])
        ]
    );
    let work = budget.usage().analysis_work;
    let bytes = budget.live_temporary_bytes();
    for _ in 0..2 {
        let mut exact = allowance();
        exact.max_analysis_work = work;
        exact.max_temporary_bytes = bytes;
        assert_eq!(
            construct_type(
                &CtTypeConstructor::Tuple,
                &args,
                &mut CtExecutionBudget::new(exact)
            )
            .unwrap(),
            result
        );
        for (work_limit, bytes_limit) in [(work - 1, bytes), (work, bytes - 1)] {
            exact.max_analysis_work = work_limit;
            exact.max_temporary_bytes = bytes_limit;
            assert!(
                construct_type(
                    &CtTypeConstructor::Tuple,
                    &args,
                    &mut CtExecutionBudget::new(exact)
                )
                .unwrap_err()
                .iter()
                .all(|error| error.category == zil_base::ResultCategory::ResourceLimit)
            );
        }
    }
}

#[test]
fn unsupported_namespace_merges_and_scoped_nodes_are_not_silently_erased() {
    let original = input();
    let pair = [
        CtValue::Type(original.clone()),
        CtValue::Type(original.clone()),
    ];
    assert!(
        construct_type(
            &CtTypeConstructor::Tuple,
            &pair,
            &mut CtExecutionBudget::new(allowance())
        )
        .unwrap_err()
        .iter()
        .all(|error| error.category == zil_base::ResultCategory::Unsupported)
    );
    assert!(
        construct_type(
            &CtTypeConstructor::AnonymousSum,
            &pair[..1],
            &mut CtExecutionBudget::new(allowance())
        )
        .is_err()
    );
    let mut scoped = original;
    scoped.nodes.push(CtTypeNode::ScopedDyn {
        descriptor: CtDynamicTypeId(0),
        context: None,
        classes: Vec::new(),
    });
    scoped.root = CtTypeNodeId(1);
    assert!(
        construct_type(
            &CtTypeConstructor::Managed,
            &[CtValue::Type(scoped)],
            &mut CtExecutionBudget::new(allowance())
        )
        .is_err()
    );
}
