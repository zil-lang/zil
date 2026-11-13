//! Actual Source reached Types with an incomplete environment proposal. It is
//! untrusted empty-plan input, never an Analyze or Verify success fixture.
use zil_base::Limits;

#[test]
fn actual_nonzero_context_capture_scope_is_neither_unknown_nor_forged_zero() {
    let limits = Limits::default();
    let bytes = include_bytes!("fixtures/dyn-context/context-capture-unknown-scope.typed.wire");
    let (module, mut plan) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
    // Supply only the inventory required to reach signature authentication.
    // This remains an untrusted skeletal plan: it grants no CFG/Hold fact and
    // cannot pass verification. An empty function inventory stops too early
    // to test the real nonzero environment scope obligation below.
    assert!(plan.functions.is_empty());
    plan.functions = module
        .functions
        .iter()
        .map(|function| zil_ir::FunctionPlan {
            function: function.id,
            blocks: vec![],
            holds: vec![],
        })
        .collect();
    assert_eq!(module.closures.len(), 1);
    let definition = &module.closures[0];
    let zil_ir::Type::OwnedDyn { interface } = definition.captures[0] else {
        panic!("real source must capture its owning Context")
    };
    assert_eq!(
        module
            .dyn_interfaces
            .iter()
            .find(|d| d.id == interface)
            .unwrap()
            .retained_life
            .binder_count,
        1
    );
    assert!(definition.storage_life.is_none());
    assert!(
        module.functions[definition.function.0 as usize]
            .storage_life
            .is_none()
    );
    for forged_zero in [false, true] {
        let mut module = module.clone();
        let mut plan = plan.clone();
        if forged_zero {
            let scope = zil_ir::StorageLifeContract {
                fixed_binder_count: 0,
                invocation_binder_count: 0,
                bindings: vec![],
                static_bindings: vec![],
                classes: vec![],
            };
            let entry = module.closures[0].function;
            module.closures[0].storage_life = Some(scope.clone());
            module.functions[entry.0 as usize].storage_life = Some(scope);
        }
        plan.module_fingerprint = module.fingerprint();
        for wire in [false, true] {
            let errors = if wire {
                let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
                zil_verify::verify_module_bytes(&bytes, &module.target, &limits).unwrap_err()
            } else {
                zil_verify::verify_module(module.clone(), plan.clone(), &module.target, &limits)
                    .unwrap_err()
            };
            assert!(
                errors.iter().any(|error| if forged_zero {
                    error.code == "E-VERIFY-SIGNATURE-CLASS"
                } else {
                    error
                        .message
                        .contains("Context capture requires its complete actual environment scope")
                }),
                "forged_zero={forged_zero}, wire={wire}: {errors:?}"
            );
        }
    }
}
