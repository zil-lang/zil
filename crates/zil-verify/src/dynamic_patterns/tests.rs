//! Raw/Wire tests of the private header interpreter. These are deliberately
//! not whole-module execution or Source-conformance receipts.
use super::*;
use zil_base::{ModuleId, NodeId};
fn id(n: u32) -> LifeDeclarationId {
    LifeDeclarationId {
        module: ModuleId(0),
        declaration: NodeId(n),
        compiler: false,
    }
}
fn fixture(formals: Vec<u32>, classes: Vec<u32>, actuals: Vec<u32>) -> TypedModule {
    let (mut module, _) = crate::tests::baseline();
    let nominal = id(10);
    let implementation = id(11);
    let trait_id = id(12);
    let count = classes.iter().copied().max().map_or(0, |n| n + 1);
    let self_type = Type::BorrowedStorage {
        storage: Box::new(Type::Record {
            name: "Provider".into(),
            fields: vec![],
        }),
        life: StorageLifeShape {
            binder_count: count,
            slots: vec![],
            retained: vec![],
            nominal_arguments: classes
                .iter()
                .enumerate()
                .map(|(argument, binder)| NominalLifeArgument {
                    path: vec![],
                    declaration: nominal,
                    argument: argument as u32,
                    binder: *binder,
                })
                .collect(),
        },
    };
    module.life_instances.push(LifeNominalInstance {
        declaration: nominal,
        ty: self_type.clone(),
        arguments: classes
            .into_iter()
            .map(LifeNominalActual::Lifetime)
            .collect(),
    });
    module.dyn_declarations.push(DynDeclaration {
        direct_supertraits: None,
        requirements: None,
        associated_definitions: None,
        method_signatures: None,
        method_definitions: None,
        self_pattern: Some(zil_ir::DynTypePattern::Nominal(Box::new(DynSelfPattern {
            declaration: nominal,
            arguments: formals
                .into_iter()
                .map(DynPatternActual::LifeParameter)
                .collect(),
        }))),
        implemented_trait: Some(DynSelfPattern {
            declaration: trait_id,
            arguments: (0..actuals.len() as u32)
                .map(DynPatternActual::LifeParameter)
                .collect(),
        }),
        id: implementation,
        identity: LifeDeclarationIdentity {
            package: "test".into(),
            module_path: vec![],
            declaration_path: vec!["ProviderImpl".into()],
            generation_path: vec![],
        },
        kind: DynDeclarationKind::Impl,
        parameters: vec![LifeParameterKind::Lifetime; actuals.len()],
        members: vec![],
    });
    module.dyn_interfaces.push(DynInterface {
        context: Some(Type::Unit),
        owning_cleanup: None,
        id: DynInterfaceId {
            module: ModuleId(0),
            index: 0,
        },
        application: DynTraitApplication {
            declaration: trait_id,
            actuals: actuals
                .iter()
                .map(|n| DynGenericActual::Life(StorageBinder::Fixed(*n)))
                .collect(),
        },
        inherited: vec![],
        associated_types: vec![],
        associated_constants: vec![],
        retained_life: RetainedLifeShape {
            binder_count: 2,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
        slots: vec![],
    });
    module.dyn_vtables.push(DynVtable {
        self_life: None,
        implementation_witnesses: None,
        self_classes: (0..count).map(StorageBinder::Fixed).collect(),
        id: DynVtableId {
            module: ModuleId(0),
            index: 0,
        },
        interface: module.dyn_interfaces[0].id,
        self_type,
        implementation,
        actuals: actuals
            .into_iter()
            .map(|n| DynGenericActual::Life(StorageBinder::Fixed(n)))
            .collect(),
        ports: vec![],
        entries: vec![],
        cleanup: None,
        owned_storage: Some(DynOwnedStorage::Heap),
    });
    module
}
fn check_raw_and_wire(module: &TypedModule, pass: bool) {
    let limits = Limits::default();
    let plan = CandidatePlan::empty_for(module);
    let bytes = zil_ir::wire::encode_module_candidate(module, &plan, &limits).unwrap();
    let (decoded, _) = zil_ir::wire::decode_module_candidate(&bytes, &limits).unwrap();
    for module in [module, &decoded] {
        let mut budget = Budget::new(&limits);
        let result = provider(
            module,
            &module.dyn_vtables[0],
            &module.dyn_declarations[0],
            &module.dyn_interfaces[0],
            &mut budget,
        );
        assert_eq!(result.is_ok(), pass, "{result:?}");
        if let Err(error) = result {
            assert_eq!(error.code, "E-VERIFY-DYN-PATTERN");
        }
    }
}
#[test]
fn dyn_pattern_preserves_original_formal_order_in_raw_and_wire() {
    let module = fixture(vec![1, 0], vec![0, 1], vec![1, 0]);
    check_raw_and_wire(&module, true);
    let mut wrong = module.clone();
    wrong.dyn_vtables[0].actuals.swap(0, 1);
    check_raw_and_wire(&wrong, false);
    let mut wrong = module.clone();
    let DynTypePattern::Nominal(pattern) = wrong.dyn_declarations[0].self_pattern.as_mut().unwrap()
    else {
        unreachable!()
    };
    pattern.arguments.swap(0, 1);
    check_raw_and_wire(&wrong, false);
}
#[test]
fn dyn_pattern_keeps_equal_life_occurrences_and_phantom_trait_actuals() {
    let module = fixture(vec![0, 1], vec![0, 0], vec![0, 0]);
    check_raw_and_wire(&module, true);
    let mut wrong = module.clone();
    wrong.dyn_vtables[0].actuals[1] = DynGenericActual::Life(StorageBinder::Fixed(1));
    check_raw_and_wire(&wrong, false);
    let module = fixture(vec![0], vec![0], vec![0, 1]);
    check_raw_and_wire(&module, true);
    let mut wrong = module.clone();
    wrong.dyn_interfaces[0].application.actuals.swap(0, 1);
    check_raw_and_wire(&wrong, false);
    let mut wrong = module.clone();
    wrong.dyn_declarations[0].implemented_trait = None;
    check_raw_and_wire(&wrong, false);
}
#[test]
fn dyn_pattern_missing_nominal_and_foreign_trait_are_unknown() {
    let module = fixture(vec![0], vec![0], vec![0]);
    let mut wrong = module.clone();
    wrong.life_instances.clear();
    check_raw_and_wire(&wrong, false);
    let mut wrong = module.clone();
    wrong.dyn_declarations[0]
        .implemented_trait
        .as_mut()
        .unwrap()
        .declaration = id(99);
    check_raw_and_wire(&wrong, false);
}
