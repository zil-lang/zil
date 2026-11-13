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
    f.storage_life = Some(StorageLifeContract {
        static_bindings: Vec::new(),
        fixed_binder_count: 0,
        invocation_binder_count: 2,
        bindings: vec![
            StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(0),
                    path: vec![],
                },
                binder: StorageBinder::Invocation(0),
            },
            StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(1),
                    path: vec![],
                },
                binder: StorageBinder::Invocation(1),
            },
        ],
        classes: vec![SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Result,
                path: vec![],
                class: 0,
            },
            binder: StorageBinder::Invocation(0),
        }],
    });
    f.retained_return_sources = Some(vec![StorageSourceBinding {
        position: StoragePosition {
            base: StorageBase::Result,
            path: vec![ContractProjection::RetainedPort(0)],
        },
        sources: (vec![StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        }])
        .into_iter()
        .map(zil_ir::StorageSource::Exact)
        .collect(),
    }]);
    let source = Source {
        static_dependency: None,
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
            head_sources: Vec::new(),
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
    exact_source_mut(&mut forged.retained_return_sources.as_mut().unwrap()[0].sources[0]).base =
        StorageBase::Parameter(1);
    assert!(!checked(&module, &forged, &rows));
    let mut forged = rows.clone();
    forged.0[0].1.permission.sources[0].base = HoldRoot::Local(LocalId(0));
    assert!(!checked(&module, &f, &forged));
}
#[test]
fn exact_root_address_is_the_complete_parameter_value_not_a_stored_pointee() {
    let (module, mut f, rows) = boundary();
    f.locals[0].ty = Type::Address {
        mutable: false,
        inner: Box::new(f.locals[0].ty.clone()),
    };
    let position = StoragePosition {
        base: StorageBase::Parameter(0),
        path: vec![],
    };
    let (leaf, _) = input(&module, &f, &position, &mut Budget::new(&Limits::default())).unwrap();
    assert_eq!(leaf, f.locals[0].ty);
    assert!(!checked(&module, &f, &rows));
}

#[test]
fn exact_does_not_take_a_scalar_field_address_or_weaken_mutability() {
    let (module, mut f, rows) = boundary();
    let Type::Address { mutable, .. } = &mut f.locals[0].ty else {
        unreachable!()
    };
    *mutable = true;
    assert!(!checked(&module, &f, &rows));
    f.locals[0].ty = Type::Address {
        mutable: false,
        inner: Box::new(Type::Record {
            name: "Pair".into(),
            fields: vec![("left".into(), Type::I32)],
        }),
    };
    let position = StoragePosition {
        base: StorageBase::Parameter(0),
        path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
    };
    assert!(input(&module, &f, &position, &mut Budget::new(&Limits::default())).is_err());
}

#[test]
fn static_is_a_real_target_and_never_an_empty_or_unrelated_exact_upper() {
    let (module, mut f, mut rows) = boundary();
    rows.0[0].1.permission.sources[0].base = HoldRoot::Static;
    assert!(!checked(&module, &f, &rows));
    f.retained_return_sources.as_mut().unwrap()[0]
        .sources
        .clear();
    assert!(!checked(&module, &f, &rows));
    f.retained_return_sources = Some(vec![]);
    assert!(!checked(&module, &f, &rows));
    rows.0[0].1.permission.sources.clear();
    assert!(checked(&module, &f, &rows));
}

#[test]
fn place_address_preserves_actual_field_window_class_and_access_mode() {
    let (module, mut f, mut actual) = boundary();
    f.locals[0].ty = Type::Address {
        mutable: true,
        inner: Box::new(Type::Record {
            name: "Pair".into(),
            fields: vec![("left".into(), Type::I32), ("right".into(), Type::I32)],
        }),
    };
    f.retained_return_sources.as_mut().unwrap()[0].sources = vec![StorageSource::PlaceAddress {
        place: StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
        },
        mode: StoredBorrowMode::SharedAddress,
    }];
    actual.0[0].1.permission.sources[0].inline = vec![PlaceProjection::Field(0)];
    assert!(checked(&module, &f, &actual));
    let mut wrong = f.clone();
    let StorageSource::PlaceAddress { place, .. } =
        &mut wrong.retained_return_sources.as_mut().unwrap()[0].sources[0]
    else {
        unreachable!()
    };
    place.path = vec![ContractProjection::Place(PlaceProjection::Field(1))];
    assert!(!checked(&module, &wrong, &actual));
    let mut wrong = actual.clone();
    wrong.0[0].1.permission.sources[0].inline.clear();
    wrong.0[0].1.permission.sources[0].leaf = vec![PlaceProjection::Field(0)];
    assert!(!checked(&module, &f, &wrong));
    let mut wrong = f.clone();
    wrong.storage_life.as_mut().unwrap().classes[0].binder = StorageBinder::Invocation(1);
    assert!(!checked(&module, &wrong, &actual));
    let Type::Address { mutable, .. } = &mut f.locals[0].ty else {
        unreachable!()
    };
    *mutable = false;
    let StorageSource::PlaceAddress { mode, .. } =
        &mut f.retained_return_sources.as_mut().unwrap()[0].sources[0]
    else {
        unreachable!()
    };
    *mode = StoredBorrowMode::ExclusiveAddress;
    assert!(!checked(&module, &f, &actual));
}

#[test]
fn static_atom_requires_actual_target_complete_leaf_and_readonly_mode() {
    let (module, mut f, mut actual) = boundary();
    actual.0[0].1.permission.sources[0].base = HoldRoot::Static;
    f.retained_return_sources.as_mut().unwrap()[0].sources = vec![StorageSource::Static {
        leaf: module.dyn_interfaces[0].retained_life.ports[0].leaf.clone(),
        mode: StoredBorrowMode::SharedAddress,
    }];
    assert!(checked(&module, &f, &actual));
    let mut no_static = actual.clone();
    no_static.0[0].1.permission.sources.clear();
    assert!(
        !checked(&module, &f, &no_static),
        "known empty is not static evidence"
    );
    let mut wrong = actual.clone();
    wrong.0[0].1.permission.sources[0].base = HoldRoot::Parameter(0);
    assert!(!checked(&module, &f, &wrong));
    let mut wrong = actual.clone();
    wrong.0[0].1.parent_sources.push(Source {
        static_dependency: None,
        base: HoldRoot::Local(LocalId(0)),
        retained_port: None,
        leaf: vec![],
        inline: vec![],
        heap_element: None,
        heap_parents: vec![],
    });
    assert!(!checked(&module, &f, &wrong));
    let mut wrong = f.clone();
    let StorageSource::Static { mode, .. } =
        &mut wrong.retained_return_sources.as_mut().unwrap()[0].sources[0]
    else {
        unreachable!()
    };
    *mode = StoredBorrowMode::ExclusiveAddress;
    assert!(!checked(&module, &wrong, &actual));
    let StorageSource::Static { leaf, .. } =
        &mut f.retained_return_sources.as_mut().unwrap()[0].sources[0]
    else {
        unreachable!()
    };
    *leaf = Type::Address {
        mutable: false,
        inner: Box::new(Type::I64),
    };
    assert!(!checked(&module, &f, &actual));
}

