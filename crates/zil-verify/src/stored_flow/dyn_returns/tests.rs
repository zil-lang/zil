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
    exact_source_mut(&mut forged.retained_return_sources.as_mut().unwrap()[0].sources[0]).base =
        StorageBase::Parameter(1);
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
            sources: vec![data.clone()],
            parents: vec![1],
            exclusive: false,
        },
        Loan {
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
