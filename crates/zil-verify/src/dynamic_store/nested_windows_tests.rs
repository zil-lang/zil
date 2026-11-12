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

#[test]
fn actual_nested_manager_effects_keep_exact_fresh_raw_wire_budgets() {
    let (module, mut plan) = request(Scenario {
        mutable_parent: true,
        reserve_before: true,
        through_helper: true,
        ..Scenario::default()
    });
    plan.module_fingerprint = module.fingerprint();
    let wire=zil_ir::wire::encode_module_candidate(&module,&plan,&Limits::default()).unwrap();
    for encoded in [false,true] {
        let run=|n| {
            let limits=Limits {max_analysis_steps:n,..Limits::default()};
            if encoded {verify_module_bytes(&wire,&module.target,&limits)}
            else {verify_module(module.clone(),plan.clone(),&module.target,&limits)}
        };
        let (mut low,mut high)=(0,Limits::default().max_analysis_steps);
        run(high).unwrap();
        while low+1<high {
            let mid=low+(high-low)/2;
            match run(mid) {
                Ok(_)=>high=mid,
                Err(errors)=>{
                    assert!(errors.iter().all(|e|e.category==zil_base::ResultCategory::ResourceLimit),"{errors:?}");
                    low=mid;
                }
            }
        }
        run(high).unwrap();
        assert!(run(high-1).unwrap_err().iter().all(|e|e.category==zil_base::ResultCategory::ResourceLimit));
        run(high).unwrap();
        eprintln!("V complete two-manager helper {} N={high}; N-1/fresh checked",if encoded {"Wire"} else {"raw"});
    }
}