#[test]
fn static_return_evidence_is_checked_without_inventing_an_input_binder() {
    let (module, mut f, mut actual) = boundary();
    actual.0[0].1.permission.sources[0].base = HoldRoot::Static;
    f.parameters.clear();
    f.locals.clear();
    let life = f.storage_life.as_mut().unwrap();
    life.bindings.clear();
    life.fixed_binder_count = 1;
    life.invocation_binder_count = 0;
    life.classes[0].binder = StorageBinder::Fixed(0);
    f.retained_return_sources.as_mut().unwrap()[0].sources = vec![StorageSource::Static {
        leaf: module.dyn_interfaces[0].retained_life.ports[0].leaf.clone(),
        mode: StoredBorrowMode::SharedAddress,
    }];
    assert!(checked(&module, &f, &actual));
    actual.0[0].1.known = false;
    assert!(!checked(&module, &f, &actual));
}

#[test]
fn retaining_an_actual_slot_requires_its_current_initialized_region() {
    let (_, _, rows) = boundary();
    let mut permission = rows.0[0].1.permission.clone();
    permission.sources[0].inline = vec![PlaceProjection::Field(0)];
    let mut state = Frame::default();
    let mut hole = permission.sources[0].clone();
    hole.inline.clear();
    state.vacant.push(hole);
    assert!(initialized(&state, &permission, &mut Budget::new(&Limits::default())).is_err());
    state.vacant[0].inline = vec![PlaceProjection::Field(1)];
    initialized(&state, &permission, &mut Budget::new(&Limits::default())).unwrap();
    // A pointer read from a holder is a different actual target from its slot.
    permission.sources[0].leaf = vec![PlaceProjection::Field(1)];
    permission.sources[0].inline.clear();
    initialized(&state, &permission, &mut Budget::new(&Limits::default())).unwrap();
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
        sources: (vec![])
            .into_iter()
            .map(zil_ir::StorageSource::Exact)
            .collect(),
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
        static_dependency: None,
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

fn actual_helper_step(update: bool, head_parent: bool) -> (Payload, Frame, Vec<Loan>, LocalId) {
    let (mut module, mut caller, _) = boundary();
    let Type::OwnedDyn { interface } = caller.return_type else {
        unreachable!()
    };
    let owner = LocalId(2);
    for (index, local) in caller.locals.iter_mut().enumerate() {
        local.ty = Type::I32;
        local.mutable = true;
        local.id = LocalId(index as u32);
    }
    caller.locals.push(LocalDecl {
        id: owner,
        name: "actual_owner".into(),
        ty: Type::OwnedDyn { interface },
        mutable: true,
        span: caller.span,
    });
    caller.parameters.clear();
    let pointer = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    caller.values = vec![
        Type::Address {
            mutable: true,
            inner: Box::new(Type::OwnedDyn { interface }),
        },
        pointer.clone(),
        pointer.clone(),
    ];
    let mut callee = caller.clone();
    callee.id = FunctionId(1);
    callee.return_type = pointer;
    callee.parameters = vec![LocalId(0), LocalId(1)];
    callee.locals[0].ty = caller.values[0].clone();
    callee.locals[1].ty = caller.values[1].clone();
    module.functions = vec![caller.clone(), callee];
    let source = |local| Source {
        static_dependency: None,
        base: HoldRoot::Local(LocalId(local)),
        retained_port: None,
        leaf: vec![],
        inline: vec![],
        heap_parents: Vec::new(),
        heap_element: None,
    };
    let path = vec![ContractProjection::RetainedPort(0)];
    let mut state = Frame {
        locals: vec![Payload::default(); 3],
        ..Frame::default()
    };
    state.locals[2].dyn_ports = dyn_ports::Rows(vec![(
        path.clone(),
        dyn_ports::Port {
            permission: Permission {
                sources: vec![source(0)],
                loans: vec![4],
            },
            parent_sources: vec![],
            head_sources: Vec::new(),
            known: true,
        },
    )]);
    let mut values = vec![Payload::default(); 3];
    values[0].address = Permission {
        sources: vec![source(2)],
        loans: vec![5],
    };
    values[1].address = Permission {
        sources: vec![source(1)],
        loans: vec![6],
    };
    let mut summaries = vec![Summary::default(); 2];
    let old = Source {
        static_dependency: None,
        base: HoldRoot::Parameter(0),
        retained_port: Some(PortOrigin::Dyn {
            carrier: vec![],
            port: 0,
        }),
        leaf: vec![],
        inline: vec![],
        heap_parents: Vec::new(),
        heap_element: None,
    };
    summaries[1].complete = true;
    summaries[1].result = vec![(vec![], vec![old])];
    if head_parent {
        summaries[1].dyn_result_parents = vec![Source {
            static_dependency: None,
            base: HoldRoot::Parameter(0),
            retained_port: None,
            leaf: vec![],
            inline: vec![],
            heap_parents: Vec::new(),
            heap_element: None,
        }];
    }
    if update {
        summaries[1].dyn_after = vec![(
            0,
            dyn_ports::Rows(vec![(
                path,
                dyn_ports::Port {
                    permission: Permission {
                        sources: vec![Source {
                            static_dependency: None,
                            base: HoldRoot::Parameter(1),
                            retained_port: None,
                            leaf: vec![],
                            inline: vec![],
                            heap_parents: Vec::new(),
                            heap_element: None,
                        }],
                        loans: vec![],
                    },
                    parent_sources: vec![],
                    head_sources: Vec::new(),
                    known: true,
                },
            )]),
        )];
    }
    let mut loans = vec![Loan::default(); 7];
    for (id, local) in [(4, 0), (5, 2), (6, 1)] {
        loans[id].sources = vec![source(local)];
    }
    let mut b = Budget::new(&Limits::default());
    b.dyn_contexts = true;
    let i = Instruction {
        result: Some(ValueId(2)),
        kind: InstructionKind::Call {
            function: FunctionId(1),
            arguments: vec![ValueId(0), ValueId(1)],
        },
        span: caller.span,
    };
    let out = step(
        &module,
        &caller,
        &i,
        &mut state,
        &values,
        &summaries,
        &Vec::new(),
        &mut dyn_leases::Sites::default(),
        &mut loans,
        &mut b,
    )
    .unwrap();
    (out, state, loans, owner)
}
#[test]
fn dyn_helper_result_uses_call_before_port_while_after_installs_new_target() {
    let (result, state, loans, owner) = actual_helper_step(true, false);
    assert_eq!(
        result.address.sources[0].base,
        HoldRoot::Local(LocalId(0)),
        "returned old pointer must not be retargeted to the after-state"
    );
    assert_eq!(
        state.locals[owner.0 as usize].dyn_ports.0[0]
            .1
            .permission
            .sources[0]
            .base,
        HoldRoot::Local(LocalId(1))
    );
    assert!(loans[2].parents.contains(&4));
    assert!(!loans[2].parents.contains(&6));
}
#[test]
fn dyn_helper_preserves_non_target_head_parent_after_address_result_assignment() {
    let (result, _, loans, owner) = actual_helper_step(false, true);
    assert!(
        result
            .address
            .sources
            .iter()
            .all(|source| source.base != HoldRoot::Local(owner)),
        "the head is a parent, not a fabricated data target"
    );
    assert!(loans[2].parents.contains(&4));
    assert!(
        loans[2].parents.contains(&5),
        "ordinary result assignment must preserve the real receiver head parent"
    );
    let mut b = Budget::new(&Limits::default());
    let head = Permission {
        sources: vec![Source {
            static_dependency: None,
            base: HoldRoot::Local(owner),
            retained_port: None,
            leaf: vec![],
            inline: vec![],
            heap_element: None,
            heap_parents: Vec::new(),
        }],
        loans: vec![],
    };
    let active = ancestry(&result.address.loans, &loans, &mut b).unwrap();
    assert!(
        access(&head, true, &active, &loans, &mut vec![], &mut b).is_err(),
        "retiring the real owner while its short view lives must reject"
    );
    let mut stripped = loans.clone();
    stripped[2].parents.retain(|parent| *parent != 5);
    let active = ancestry(&result.address.loans, &stripped, &mut b).unwrap();
    assert!(
        access(&head, true, &active, &stripped, &mut vec![], &mut b).is_ok(),
        "removing the non-target head parent would incorrectly authorize retirement"
    );
}

#[test]
fn borrowed_return_cannot_hide_a_local_head_behind_external_data_sources() {
    let (_, f, _) = boundary();
    let mut b = Budget::new(&Limits::default());
    let data = Source {
        static_dependency: None,
        base: HoldRoot::Parameter(0),
        retained_port: None,
        leaf: vec![],
        inline: vec![],
        heap_element: None,
        heap_parents: Vec::new(),
    };
    let local_head = Source {
        base: HoldRoot::Local(LocalId(2)),
        ..data.clone()
    };
    let loans = vec![
        Loan {
            receiver_head: false,
            sources: vec![data.clone()],
            parents: vec![1],
            exclusive: false,
        },
        Loan {
            receiver_head: false,
            sources: vec![local_head],
            parents: vec![],
            exclusive: false,
        },
    ];
    let result = Permission {
        sources: vec![data.clone()],
        loans: vec![0],
    };
    let actual = parents(&result, &loans, &mut b).unwrap();
    assert!(check_parent_escape(&f, &actual, &mut b).is_err());
    assert!(check_parent_escape(&f, &[data], &mut b).is_ok());
}

fn exact_source_mut(source: &mut zil_ir::StorageSource) -> &mut zil_ir::StoragePosition {
    match source {
        zil_ir::StorageSource::Exact(position) => position,
        _ => panic!("fixture requires an exact source"),
    }
}

#[test]
fn owned_inline_region_has_all_actual_windows_and_stops_at_borrowed_payloads() {
    let (module, mut f, mut actual) = boundary();
    f.locals[0].ty = Type::Address {
        mutable: true,
        inner: Box::new(Type::Record {
            name: "Pair".into(),
            fields: vec![("left".into(), Type::I32), ("right".into(), Type::I32)],
        }),
    };
    let leaf = module.dyn_interfaces[0].retained_life.ports[0].leaf.clone();
    f.retained_return_sources.as_mut().unwrap()[0].sources = vec![StorageSource::OwnedView {
        head: StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        },
        leaf: leaf.clone(),
        mode: StoredBorrowMode::SharedAddress,
    }];
    actual.0[0].1.permission.sources[0].inline = vec![PlaceProjection::Field(0)];
    assert!(checked(&module, &f, &actual));
    actual.0[0].1.permission.sources[0].inline = vec![PlaceProjection::Field(1)];
    assert!(checked(&module, &f, &actual));
    let limits = Limits::default();
    let sources = published_sources(
        &module,
        &f,
        &[ContractProjection::RetainedPort(0)],
        &mut Budget::new(&limits),
    )
    .unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].inline, [PlaceProjection::Field(0)]);
    assert_eq!(sources[1].inline, [PlaceProjection::Field(1)]);
    assert!(
        sources
            .iter()
            .all(|s| s.leaf.is_empty() && s.retained_port.is_none())
    );
    // A declaration's region upper contains both real windows; caller-side
    // current initialization must still reject a vacated candidate window.
    let state = Frame {
        vacant: vec![sources[0].clone()],
        ..Frame::default()
    };
    let permission = Permission {
        sources,
        loans: vec![],
    };
    assert!(initialized(&state, &permission, &mut Budget::new(&limits)).is_err());
    actual.0[0].1.permission.sources[0].base = HoldRoot::Parameter(1);
    assert!(!checked(&module, &f, &actual));
    actual.0[0].1.permission.sources[0].base = HoldRoot::Parameter(0);
    f.locals[0].ty = Type::Address {
        mutable: false,
        inner: Box::new(Type::Record {
            name: "Holder".into(),
            fields: vec![("pointer".into(), leaf)],
        }),
    };
    actual.0[0].1.permission.sources[0].inline.clear();
    actual.0[0].1.permission.sources[0].leaf = vec![PlaceProjection::Field(0)];
    assert!(
        !checked(&module, &f, &actual),
        "owned region cannot cross the stored pointer edge"
    );
}

