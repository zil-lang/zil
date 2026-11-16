//! Independent Verify over complete handwritten public IR manager inputs.
use super::*;
#[allow(dead_code)]
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/nested-store/raw-managers.rs"
    ));
}
use fixture::{Scenario, request};
#[test]
fn two_real_prefixes_preserve_actual_manager_and_parent_permission_raw_wire() {
    for scenario in [
        Scenario::default(),
        Scenario {
            mutable_parent: true,
            reserve_before: true,
            ..Scenario::default()
        },
        Scenario {
            mutable_parent: true,
            mutable_leaf: true,
            ..Scenario::default()
        },
        Scenario {
            mutable_parent: true,
            reserve_before: true,
            through_helper: true,
            ..Scenario::default()
        },
    ] {
        let (module, plan) = request(scenario);
        for result in tests::pod_both(&module, &plan, &Limits::default()) {
            result.unwrap();
        }
        // Descriptions keep their exact manager/epoch in the resource kernel,
        // rather than acquiring the strong Address Hold of that manager. Keep
        // the historical false claims as hostile raw/Wire controls, after the
        // valid candidate has reached the real nested permission checks.
        for function in &module.functions {
            for (value, ty) in function.values.iter().enumerate() {
                if !matches!(ty, Type::Desc(_)) {
                    continue;
                }
                assert!(plan.functions[function.id.0 as usize].holds[value].roots.is_empty());
                let mut wrong = plan.clone();
                wrong.functions[function.id.0 as usize].holds[value].roots =
                    vec![if function.parameters.is_empty() {
                        HoldRoot::Local(LocalId(0))
                    } else {
                        HoldRoot::Parameter(0)
                    }];
                for result in tests::pod_both(&module, &wrong, &Limits::default()) {
                    let errors = result.unwrap_err();
                    assert!(errors.iter().any(|error| error.code == "E-VERIFY-HOLD"), "{errors:?}");
                }
            }
        }
    }
    for scenario in [
        Scenario {
            reserve_before: true,
            ..Scenario::default()
        },
        Scenario {
            mutable_leaf: true,
            ..Scenario::default()
        },
        Scenario {
            mutable_parent: true,
            reserve_inner_live: true,
            ..Scenario::default()
        },
        Scenario {
            reserve_outer_live: true,
            ..Scenario::default()
        },
        Scenario {
            retire_outer_live: true,
            ..Scenario::default()
        },
        Scenario {
            reserve_before: true,
            through_helper: true,
            ..Scenario::default()
        },
    ] {
        let (module, plan) = request(scenario);
        for result in tests::pod_both(&module, &plan, &Limits::default()) {
            let errors = result.unwrap_err();
            assert!(
                errors
                    .iter()
                    .all(|e| e.category == zil_base::ResultCategory::SafeReject),
                "{errors:?}"
            );
        }
    }
}
