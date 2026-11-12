use super::*;
use zil_base::{ModuleId, NodeId};
fn boundary() -> (TypedModule, TypedFunction, dyn_ports::Rows) {
    let (mut module, _) = crate::tests::baseline();
    let interface = DynInterfaceId {
        module: ModuleId(0),
        index: 0,
    };
    module.dyn_interfaces.push(DynInterface {
        context: Some(Type::Unit),
        owning_cleanup: None,
        id: interface,
        application: DynTraitApplication {
            declaration: LifeDeclarationId {
                module: ModuleId(0),
                declaration: NodeId(9),
                compiler: false,
            },
            actuals: vec![],
        },
        inherited: vec![],
        associated_types: vec![],
        associated_constants: vec![],
        retained_life: RetainedLifeShape {
            binder_count: 1,
            ports: vec![RetainedBorrowPort {
                leaf: Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
                mode: StoredBorrowMode::SharedAddress,
                binder: 0,
            }],
            retained: vec![],
            nominal_arguments: vec![],
        },
        slots: vec![],
    });
    let mut f = module.functions[0].clone();
    f.parameters = vec![LocalId(0), LocalId(1)];
    f.locals = vec![
        LocalDecl {
            id: LocalId(0),
            name: "first".into(),
            ty: Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            mutable: false,
            span: f.span,
        },
        LocalDecl {
            id: LocalId(1),
            name: "same_type_second".into(),
            ty: Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            mutable: false,
            span: f.span,
        },
    ];
    f.return_type = Type::OwnedDyn { interface };
    f.retained_return_sources = Some(vec![StorageSourceBinding {
        position: StoragePosition {
            base: StorageBase::Result,
            path: vec![ContractProjection::RetainedPort(0)],
        },
        sources: vec![StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        }],
    }]);
    let source = Source {
        retained_port: None,
        base: HoldRoot::Parameter(0),
        leaf: vec![],
        inline: vec![],
        heap_parents: Vec::new(),
        heap_element: None,
    };
    let rows = dyn_ports::Rows(vec![(
        vec![ContractProjection::RetainedPort(0)],
        dyn_ports::Port {
            parent_sources: Vec::new(),
            permission: Permission {
                sources: vec![source],
                loans: vec![],
            },
            known: true,
        },
    )]);
    (module, f, rows)
}
fn checked(module: &TypedModule, f: &TypedFunction, rows: &dyn_ports::Rows) -> bool {
    let limits = Limits::default();
    let mut b = Budget::new(&limits);
    b.dyn_contexts = true;
    validate(module, f, rows, &mut b).is_ok()
}
#[test]
fn dyn_result_upper_keeps_actual_same_type_input_and_rejects_local_escape() {
    let (module, f, rows) = boundary();
    assert!(checked(&module, &f, &rows));
    let mut forged = f.clone();
    forged.retained_return_sources.as_mut().unwrap()[0].sources[0].base = StorageBase::Parameter(1);
    assert!(!checked(&module, &forged, &rows));
    let mut forged = rows.clone();
    forged.0[0].1.permission.sources[0].base = HoldRoot::Local(LocalId(0));
    assert!(!checked(&module, &f, &forged));
}
#[test]
fn dyn_result_unknown_missing_and_empty_are_distinct() {
    let (module, f, rows) = boundary();
    let mut claim = f.clone();
    claim.retained_return_sources = None;
    assert!(!checked(&module, &claim, &rows));
    claim.retained_return_sources = Some(vec![]);
    assert!(!checked(&module, &claim, &rows));
    let mut actual = rows.clone();
    actual.0[0].1.permission = Permission::default();
    assert!(checked(&module, &claim, &actual));
    actual.0[0].1.known = false;
    assert!(!checked(&module, &claim, &actual));
    assert!(!checked(&module, &claim, &dyn_ports::Rows::default()));
    claim.retained_return_sources = Some(vec![StorageSourceBinding {
        position: StoragePosition {
            base: StorageBase::Result,
            path: vec![ContractProjection::RetainedPort(0)],
        },
        sources: vec![],
    }]);
    assert!(!checked(&module, &claim, &rows));
    actual.0[0].1.known = true;
    assert!(checked(&module, &claim, &actual));
}
#[test]
fn dyn_port_namespace_and_complete_carrier_are_never_task_or_head_roots() {
    let path = vec![
        ContractProjection::Place(PlaceProjection::Field(1)),
        ContractProjection::Capture(2),
    ];
    let dyn_source = Source {
        retained_port: Some(PortOrigin::Dyn {
            carrier: path.clone(),
            port: 0,
        }),
        base: HoldRoot::Parameter(0),
        leaf: vec![],
        inline: vec![],
        heap_parents: Vec::new(),
        heap_element: None,
    };
    let task_source = Source {
        retained_port: Some(PortOrigin::Task(0)),
        ..dyn_source.clone()
    };
    let head = Source {
        retained_port: None,
        ..dyn_source.clone()
    };
    let mut b = Budget::new(&Limits::default());
    assert!(!overlap(&dyn_source, &task_source, &mut b).unwrap());
    assert!(!overlap(&dyn_source, &head, &mut b).unwrap());
    let mut other = dyn_source.clone();
    other.retained_port = Some(PortOrigin::Dyn {
        carrier: vec![ContractProjection::Place(PlaceProjection::Field(2))],
        port: 0,
    });
    assert!(!overlap(&dyn_source, &other, &mut b).unwrap());
    let mut requirements = vec![];
    apart(&dyn_source, &task_source, &mut requirements, &mut b).unwrap();
    assert_eq!(
        requirements.len(),
        1,
        "different abstract namespaces still require actual caller alias checking"
    );
}