#[test]
fn owned_inline_region_respects_selected_head_access_and_complete_leaf_type() {
    let (mut module, mut f, mut actual) = boundary();
    f.locals[0].ty = Type::Address {
        mutable: true,
        inner: Box::new(Type::Tuple(vec![Type::I32, Type::I32])),
    };
    let mut leaf = module.dyn_interfaces[0].retained_life.ports[0].leaf.clone();
    let Type::Address { mutable, .. } = &mut leaf else {
        unreachable!()
    };
    *mutable = true;
    module.dyn_interfaces[0].retained_life.ports[0].leaf = leaf.clone();
    module.dyn_interfaces[0].retained_life.ports[0].mode = StoredBorrowMode::ExclusiveAddress;
    f.retained_return_sources.as_mut().unwrap()[0].sources = vec![StorageSource::OwnedView {
        head: StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
        },
        leaf,
        mode: StoredBorrowMode::ExclusiveAddress,
    }];
    actual.0[0].1.permission.sources[0].inline = vec![PlaceProjection::Field(0)];
    assert!(checked(&module, &f, &actual));
    actual.0[0].1.permission.sources[0].inline = vec![PlaceProjection::Field(1)];
    assert!(
        !checked(&module, &f, &actual),
        "another same-type sibling is outside the selected region"
    );
    actual.0[0].1.permission.sources[0].inline = vec![PlaceProjection::Field(0)];
    let Type::Address { mutable, .. } = &mut f.locals[0].ty else {
        unreachable!()
    };
    *mutable = false;
    assert!(
        !checked(&module, &f, &actual),
        "shared actual head cannot grant exclusive view"
    );
    let StorageSource::OwnedView { mode, leaf, .. } =
        &mut f.retained_return_sources.as_mut().unwrap()[0].sources[0]
    else {
        unreachable!()
    };
    *mode = StoredBorrowMode::SharedAddress;
    let Type::Address { mutable, inner } = leaf else {
        unreachable!()
    };
    *mutable = false;
    **inner = Type::I64;
    assert!(
        !checked(&module, &f, &actual),
        "same machine width or outer mode does not replace complete leaf type"
    );
}

