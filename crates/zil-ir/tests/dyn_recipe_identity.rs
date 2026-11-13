//! An independent byte oracle for declaration recipes. This tests raw transport
//! identity only; neither a recipe nor its fingerprint proves an implementation.
use zil_base::{Fingerprint, Limits, ModuleId, NodeId, SCHEMA_VERSION, TargetProfile};
use zil_ir::*;
fn id(node: u32) -> LifeDeclarationId {
    LifeDeclarationId {
        module: ModuleId(0),
        declaration: NodeId(node),
        compiler: false,
    }
}
fn fixture() -> TypedModule {
    TypedModule {
        semantic: None,
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::host_llvm(),
        life_declarations: vec![],
        life_instances: vec![],
        dyn_declarations: vec![DynDeclaration {
            direct_supertraits: None,
            requirements: Some(vec![
                DynObligation::Structural {
                    receiver: DynTypePattern::Parameter(2),
                    bound: DynStructuralBound::Copy,
                },
                DynObligation::TypeEqual {
                    left: DynTypePattern::Parameter(2),
                    right: DynTypePattern::Parameter(2),
                },
                DynObligation::ConstEqual {
                    ty: DynTypePattern::Concrete(Type::Usize),
                    left: DynConstPattern::Value(Constant::Usize(7)),
                    right: DynConstPattern::Parameter(3),
                },
            ]),
            associated_definitions: None,
            method_signatures: None,
            method_definitions: None,
            id: id(9),
            identity: LifeDeclarationIdentity {
                package: "recipe".into(),
                module_path: vec!["m".into()],
                declaration_path: vec!["Impl".into()],
                generation_path: vec![],
            },
            kind: DynDeclarationKind::Impl,
            implemented_trait: Some(DynSelfPattern {
                declaration: id(5),
                arguments: vec![DynPatternActual::Life(
                    DynLifePattern::DeclarationParameter(0),
                )],
            }),
            self_pattern: Some(DynTypePattern::Nominal(Box::new(DynSelfPattern {
                declaration: id(7),
                arguments: vec![
                    DynPatternActual::Life(DynLifePattern::DeclarationParameter(1)),
                    DynPatternActual::Life(DynLifePattern::DeclarationParameter(0)),
                    DynPatternActual::Life(DynLifePattern::DeclarationParameter(1)),
                    DynPatternActual::Type(DynTypePattern::Array {
                        element: Box::new(DynTypePattern::Parameter(2)),
                        length: DynConstPattern::Parameter(3),
                    }),
                    DynPatternActual::Const(DynConstPattern::Value(Constant::Usize(7))),
                ],
            }))),
            parameters: vec![
                LifeParameterKind::Lifetime,
                LifeParameterKind::Lifetime,
                LifeParameterKind::Type,
                LifeParameterKind::Const(Type::Usize),
            ],
            members: vec![DynMemberDeclaration {
                name: "read".into(),
                kind: DynMemberKind::Method(vec![]),
            }],
        }],
        dyn_interfaces: vec![],
        dyn_vtables: vec![],
        functions: vec![],
        closures: vec![],
        resources: vec![],
        unsupported: vec![],
    }
}
fn word(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn small(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn text(out: &mut Vec<u8>, v: &str) {
    word(out, v.len() as u64);
    out.extend_from_slice(v.as_bytes());
}
fn decl(out: &mut Vec<u8>, v: u32) {
    small(out, 0);
    small(out, v);
    out.push(0);
}
fn independent_bytes(m: &TypedModule) -> Vec<u8> {
    let mut out = b"ZIL-TYPED-MODULE-V48\0".to_vec();
    small(&mut out, SCHEMA_VERSION);
    small(&mut out, 0);
    out.extend_from_slice(&[0; 32]);
    text(&mut out, &m.target.to_json());
    word(&mut out, 0);
    word(&mut out, 0); // Complete nominal inventories.
    word(&mut out, 1);
    out.extend_from_slice(&[0, 1]); // No supers, one original structural requirement.
    word(&mut out, 3);
    out.extend_from_slice(&[0, 1]); // Structural(receiver=type formal2).
    small(&mut out, 2);
    out.push(0); // Copy.
    out.extend_from_slice(&[3, 1]); // Complete type equality, left type formal2.
    small(&mut out, 2);
    out.push(1);
    small(&mut out, 2); // Right type formal2.
    out.extend_from_slice(&[4, 0, 27, 0, 10]); // Const equality: usize, literal Usize(7).
    word(&mut out, 7);
    out.push(1);
    small(&mut out, 3); // Right original const formal3.
    out.extend_from_slice(&[0, 0, 0]); // No assoc/method/definition inventories.
    decl(&mut out, 9);
    text(&mut out, "recipe");
    word(&mut out, 1);
    text(&mut out, "m");
    word(&mut out, 1);
    text(&mut out, "Impl");
    word(&mut out, 0);
    out.extend_from_slice(&[1, 1, 2]); // Impl kind; present nominal Self recipe.
    decl(&mut out, 7);
    word(&mut out, 5);
    for formal in [1, 0, 1] {
        out.extend_from_slice(&[2, 0]); // Life actual, declaration formal scope.
        small(&mut out, formal);
    }
    out.extend_from_slice(&[0, 5, 1]);
    small(&mut out, 2); // Type(Array(element=type formal2)).
    out.push(1);
    small(&mut out, 3); // Length is const formal3.
    out.extend_from_slice(&[1, 0, 10]);
    word(&mut out, 7); // Actual const Value(Usize(7)).
    out.push(1); // A complete implemented-trait application recipe.
    decl(&mut out, 5);
    word(&mut out, 1);
    out.extend_from_slice(&[2, 0]);
    small(&mut out, 0);
    word(&mut out, 4);
    out.extend_from_slice(&[2, 2, 0, 1, 27]); // Formal kind/order/domain.
    word(&mut out, 1);
    text(&mut out, "read");
    out.push(2);
    word(&mut out, 0);
    for _ in 0..6 {
        word(&mut out, 0);
    } // Interfaces, tables, functions, closures, resources, features.
    out
}
#[test]
fn recipe_occurrences_and_formal_ordinals_match_independent_identity() {
    let module = fixture();
    assert_eq!(
        Fingerprint::of(&independent_bytes(&module)),
        module.fingerprint()
    );
    let plan = CandidatePlan::empty_for(&module);
    let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    assert_eq!(
        wire::decode_module_candidate(&bytes, &Limits::default())
            .unwrap()
            .0,
        module
    );
    for mutation in 0..11 {
        let mut changed = module.clone();
        let declaration = &mut changed.dyn_declarations[0];
        if mutation == 0 {
            declaration.self_pattern = None;
        } else if mutation >= 9 {
            match &mut declaration.requirements.as_mut().unwrap()[if mutation == 9 { 1 } else { 2 }]
            {
                DynObligation::TypeEqual { right, .. } => *right = DynTypePattern::Parameter(3),
                DynObligation::ConstEqual { ty, .. } => *ty = DynTypePattern::Concrete(Type::U64),
                _ => unreachable!(),
            }
        } else if mutation >= 6 {
            match mutation {
                6 => declaration.implemented_trait = None,
                7 => declaration.implemented_trait.as_mut().unwrap().declaration = id(6),
                _ => {
                    declaration.implemented_trait.as_mut().unwrap().arguments[0] =
                        DynPatternActual::Life(DynLifePattern::DeclarationParameter(1))
                }
            }
        } else {
            let DynTypePattern::Nominal(pattern) = declaration.self_pattern.as_mut().unwrap()
            else {
                unreachable!()
            };
            match mutation {
                1 => pattern.arguments.swap(0, 1),
                2 => {
                    pattern.arguments[2] =
                        DynPatternActual::Life(DynLifePattern::DeclarationParameter(0))
                }
                3 => pattern.declaration.compiler = true,
                4 => pattern.arguments[3] = DynPatternActual::Type(DynTypePattern::Parameter(2)),
                _ => pattern.arguments[4] = DynPatternActual::Const(DynConstPattern::Parameter(3)),
            }
        }
        assert_ne!(changed.fingerprint(), module.fingerprint());
        let encoded = wire::encode_module_candidate(
            &changed,
            &CandidatePlan::empty_for(&changed),
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(
            wire::decode_module_candidate(&encoded, &Limits::default())
                .unwrap()
                .0,
            changed
        );
    }
}
#[test]
fn deeply_nested_recipe_stops_at_a_finite_wire_bound() {
    let mut module = fixture();
    let mut pattern = DynTypePattern::Parameter(2);
    for _ in 0..130 {
        pattern = DynTypePattern::Tuple(vec![pattern]);
    }
    let DynTypePattern::Nominal(root) = module.dyn_declarations[0].self_pattern.as_mut().unwrap()
    else {
        unreachable!()
    };
    root.arguments[3] = DynPatternActual::Type(pattern);
    let limits = Limits {
        max_call_depth: 100_000,
        ..Limits::default()
    };
    let plan = CandidatePlan::empty_for(&module);
    let errors = wire::encode_module_candidate(&module, &plan, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .all(|error| error.category == zil_base::ResultCategory::ResourceLimit)
    );
}

#[test]
fn receiver_after_sources_have_independent_bytes_and_preserve_before_selectors() {
    let mut module = fixture();
    let old = StoragePosition {
        base: StorageBase::Parameter(0),
        path: vec![ContractProjection::RetainedPort(0)],
    };
    let next = StoragePosition {
        base: StorageBase::Parameter(1),
        path: vec![],
    };
    let application = DynTraitApplication {
        declaration: id(7),
        actuals: vec![],
    };
    module.dyn_interfaces.push(DynInterface {
        context: None,
        owning_cleanup: None,
        id: DynInterfaceId {
            module: ModuleId(0),
            index: 0,
        },
        application: application.clone(),
        inherited: vec![],
        associated_types: vec![],
        associated_constants: vec![],
        retained_life: RetainedLifeShape {
            binder_count: 0,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
        slots: vec![DynSlot {
            member: DynMember {
                application,
                ordinal: 0,
            },
            actuals: vec![],
            receiver: DynReceiver {
                mutable: true,
                mode: PassingMode::BorrowMut,
            },
            parameters: vec![],
            parameter_modes: vec![],
            result: Type::Unit,
            result_mode: PassingMode::Infer,
            call_kind: CallKind::Direct,
            storage_life: None,
            contract: DynContract {
                requires: None,
                guarantees: Some(vec![DynGuarantee::StorageSources(StorageSourceBinding {
                    position: old.clone(),
                    sources: (vec![old, next])
                        .into_iter()
                        .map(zil_ir::StorageSource::Exact)
                        .collect(),
                })]),
                hold: None,
                call: None,
                result: None,
                cleanup: None,
                effects: None,
                resume: None,
                threads: None,
            },
        }],
    });
    // This intentionally untrusted interface is transport data, not a callable
    // certificate. Missing prerequisites remain unknown to semantic consumers.
    let mut expected = independent_bytes(&module);
    expected.truncate(expected.len() - 48);
    word(&mut expected, 1);
    expected.extend_from_slice(&[0, 0]);
    small(&mut expected, 0);
    small(&mut expected, 0);
    decl(&mut expected, 7);
    for _ in 0..4 {
        word(&mut expected, 0);
    }
    small(&mut expected, 0);
    for _ in 0..3 {
        word(&mut expected, 0);
    }
    word(&mut expected, 1);
    decl(&mut expected, 7);
    word(&mut expected, 0);
    small(&mut expected, 0);
    word(&mut expected, 0);
    expected.extend_from_slice(&[1, 6]);
    word(&mut expected, 0);
    word(&mut expected, 0);
    expected.extend_from_slice(&[0, 7, 0, 0]);
    expected.extend_from_slice(&[0, 1]);
    word(&mut expected, 1);
    expected.push(2);
    let before_port = |out: &mut Vec<u8>| {
        out.push(0);
        small(out, 0);
        word(out, 1);
        out.push(1);
        small(out, 0);
    };
    before_port(&mut expected);
    word(&mut expected, 2);
    expected.push(0); // Exact(call-before receiver port).
    before_port(&mut expected);
    expected.extend_from_slice(&[0, 0]); // Exact(Parameter(1)).
    small(&mut expected, 1);
    word(&mut expected, 0);
    expected.extend_from_slice(&[0; 7]);
    for _ in 0..5 {
        word(&mut expected, 0);
    }
    assert_eq!(Fingerprint::of(&expected), module.fingerprint());
    let encoded = wire::encode_module_candidate(
        &module,
        &CandidatePlan::empty_for(&module),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(
        wire::decode_module_candidate(&encoded, &Limits::default())
            .unwrap()
            .0,
        module
    );
    let mut changed = module.clone();
    let DynGuarantee::StorageSources(binding) = &mut changed.dyn_interfaces[0].slots[0]
        .contract
        .guarantees
        .as_mut()
        .unwrap()[0]
    else {
        unreachable!()
    };
    binding.sources.remove(0);
    assert_ne!(changed.fingerprint(), module.fingerprint());
}

#[test]
fn finite_deep_recipe_round_trips_with_the_same_exact_identity() {
    for depth in [0, 1, 32, 112] {
        let mut module = fixture();
        let mut pattern = DynTypePattern::Parameter(2);
        for _ in 0..depth {
            pattern = DynTypePattern::Tuple(vec![pattern]);
        }
        let DynTypePattern::Nominal(root) =
            module.dyn_declarations[0].self_pattern.as_mut().unwrap()
        else {
            unreachable!()
        };
        root.arguments[3] = DynPatternActual::Type(pattern);
        let limits = Limits {
            max_call_depth: 100_000,
            ..Limits::default()
        };
        let plan = CandidatePlan::empty_for(&module);
        let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
        let (decoded, recovered) = wire::decode_module_candidate(&bytes, &limits).unwrap();
        assert_eq!(decoded, module, "depth {depth}");
        assert_eq!(recovered, plan, "depth {depth}");
        assert_eq!(decoded.fingerprint(), module.fingerprint(), "depth {depth}");
    }
}
