//! Source-derived mutations retain a fresh fingerprint, so rejection must come
//! from independent complete-type and actual instruction checks.
use super::super::{candidate, directory};
use std::fs;
use zil_base::{Limits, Phase, ResultCategory, TargetProfile};
use zil_ir::{CandidatePlan, InstructionKind, Type, TypedModule};

fn rejected(label: &str, module: TypedModule, mut plan: CandidatePlan) {
    let limits = Limits::default();
    let analyze = zil_analyze::analyze(&module, &limits).expect_err(label);
    assert!(
        analyze.iter().any(|d| d.phase == Phase::Analyze),
        "{label}: {analyze:?}"
    );
    plan.module_fingerprint = module.fingerprint();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let target = TargetProfile::host_llvm();
    let dir = directory(label);
    for (name, result) in [
        (
            "raw",
            zil_verify::verify_module(module, plan, &target, &limits),
        ),
        (
            "wire",
            zil_verify::verify_module_bytes(&bytes, &target, &limits),
        ),
    ] {
        let diagnostics = result.expect_err(label);
        assert!(
            diagnostics.iter().any(|d| d.phase == Phase::Verify
                && (matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                ) || (label == "pod-unused-definition-5"
                    && d.category == ResultCategory::ResourceLimit))),
            "{label}/{name}: {diagnostics:?}"
        );
        fs::write(dir.join(format!("{name}.txt")), format!("{diagnostics:#?}")).unwrap();
    }
}

#[test]
fn pod_source_derived_unused_definitions_and_operations_reject_forged_types() {
    let source = "struct Pod{x:i64;y:i64}fn main(){let items:Store<Pod>=store_new();store_reserve(items,1);store_push(items,Pod{x:42,y:73})}";
    let (original, plan) = candidate(source);
    for variant in 0..6 {
        let mut module = original.clone();
        let resource = module
            .resources
            .iter_mut()
            .find(|r| matches!(r.payload, Type::OwnedStore { .. }))
            .unwrap();
        let Type::OwnedStore { element } = &mut resource.payload else {
            unreachable!()
        };
        let Type::Record { name, fields } = element.as_mut() else {
            unreachable!()
        };
        match variant {
            0 => name.push_str("_counterfeit"),
            1 => fields[1].0 = fields[0].0.clone(),
            2 => {
                fields[1].1 = Type::Array {
                    element: Box::new(Type::Address {
                        mutable: false,
                        inner: Box::new(Type::I32),
                    }),
                    length: 0,
                }
            }
            3 => fields[1].1 = Type::Managed(Box::new(Type::ByteBuffer)),
            4 => {
                for (_, ty) in fields {
                    *ty = Type::Unit;
                }
            }
            5 => {
                fields[1].1 = Type::Array {
                    element: Box::new(Type::Array {
                        element: Box::new(Type::I64),
                        length: u32::MAX,
                    }),
                    length: u32::MAX,
                }
            }
            _ => unreachable!(),
        }
        rejected(
            &format!("pod-unused-definition-{variant}"),
            module,
            plan.clone(),
        );
    }
    let mut module = original.clone();
    let element = module
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .flat_map(|b| &mut b.instructions)
        .find_map(|i| match &mut i.kind {
            InstructionKind::OwnedStoreNew { element } => Some(element),
            _ => None,
        })
        .unwrap();
    let Type::Record { fields, .. } = element else {
        unreachable!()
    };
    fields[0].0.push_str("_same_layout");
    rejected("pod-operation-full-identity", module, plan);
}