#[test]
fn output_only_dyn_class_stays_pending_until_actual_static_or_known_no_target() {
    let (module, mut f, mut actual) = boundary();
    f.parameters.clear();
    let life = f.storage_life.as_mut().unwrap();
    life.fixed_binder_count = 1;
    life.invocation_binder_count = 0;
    life.bindings.clear();
    life.classes[0].binder = StorageBinder::Fixed(0);
    f.retained_return_sources.as_mut().unwrap()[0].sources = vec![StorageSource::Static {
        leaf: module.dyn_interfaces[0].retained_life.ports[0].leaf.clone(),
        mode: StoredBorrowMode::SharedAddress,
    }];
    actual.0[0].1.permission.sources[0].base = HoldRoot::Static;
    let limits = Limits::default();
    let mut b = Budget::new(&limits);
    b.dyn_contexts = true;
    let pending = pending::register(&module, &f, &mut b).unwrap();
    assert_eq!(
        pending,
        [f.storage_life.as_ref().unwrap().classes[0].position.clone()]
    );
    assert_eq!(
        b.dyn_static_classes[0].grounding,
        DynClassGrounding::Unknown
    );
    let contract = f.storage_life.as_ref().unwrap();
    assert!(
        signature_life::function_relation(&module, &[], &f.return_type, contract, None, false)
            .is_err(),
        "generic structural caller has no concrete actual-CFG obligation ledger"
    );
    signature_life::function_relation_pending(
        &module,
        &[],
        &f.return_type,
        contract,
        None,
        false,
        false,
        &pending,
    )
    .unwrap();
    validate(&module, &f, &actual, &mut b).unwrap();
    assert_eq!(b.dyn_static_classes[0].grounding, DynClassGrounding::Static);
    assert_eq!(
        f.storage_life.as_ref().unwrap().classes[0].binder,
        StorageBinder::Fixed(0)
    );
    assert!(f.storage_life.as_ref().unwrap().static_bindings.is_empty());
    // A subsequent actual summary must be rechecked, even if an earlier solve
    // iteration proved the same occurrence. A declaration never fills facts.
    let mut wrong = actual.clone();
    wrong.0[0].1.permission.sources[0].base = HoldRoot::Parameter(0);
    assert!(pending::discharge(&f, &wrong, &mut b).is_err());
    assert_eq!(
        b.dyn_static_classes[0].grounding,
        DynClassGrounding::Unknown
    );
    let mut unknown = actual.clone();
    unknown.0[0].1.known = false;
    assert!(pending::discharge(&f, &unknown, &mut b).is_err());
    assert!(pending::discharge(&f, &dyn_ports::Rows::default(), &mut b).is_err());
    // Complete known-empty is a no-target proof, never static evidence.
    actual.0[0].1.permission.sources.clear();
    f.retained_return_sources = Some(vec![]);
    validate(&module, &f, &actual, &mut b).unwrap();
    assert_eq!(
        b.dyn_static_classes[0].grounding,
        DynClassGrounding::NoTarget
    );
}

#[test]
fn static_class_obligations_do_not_borrow_evidence_from_another_port_or_parent() {
    let (mut module, mut f, mut actual) = boundary();
    f.parameters.clear();
    let life = f.storage_life.as_mut().unwrap();
    life.fixed_binder_count = 1;
    life.invocation_binder_count = 0;
    life.bindings.clear();
    life.classes[0].binder = StorageBinder::Fixed(0);
    let port = module.dyn_interfaces[0].retained_life.ports[0].clone();
    module.dyn_interfaces[0].retained_life.ports.push(port);
    let mut other = actual.0[0].clone();
    other.0 = vec![ContractProjection::RetainedPort(1)];
    actual.0[0].1.permission.sources[0].base = HoldRoot::Static;
    // Same leaf, mode and class, different actual port occurrence.
    actual.0.push(other);
    let mut b = Budget::new(&Limits::default());
    b.dyn_contexts = true;
    pending::register(&module, &f, &mut b).unwrap();
    assert_eq!(b.dyn_static_classes[0].ports.len(), 2);
    assert!(pending::discharge(&f, &actual, &mut b).is_err());
    actual.0[1].1.permission.sources[0].base = HoldRoot::Static;
    pending::discharge(&f, &actual, &mut b).unwrap();
    actual.0[1].1.parent_sources.push(Source {
        static_dependency: None,
        base: HoldRoot::Local(LocalId(0)),
        retained_port: None,
        leaf: vec![],
        inline: vec![],
        heap_element: None,
        heap_parents: vec![],
    });
    assert!(pending::discharge(&f, &actual, &mut b).is_err());
    assert_eq!(
        b.dyn_static_classes[0].grounding,
        DynClassGrounding::Unknown
    );
}

