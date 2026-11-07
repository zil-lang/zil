//! Source-derived mutations retain a fresh fingerprint, so rejection must come
//! from independent complete-type and actual instruction checks.
use super::super::{candidate, directory};
use std::fs;
use zil_base::{Limits, Phase, ResultCategory, TargetProfile};
use zil_ir::{CandidatePlan, InstructionKind, Type, TypedModule};

fn rejected(label: &str, module: TypedModule, mut plan: CandidatePlan) {
    let limits = Limits::default();
    // Keep the producer outcome separate: raw and Wire kernel checks execute
    // even when Analyze itself has failed to reject the forged scope.
    let analyze = zil_analyze::analyze(&module, &limits);
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
    let analyze = analyze.expect_err(label);
    assert!(
        analyze.iter().any(|d| d.phase == Phase::Analyze),
        "{label}: {analyze:?}"
    );
}

fn admitted_unused(label: &str, module: TypedModule) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut plan = zil_analyze::analyze(&module, &limits).expect(label);
    plan.module_fingerprint = module.fingerprint();
    let wire = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    zil_verify::verify_module(module, plan, &target, &limits).expect(label);
    zil_verify::verify_module_bytes(&wire, &target, &limits).expect(label);
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

#[test]
fn pod_source_derived_unused_user_resource_cannot_hide_an_owning_store() {
    let source = "struct Pod{x:i64;y:i64}fn main(){let items:Store<Pod>=store_new();store_reserve(items,1);store_push(items,Pod{x:42,y:73})}";
    let (mut module, plan) = candidate(source);
    let store = module
        .resources
        .iter()
        .find(|definition| matches!(definition.payload, Type::OwnedStore { .. }))
        .unwrap()
        .payload
        .clone();
    // The genuine source-derived Store and canonical definition remain valid.
    // This unused user resource must independently retain its closed Copy
    // payload bound; POD admission cannot add ownership to ordinary Records.
    module.resources.push(zil_ir::ResourceDefinition {
        name: "@module0:Hidden".into(),
        payload: Type::Record {
            name: "@module0:Hidden".into(),
            fields: vec![("store".into(), Type::Managed(Box::new(store)))],
        },
        teardown: None,
    });
    rejected("pod-unused-resource-hidden-owner", module, plan);
}

#[test]
fn pod_llvm_requires_the_exact_verified_resource_and_protocol() {
    let (module, plan) = candidate(
        "struct Pod{x:i64;y:i64}fn main(){let items:Store<Pod>=store_new();store_reserve(items,1);store_push(items,Pod{x:42,y:73});let view=store_at(items,0);let value=view.x;store_pop(items)}",
    );
    let target = module.target.clone();
    let verified = zil_verify::verify_module(module, plan, &target, &Limits::default()).unwrap();
    let original = zil_lower::lower(&verified).unwrap();
    zil_backend_llvm::emit(&verified, &original).unwrap();
    for variant in 0..4 {
        let mut lir = original.clone();
        if variant == 0 {
            let resource = lir
                .resources
                .iter_mut()
                .find(|r| matches!(r.payload, Type::OwnedStore { .. }))
                .unwrap();
            let Type::OwnedStore { element } = &mut resource.payload else {
                unreachable!()
            };
            let Type::Record { fields, .. } = element.as_mut() else {
                unreachable!()
            };
            fields[0].0.push_str("_forged");
        } else {
            let f = lir.functions.iter_mut().find(|f| f.name == "main").unwrap();
            let instruction = f
                .blocks
                .iter_mut()
                .flat_map(|b| &mut b.instructions)
                .find(|i| match variant {
                    1 => matches!(i.kind, InstructionKind::OwnedStoreNew { .. }),
                    2 => matches!(i.kind, InstructionKind::OwnedStorePush { .. }),
                    3 => matches!(i.kind, InstructionKind::OwnedStoreBorrow { .. }),
                    _ => false,
                })
                .unwrap();
            match (&mut instruction.kind, variant) {
                (InstructionKind::OwnedStoreNew { element }, 1) => {
                    let Type::Record { fields, .. } = element else {
                        unreachable!()
                    };
                    fields[0].0.push_str("_forged");
                }
                (_, 2) => f.values[instruction.result.unwrap().0 as usize] = Type::Unit,
                (InstructionKind::OwnedStoreBorrow { store, index }, 3) => {
                    instruction.kind = InstructionKind::OwnedStoreBorrowMut {
                        store: *store,
                        index: *index,
                    }
                }
                _ => unreachable!(),
            }
        }
        let errors = zil_backend_llvm::emit(&verified, &lir).unwrap_err();
        assert!(
            errors.iter().any(|d| d.code == "E-CODEGEN-UNBOUND-LIR"),
            "variant{variant}: {errors:?}"
        );
    }
}

#[test]
fn pod_source_derived_unused_ordinary_owners_keep_container_boundaries() {
    let source = "struct Pod{x:i64;y:i64}fn main(){let items:Store<Pod>=store_new();store_reserve(items,1);store_push(items,Pod{x:42,y:73})}";
    let (original, plan) = candidate(source);
    let store = original
        .resources
        .iter()
        .find(|definition| matches!(definition.payload, Type::OwnedStore { .. }))
        .unwrap()
        .payload
        .clone();
    let head = Type::Managed(Box::new(store.clone()));
    let bag = Type::Package {
        name: "@module0:Bag".into(),
        fields: vec![("items".into(), head.clone())],
    };
    let product = Type::Package {
        name: "@module0:Product".into(),
        fields: vec![("child".into(), bag.clone())],
    };
    let mut forged = store;
    let Type::OwnedStore { element } = &mut forged else {
        unreachable!()
    };
    let Type::Record { name, .. } = element.as_mut() else {
        unreachable!()
    };
    name.push_str("_forged");
    for (label, ty, allowed) in [
        ("pod-unused-package-owning-store", bag.clone(), true),
        ("pod-unused-product-owning-store", product, true),
        (
            "pod-unused-tuple-owning-store",
            Type::Tuple(vec![head.clone()]),
            true,
        ),
        (
            "pod-unused-enum-owning-store",
            Type::Enum {
                name: "@module0:Message".into(),
                variants: vec![("Own".into(), vec![head.clone()]), ("Empty".into(), vec![])],
            },
            true,
        ),
        (
            "pod-unused-error-owning-store",
            Type::ErrorUnion {
                error: Box::new(Type::Unit),
                result: Box::new(head.clone()),
            },
            true,
        ),
        (
            "pod-unused-owning-array",
            Type::Array {
                element: Box::new(bag),
                length: 1,
            },
            false,
        ),
        (
            "pod-unused-record-with-owner",
            Type::Record {
                name: "@module0:NotAnOwner".into(),
                fields: vec![("items".into(), head)],
            },
            false,
        ),
        (
            "pod-unused-forged-store-identity",
            Type::Managed(Box::new(forged)),
            false,
        ),
    ] {
        let mut module = original.clone();
        let main = module
            .functions
            .iter_mut()
            .find(|f| f.name == "main")
            .unwrap();
        // An unused local has neither storage events nor initialized debts.
        // Check structural admission independently of runtime cleanup.
        main.locals.push(zil_ir::LocalDecl {
            id: zil_ir::LocalId(main.locals.len() as u32),
            name: "unused_owning_container".into(),
            ty,
            mutable: false,
            span: main.span,
        });
        if allowed {
            admitted_unused(label, module);
        } else {
            rejected(label, module, plan.clone());
        }
    }
}