#[test]
fn output_only_invocation_class_preserves_its_original_nominal_call_domain() {
    let (module, mut f, mut actual) = boundary();
    f.parameters.clear();
    let life = f.storage_life.as_mut().unwrap();
    life.fixed_binder_count = 0;
    life.invocation_binder_count = 1;
    life.bindings.clear();
    life.classes[0].binder = StorageBinder::Invocation(0);
    f.retained_return_sources.as_mut().unwrap()[0].sources = vec![StorageSource::Static {
        leaf: module.dyn_interfaces[0].retained_life.ports[0].leaf.clone(),
        mode: StoredBorrowMode::SharedAddress,
    }];
    actual.0[0].1.permission.sources[0].base = HoldRoot::Static;
    let mut b = Budget::new(&Limits::default());
    b.dyn_contexts = true;
    let keys = pending::register(&module, &f, &mut b).unwrap();
    assert_eq!(keys.len(), 1);
    let c = f.storage_life.as_ref().unwrap();
    assert!(
        signature_life::function_relation(&module, &[], &f.return_type, c, None, false).is_err()
    );
    signature_life::function_relation_pending(
        &module,
        &[],
        &f.return_type,
        c,
        None,
        false,
        false,
        &keys,
    )
    .unwrap();
    validate(&module, &f, &actual, &mut b).unwrap();
    assert_eq!(b.dyn_static_classes[0].binder, StorageBinder::Invocation(0));
    assert_eq!(b.dyn_static_classes[0].grounding, DynClassGrounding::Static);
    assert_eq!(module.dyn_interfaces[0].retained_life.binder_count, 1);
    let mut forged = actual.clone();
    forged.0[0].1.permission.sources[0].base = HoldRoot::Parameter(0);
    assert!(pending::discharge(&f, &forged, &mut b).is_err());
    assert_eq!(
        b.dyn_static_classes[0].grounding,
        DynClassGrounding::Unknown
    );
}

fn borrowed_owner_boundary() -> (TypedModule, TypedFunction, dyn_ports::Rows) {
    let (module, mut f, mut actual) = boundary();
    let owner = f.return_type.clone();
    for local in &mut f.locals {
        local.ty = Type::Address {
            mutable: true,
            inner: Box::new(owner.clone()),
        };
    }
    let life = f.storage_life.as_mut().unwrap();
    life.fixed_binder_count = 1;
    let output = life.classes[0].position.clone();
    life.classes = vec![
        SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(0),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Fixed(0),
        },
        SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(1),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Fixed(0),
        },
        SignatureLifeClassBinding {
            position: output,
            binder: StorageBinder::Fixed(0),
        },
    ];
    f.retained_return_sources.as_mut().unwrap()[0].sources = vec![StorageSource::ReborrowedLeaf {
        source: StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![ContractProjection::RetainedPort(0)],
        },
        head: StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        },
    }];
    actual.0[0].1.permission.sources[0].retained_port = Some(PortOrigin::Dyn {
        carrier: vec![],
        port: 0,
    });
    (module, f, actual)
}

fn context_environment_boundary() -> (TypedModule, TypedFunction, dyn_ports::Rows) {
    let (mut module, mut f, mut rows) = boundary();
    let owner = f.return_type.clone();
    for local in &mut f.locals {
        local.ty = owner.clone();
    }
    let closure = ClosureId(0);
    let life = StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: 0,
        bindings: vec![],
        static_bindings: vec![],
        classes: vec![SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Capture(0),
                path: vec![],
                class: 0,
            },
            binder: StorageBinder::Fixed(0),
        }],
    };
    module.closures.push(ClosureDefinition {
        id: closure,
        function: f.id,
        captures: vec![owner],
        parameters: vec![],
        result: Type::I32,
        kind: CallableKind::Fn,
        lending_captures: vec![],
        storage_life: Some(life.clone()),
    });
    f.return_type = Type::Closure(closure);
    f.storage_life = Some(StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: 0,
        bindings: vec![],
        static_bindings: vec![],
        classes: [
            StorageBase::Parameter(0),
            StorageBase::Parameter(1),
            StorageBase::Result,
        ]
        .into_iter()
        .map(|base| SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base,
                path: vec![],
                class: 0,
            },
            binder: StorageBinder::Fixed(0),
        })
        .collect(),
    });
    f.retained_return_sources = None;
    f.callable_return_contract = Some(CallableReturnContract {
        life: Some(life),
        source: CallableReturnSource::Fresh,
        closure,
        kind: CallableKind::Fn,
        hold: vec![0],
        call: vec![0],
        writes: vec![],
        cleanup: vec![0],
        lending_captures: vec![],
        capture_sources: Some(vec![StorageSourceBinding {
            position: StoragePosition {
                base: StorageBase::Capture(0),
                path: vec![ContractProjection::RetainedPort(0)],
            },
            sources: vec![StorageSource::Exact(StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![ContractProjection::RetainedPort(0)],
            })],
        }]),
    });
    rows.0[0].0.insert(0, ContractProjection::Capture(0));
    rows.0[0].1.permission.sources[0].retained_port = Some(PortOrigin::Dyn {
        carrier: vec![],
        port: 0,
    });
    (module, f, rows)
}

#[test]
fn context_environment_logical_source_family_keeps_same_type_actual_roots_distinct() {
    let (module, mut f, rows) = context_environment_boundary();
    assert!(checked(&module, &f, &rows));
    let mut b = Budget::new(&Limits::default());
    b.dyn_contexts = true;
    let physical = returned_environment::leaves(&module, ClosureId(0), &mut b).unwrap();
    assert!(
        physical.is_empty(),
        "opaque port is not a physical pointer slot"
    );
    assert!(
        b.pending_dyn_captures.contains(&ClosureId(0)),
        "physical handoff leaves an actual memory obligation"
    );
    let StorageSource::Exact(position) = &mut f
        .callable_return_contract
        .as_mut()
        .unwrap()
        .capture_sources
        .as_mut()
        .unwrap()[0]
        .sources[0]
    else {
        unreachable!()
    };
    position.base = StorageBase::Parameter(1);
    assert!(
        !checked(&module, &f, &rows),
        "equal full Type, mode and Fixed class do not replace the actual environment operand"
    );
    f.callable_return_contract.as_mut().unwrap().capture_sources = Some(vec![]);
    assert!(!checked(&module, &f, &rows));
    f.callable_return_contract.as_mut().unwrap().capture_sources = None;
    assert!(!checked(&module, &f, &rows));
}

#[test]
fn context_capture_constructor_and_call_select_the_actual_environment_instance() {
    let (mut module, mut f, _) = context_environment_boundary();
    let ty = f.locals[0].ty.clone();
    module.closures[0].captures.push(ty.clone());
    module.closures[0]
        .storage_life
        .as_mut()
        .unwrap()
        .classes
        .push(SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Capture(1),
                path: vec![],
                class: 0,
            },
            binder: StorageBinder::Fixed(0),
        });
    f.locals.push(LocalDecl {
        id: LocalId(2),
        name: "environment".into(),
        ty: Type::Closure(ClosureId(0)),
        mutable: true,
        span: f.span,
    });
    f.values = vec![ty.clone(), ty, Type::Closure(ClosureId(0))];
    let root = |index| Source {
        static_dependency: None,
        base: HoldRoot::Local(LocalId(index)),
        retained_port: None,
        leaf: vec![],
        inline: vec![],
        heap_element: None,
        heap_parents: vec![],
    };
    let mut values = vec![Payload::default(); 3];
    for (index, payload) in values[..2].iter_mut().enumerate() {
        payload.dyn_ports = dyn_ports::Rows(vec![(
            vec![ContractProjection::RetainedPort(0)],
            dyn_ports::Port {
                permission: Permission {
                    sources: vec![root(index as u32)],
                    loans: vec![index],
                },
                parent_sources: vec![],
                head_sources: vec![],
                known: true,
            },
        )]);
    }
    let mut state = Frame {
        locals: vec![Payload::default(); 3],
        ..Frame::default()
    };
    let constructor = Instruction {
        result: Some(ValueId(2)),
        kind: InstructionKind::MakeClosure {
            closure: ClosureId(0),
            captures: vec![ValueId(0), ValueId(1)],
        },
        span: f.span,
    };
    let mut b = Budget::new(&Limits::default());
    b.dyn_contexts = true;
    let out = dyn_captures::transfer(&module, &f, &constructor, &mut state, &values, &mut b)
        .unwrap()
        .unwrap();
    assert_eq!(out.dyn_ports.0.len(), 2);
    state.locals[2] = out;
    let call = Instruction {
        result: None,
        kind: InstructionKind::CallClosureShared {
            callee: LocalId(2),
            arguments: vec![],
        },
        span: f.span,
    };
    for index in 0..2 {
        let source = Source {
            static_dependency: None,
            base: HoldRoot::Capture(index),
            retained_port: Some(PortOrigin::Dyn {
                carrier: vec![],
                port: 0,
            }),
            leaf: vec![],
            inline: vec![],
            heap_element: None,
            heap_parents: vec![],
        };
        let actual = dyn_captures::substitute(
            &module,
            &f,
            &call,
            &source,
            &module.closures[0],
            &[],
            &values,
            &state,
            &mut b,
        )
        .unwrap();
        assert_eq!(actual.sources, [root(index)]);
        assert_eq!(actual.loans, [index as usize]);
    }
}

#[test]
fn reborrow_keeps_data_fixed_class_and_rejects_a_same_typed_unrelated_head() {
    let (module, mut f, actual) = borrowed_owner_boundary();
    assert!(checked(&module, &f, &actual));
    let output = &module.dyn_interfaces[0].retained_life.ports[0].leaf;
    let (_, binder, selected) = atoms::resolve_output(
        &module,
        &f,
        &f.retained_return_sources.as_ref().unwrap()[0].sources[0],
        output,
        &mut Budget::new(&Limits::default()),
    )
    .unwrap();
    assert_eq!(binder, Some(StorageBinder::Fixed(0)));
    assert_eq!(selected.sources(), &actual.0[0].1.permission.sources);
    let head = selected.parent_head().unwrap();
    assert_eq!(head.base, HoldRoot::Parameter(0));
    assert!(head.retained_port.is_none());
    assert_eq!(
        f.storage_life.as_ref().unwrap().bindings[0].binder,
        StorageBinder::Invocation(0)
    );
    let StorageSource::ReborrowedLeaf { head, .. } =
        &mut f.retained_return_sources.as_mut().unwrap()[0].sources[0]
    else {
        unreachable!()
    };
    head.base = StorageBase::Parameter(1);
    assert!(
        !checked(&module, &f, &actual),
        "equal Type and Fixed class do not make this the actual owner head"
    );
}

#[test]
fn reborrow_weakened_mode_is_explicit_and_cannot_strengthen_a_shared_owner_or_data_leaf() {
    let (mut module, mut f, actual) = borrowed_owner_boundary();
    let mut exclusive = module.dyn_interfaces[0].clone();
    exclusive.id.index += 1;
    let Type::Address { mutable, .. } = &mut exclusive.retained_life.ports[0].leaf else {
        unreachable!()
    };
    *mutable = true;
    exclusive.retained_life.ports[0].mode = StoredBorrowMode::ExclusiveAddress;
    let input = exclusive.id;
    module.dyn_interfaces.push(exclusive);
    let Type::Address { inner, mutable } = &mut f.locals[0].ty else {
        unreachable!()
    };
    **inner = Type::OwnedDyn { interface: input };
    *mutable = false;
    assert!(
        checked(&module, &f, &actual),
        "an explicit shared child may borrow exclusive data through a shared head"
    );
    let Type::Address { mutable, .. } = &mut module.dyn_interfaces[0].retained_life.ports[0].leaf
    else {
        unreachable!()
    };
    *mutable = true;
    module.dyn_interfaces[0].retained_life.ports[0].mode = StoredBorrowMode::ExclusiveAddress;
    assert!(!checked(&module, &f, &actual));
    let Type::Address { mutable, .. } = &mut f.locals[0].ty else {
        unreachable!()
    };
    *mutable = true;
    assert!(checked(&module, &f, &actual));
    let Type::Address { mutable, .. } = &mut module.dyn_interfaces[1].retained_life.ports[0].leaf
    else {
        unreachable!()
    };
    *mutable = false;
    module.dyn_interfaces[1].retained_life.ports[0].mode = StoredBorrowMode::SharedAddress;
    assert!(!checked(&module, &f, &actual));
}

#[test]
fn reborrow_call_retains_distinct_head_and_data_children_without_publishing_head_as_data() {
    let (module, callee, actual) = borrowed_owner_boundary();
    let mut caller = callee.clone();
    caller.parameters.clear();
    caller.locals[0].ty = Type::I32;
    caller.locals[1].ty = Type::I32;
    caller.locals.push(LocalDecl {
        id: LocalId(2),
        name: "actual_owner".into(),
        ty: callee.return_type.clone(),
        mutable: true,
        span: callee.span,
    });
    caller.values = vec![callee.locals[0].ty.clone(), callee.return_type.clone()];
    let root = |id| Source {
        static_dependency: None,
        base: HoldRoot::Local(LocalId(id)),
        retained_port: None,
        leaf: vec![],
        inline: vec![],
        heap_element: None,
        heap_parents: vec![],
    };
    let mut state = Frame {
        locals: vec![Payload::default(); 3],
        ..Frame::default()
    };
    state.locals[2].dyn_ports = dyn_ports::Rows(vec![(
        vec![ContractProjection::RetainedPort(0)],
        dyn_ports::Port {
            permission: Permission {
                sources: vec![root(0)],
                loans: vec![0],
            },
            parent_sources: vec![],
            head_sources: vec![],
            known: true,
        },
    )]);
    let mut values = vec![Payload::default(); 2];
    values[0].address = Permission {
        sources: vec![root(2)],
        loans: vec![1],
    };
    let mut loans = vec![
        Loan {
            sources: vec![root(0)],
            ..Loan::default()
        },
        Loan {
            sources: vec![root(2)],
            exclusive: true,
            ..Loan::default()
        },
    ];
    let summary = Summary {
        complete: true,
        dyn_ports: actual,
        ..Summary::default()
    };
    let mut b = Budget::new(&Limits::default());
    b.dyn_contexts = true;
    let i = Instruction {
        result: Some(ValueId(1)),
        kind: InstructionKind::Call {
            function: callee.id,
            arguments: vec![ValueId(0), ValueId(0)],
        },
        span: callee.span,
    };
    let mut sites = dyn_leases::Sites::default();
    let mut out = Payload::default();
    dyn_ports::call(
        &module,
        &caller,
        &callee,
        &i,
        &summary,
        &[ValueId(0), ValueId(0)],
        &values,
        &state,
        &mut out,
        &mut sites,
        &mut loans,
        &mut b,
    )
    .unwrap();
    let permission = &out.dyn_ports.0[0].1.permission;
    assert_eq!(permission.sources, [root(0)]);
    assert!(
        !permission.sources.contains(&root(2)),
        "head is not a data target"
    );
    let active = ancestry(&permission.loans, &loans, &mut b).unwrap();
    assert!(
        access(
            &values[0].address,
            true,
            &active,
            &loans,
            &mut vec![],
            &mut b
        )
        .is_err(),
        "original &mut head SSA cannot delegate its own parent through the live view"
    );
    assert!(access(&values[0].address, true, &[], &loans, &mut vec![], &mut b).is_ok());
    assert_eq!(
        sites.head_sources(permission, &loans, &mut b).unwrap(),
        [root(2)]
    );
    // Only actual head-site roles enter this additional symbolic directory.
    let ordinary = Permission {
        sources: vec![root(0)],
        loans: vec![0],
    };
    assert!(
        sites
            .head_sources(&ordinary, &loans, &mut b)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn reborrow_nested_heads_keep_their_complete_physical_owner_position() {
    let (module, mut f, mut actual) = borrowed_owner_boundary();
    let owner = f.return_type.clone();
    f.locals[0].ty = Type::Address {
        mutable: true,
        inner: Box::new(Type::Tuple(vec![owner.clone(), owner])),
    };
    let life = f.storage_life.as_mut().unwrap();
    life.classes[0]
        .position
        .path
        .push(LifeTypeProjection::Place(PlaceProjection::Field(0)));
    let mut sibling = life.classes[0].clone();
    sibling.position.path[1] = LifeTypeProjection::Place(PlaceProjection::Field(1));
    life.classes.insert(1, sibling);
    let StorageSource::ReborrowedLeaf { source, head } =
        &mut f.retained_return_sources.as_mut().unwrap()[0].sources[0]
    else {
        unreachable!()
    };
    source
        .path
        .insert(0, ContractProjection::Place(PlaceProjection::Field(0)));
    head.path
        .push(ContractProjection::Place(PlaceProjection::Field(0)));
    let Some(PortOrigin::Dyn { carrier, .. }) =
        &mut actual.0[0].1.permission.sources[0].retained_port
    else {
        unreachable!()
    };
    carrier.push(ContractProjection::Place(PlaceProjection::Field(0)));
    assert!(checked(&module, &f, &actual));
    let (_, _, selected) = atoms::resolve_output(
        &module,
        &f,
        &f.retained_return_sources.as_ref().unwrap()[0].sources[0],
        &module.dyn_interfaces[0].retained_life.ports[0].leaf,
        &mut Budget::new(&Limits::default()),
    )
    .unwrap();
    assert_eq!(
        selected.parent_head().unwrap().inline,
        [PlaceProjection::Field(0)]
    );
    assert!(selected.parent_head().unwrap().retained_port.is_none());
    let original = f.clone();
    let StorageSource::ReborrowedLeaf { head, .. } =
        &mut f.retained_return_sources.as_mut().unwrap()[0].sources[0]
    else {
        unreachable!()
    };
    head.path[0] = ContractProjection::Place(PlaceProjection::Field(1));
    assert!(
        !checked(&module, &f, &actual),
        "same-type sibling cannot stand in for the actual held source"
    );
    let mut forged = original;
    let StorageSource::ReborrowedLeaf { head, .. } =
        &mut forged.retained_return_sources.as_mut().unwrap()[0].sources[0]
    else {
        unreachable!()
    };
    head.path = vec![ContractProjection::RetainedPort(0)];
    assert!(
        !checked(&module, &forged, &actual),
        "metadata port cannot become an owner byte address"
    );
}

#[test]
fn reborrow_slice_preserves_the_full_leaf_and_does_not_hide_element_classes() {
    let (mut module, f, actual) = borrowed_owner_boundary();
    let Type::Address { inner, .. } = &mut module.dyn_interfaces[0].retained_life.ports[0].leaf
    else {
        unreachable!()
    };
    **inner = Type::Slice {
        element: Box::new(Type::U8),
    };
    assert!(checked(&module, &f, &actual));
    let Type::Address { inner, .. } = &mut module.dyn_interfaces[0].retained_life.ports[0].leaf
    else {
        unreachable!()
    };
    **inner = Type::Slice {
        element: Box::new(Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        }),
    };
    assert!(
        !checked(&module, &f, &actual),
        "borrowed elements need their actual class occurrence maps"
    );
}

#[test]
fn head_role_survives_a_summary_even_when_the_forwarder_upper_is_exact() {
    let (module, callee, actual) = borrowed_owner_boundary();
    let mut wrapper = callee.clone();
    wrapper.values = vec![callee.locals[0].ty.clone(), callee.return_type.clone()];
    let head = Source {
        static_dependency: None,
        base: HoldRoot::Parameter(0),
        retained_port: None,
        leaf: vec![],
        inline: vec![],
        heap_element: None,
        heap_parents: vec![],
    };
    let mut data = head.clone();
    data.retained_port = Some(PortOrigin::Dyn {
        carrier: vec![],
        port: 0,
    });
    let frame = || {
        let mut state = Frame {
            external: vec![Payload::default(); 2],
            ..Frame::default()
        };
        state.external[0].dyn_ports = dyn_ports::Rows(vec![(
            vec![ContractProjection::RetainedPort(0)],
            dyn_ports::Port {
                permission: Permission {
                    sources: vec![data.clone()],
                    loans: vec![0],
                },
                parent_sources: vec![],
                head_sources: vec![],
                known: true,
            },
        )]);
        state
    };
    let values = vec![
        Payload {
            address: Permission {
                sources: vec![head.clone()],
                loans: vec![1],
            },
            ..Payload::default()
        },
        Payload::default(),
    ];
    let initial_loans = vec![
        Loan {
            sources: vec![data.clone()],
            ..Loan::default()
        },
        Loan {
            sources: vec![head.clone()],
            exclusive: true,
            ..Loan::default()
        },
    ];
    let i = Instruction {
        result: Some(ValueId(1)),
        kind: InstructionKind::Call {
            function: callee.id,
            arguments: vec![ValueId(0), ValueId(0)],
        },
        span: callee.span,
    };
    let mut b = Budget::new(&Limits::default());
    b.dyn_contexts = true;
    b.dyn_reborrow_sources = true;
    let mut loans = initial_loans.clone();
    let mut sites = dyn_leases::Sites::default();
    let mut out = Payload::default();
    dyn_ports::call(
        &module,
        &wrapper,
        &callee,
        &i,
        &Summary {
            complete: true,
            dyn_ports: actual,
            ..Summary::default()
        },
        &[ValueId(0), ValueId(0)],
        &values,
        &frame(),
        &mut out,
        &mut sites,
        &mut loans,
        &mut b,
    )
    .unwrap();
    // Only the actual function-local arena establishes this role; neither the
    // public upper nor a generic parent source is accepted as a head proof.
    let mut summary = Summary {
        complete: true,
        ..Summary::default()
    };
    summarize(
        &module,
        &wrapper,
        &out,
        &mut summary,
        &loans,
        &sites,
        &mut b,
    )
    .unwrap();
    assert_eq!(
        summary.dyn_ports.0[0].1.head_sources.as_slice(),
        std::slice::from_ref(&head)
    );
    assert_eq!(
        summary.dyn_ports.0[0].1.permission.sources.as_slice(),
        std::slice::from_ref(&data)
    );
    assert!(summary.dyn_ports.0[0].1.permission.loans.is_empty());
    wrapper.retained_return_sources.as_mut().unwrap()[0].sources =
        vec![StorageSource::Exact(StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![ContractProjection::RetainedPort(0)],
        })];
    // A separate caller has a new loan arena. It must establish a new head
    // child even though its immediate callee advertises an Exact data upper.
    let mut loans = initial_loans;
    let mut sites = dyn_leases::Sites::default();
    let mut out = Payload::default();
    dyn_ports::call(
        &module,
        &wrapper,
        &wrapper,
        &i,
        &summary,
        &[ValueId(0), ValueId(0)],
        &values,
        &frame(),
        &mut out,
        &mut sites,
        &mut loans,
        &mut b,
    )
    .unwrap();
    let permission = &out.dyn_ports.0[0].1.permission;
    assert_eq!(permission.sources, [data]);
    assert_eq!(
        sites.head_sources(permission, &loans, &mut b).unwrap(),
        [head]
    );
    let active = ancestry(&permission.loans, &loans, &mut b).unwrap();
    assert!(
        access(
            &values[0].address,
            true,
            &active,
            &loans,
            &mut vec![],
            &mut b
        )
        .is_err()
    );
    assert!(access(&values[0].address, true, &[], &loans, &mut vec![], &mut b).is_ok());
}

#[test]
fn actual_static_target_or_empty_data_does_not_discharge_a_retained_activation_head() {
    let (module, mut f, mut actual) = boundary();
    // The output class is not supplied by either input scope. Static data
    // alone cannot erase an additional activation-dependent parent loan.
    let c = f.storage_life.as_mut().unwrap();
    c.fixed_binder_count = 1;
    c.classes[0].binder = StorageBinder::Fixed(0);
    f.retained_return_sources.as_mut().unwrap()[0].sources = vec![StorageSource::Static {
        leaf: module.dyn_interfaces[0].retained_life.ports[0].leaf.clone(),
        mode: StoredBorrowMode::SharedAddress,
    }];
    let mut static_source = actual.0[0].1.permission.sources[0].clone();
    static_source.base = HoldRoot::Static;
    for head in [HoldRoot::Parameter(0), HoldRoot::Local(LocalId(0))] {
        let mut head_source = static_source.clone();
        head_source.base = head;
        let loans = vec![
            Loan {
                sources: vec![head_source.clone()],
                exclusive: true,
                ..Loan::default()
            },
            Loan {
                sources: vec![static_source.clone()],
                parents: vec![0],
                ..Loan::default()
            },
        ];
        actual.0[0].1.permission = Permission {
            sources: vec![static_source.clone()],
            loans: vec![1],
        };
        let mut b = Budget::new(&Limits::default());
        b.dyn_contexts = true;
        pending::register(&module, &f, &mut b).unwrap();
        // Reconstruct actual ancestry from the loan arena. The source upper
        // supplies neither this head fact nor the static data fact.
        actual.0[0].1.parent_sources = parents(&actual.0[0].1.permission, &loans, &mut b).unwrap();
        assert!(actual.0[0].1.parent_sources.contains(&head_source));
        assert!(pending::discharge(&f, &actual, &mut b).is_err());
        assert_eq!(
            b.dyn_static_classes[0].grounding,
            DynClassGrounding::Unknown
        );
        actual.0[0].1.permission.sources.clear();
        assert!(
            pending::discharge(&f, &actual, &mut b).is_err(),
            "empty data cannot erase the actual retained head"
        );
        // Drop only the additional head dependency, retaining the exact same
        // static data target and original output class. This now proves Static.
        actual.0[0].1.permission.sources = vec![static_source.clone()];
        actual.0[0].1.parent_sources = vec![static_source.clone()];
        pending::discharge(&f, &actual, &mut b).unwrap();
        assert_eq!(b.dyn_static_classes[0].grounding, DynClassGrounding::Static);
    }
}
