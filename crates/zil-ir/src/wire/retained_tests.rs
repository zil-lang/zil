//! Raw data transport regression. Arbitrary rows here assert no kernel admission.
use crate::*;
use zil_base::{Fingerprint, Limits, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
#[test]
fn retained_context_and_ct_transport_preserves_context_and_budgets() {
    assert_eq!(SCHEMA_VERSION, 48);
    assert_eq!(wire::WIRE_VERSION, 48);
    let shape = StorageLifeShape {
        binder_count: 1,
        slots: vec![StorageLifeSlot {
            path: vec![PlaceProjection::Field(0)],
            binder: 0,
            mode: StoredBorrowMode::SharedAddress,
        }],
        retained: vec![StorageRetainedBinding {
            path: vec![
                LifeTypeProjection::Place(PlaceProjection::Field(0)),
                LifeTypeProjection::Pointee,
                LifeTypeProjection::Element,
                LifeTypeProjection::TypeArgument(1),
                LifeTypeProjection::Parameter(2),
                LifeTypeProjection::Result,
                LifeTypeProjection::Port(0),
                LifeTypeProjection::AssociatedType(1),
            ],
            class: 0,
            binder: 0,
        }],
        nominal_arguments: vec![NominalLifeArgument {
            path: vec![LifeTypeProjection::TypeArgument(1)],
            declaration: LifeDeclarationId {
                module: ModuleId(4),
                declaration: zil_base::NodeId(6),
                compiler: false,
            },
            argument: 2,
            binder: 0,
        }],
    };
    let payload = Type::Tuple(vec![
        Type::Address {
            mutable: false,
            inner: Box::new(Type::I64),
        },
        Type::I32,
    ]);
    let stored = Type::BorrowedStorage {
        storage: Box::new(payload),
        life: shape.clone(),
    };
    let address = Type::Address {
        mutable: true,
        inner: Box::new(stored.clone()),
    };
    let contract = StorageLifeContract {
        classes: vec![SignatureLifeClassBinding {
            position: SignatureLifeClass {
                base: StorageBase::Parameter(0),
                path: vec![LifeTypeProjection::Pointee],
                class: 0,
            },
            binder: StorageBinder::Fixed(0),
        }],
        fixed_binder_count: 1,
        invocation_binder_count: 1,
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
                    base: StorageBase::Parameter(0),
                    path: vec![ContractProjection::Place(PlaceProjection::Field(0))],
                },
                binder: StorageBinder::Fixed(0),
            },
        ],
    };
    let mut contract = contract;
    contract.bindings.push(StorageLifeBinding {
        position: StoragePosition {
            base: StorageBase::Capture(0),
            path: vec![ContractProjection::RetainedPort(0)],
        },
        binder: StorageBinder::Fixed(0),
    });
    contract.classes.push(SignatureLifeClassBinding {
        position: SignatureLifeClass {
            base: StorageBase::Capture(0),
            path: vec![],
            class: 0,
        },
        binder: StorageBinder::Fixed(0),
    });
    let f = TypedFunction {
        dyn_instance: None,
        retained_return_sources: None,
        storage_life: Some(contract.clone()),
        call_kind: CallKind::Direct,
        resume_bound: crate::TaskResumeBound::Local,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contracts: Vec::new(),
        id: FunctionId(0),
        name: "probe".into(),
        foreign: None,
        environment: Some(ClosureId(0)),
        parameters: vec![LocalId(0)],
        return_type: Type::Unit,
        result_mode: PassingMode::Infer,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "storage".into(),
            ty: address.clone(),
            mutable: false,
            span: Span::default(),
        }],
        values: vec![
            address,
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I64),
            },
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::Load(LocalId(0)),
                    span: Span::default(),
                },
                Instruction {
                    result: Some(ValueId(1)),
                    kind: InstructionKind::LoadStoredBorrow {
                        storage: ValueId(0),
                        path: vec![PlaceProjection::Field(0)],
                        mode: StoredBorrowLoad::CopyShared,
                    },
                    span: Span::default(),
                },
                Instruction {
                    result: None,
                    kind: InstructionKind::StoreStoredBorrow {
                        storage: ValueId(0),
                        path: vec![PlaceProjection::Field(0)],
                        value: ValueId(1),
                    },
                    span: Span::default(),
                },
            ],
            terminator: Terminator::Return(None),
        }],
        entry: BlockId(0),
        span: Span::default(),
    };
    let declaration = LifeNominalDeclaration {
        id: LifeDeclarationId {
            module: ModuleId(4),
            declaration: zil_base::NodeId(6),
            compiler: false,
        },
        identity: LifeDeclarationIdentity {
            package: "probe".into(),
            module_path: vec!["types".into()],
            declaration_path: vec!["Box".into()],
            generation_path: vec![1, 2],
        },
        kind: LifeNominalKind::Record,
        parameters: vec![
            LifeParameterKind::Lifetime,
            LifeParameterKind::Type,
            LifeParameterKind::Const(Type::I64),
        ],
    };
    let instance = LifeNominalInstance {
        declaration: declaration.id,
        ty: stored.clone(),
        arguments: vec![
            LifeNominalActual::Lifetime(0),
            LifeNominalActual::Type(Type::I64),
            LifeNominalActual::Const(Constant::I64(2)),
        ],
    };

    let dyn_id = DynInterfaceId {
        module: ModuleId(0),
        index: 0,
    };
    let trait_id = LifeDeclarationId {
        module: ModuleId(0),
        declaration: zil_base::NodeId(100),
        compiler: false,
    };
    let application = DynTraitApplication {
        declaration: trait_id,
        actuals: vec![
            DynGenericActual::Life(StorageBinder::Fixed(0)),
            DynGenericActual::Type(Type::I64),
        ],
    };
    let member = DynMember {
        application: application.clone(),
        ordinal: 0,
    };
    let contract_dyn = DynContract {
        requires: None,
        guarantees: None,
        hold: None,
        call: None,
        result: None,
        cleanup: None,
        effects: None,
        resume: None,
        threads: None,
    };
    let slot = DynSlot {
        member: member.clone(),
        actuals: vec![DynGenericActual::Life(StorageBinder::Invocation(0))],
        receiver: DynReceiver {
            mutable: false,
            mode: PassingMode::Infer,
        },
        parameters: vec![Type::Address {
            mutable: false,
            inner: Box::new(Type::I64),
        }],
        parameter_modes: vec![PassingMode::Infer],
        result: Type::Address {
            mutable: false,
            inner: Box::new(Type::I64),
        },
        result_mode: PassingMode::Infer,
        call_kind: CallKind::Direct,
        storage_life: Some(StorageLifeContract {
            classes: vec![SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Capture(0),
                    path: vec![],
                    class: 0,
                },
                binder: StorageBinder::Fixed(0),
            }],
            fixed_binder_count: 1,
            invocation_binder_count: 1,
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
                        base: StorageBase::Result,
                        path: vec![],
                    },
                    binder: StorageBinder::Invocation(0),
                },
            ],
        }),
        contract: contract_dyn,
    };
    let interface = DynInterface {
        context: None,
        owning_cleanup: None,
        id: dyn_id,
        application,
        inherited: vec![],
        associated_types: vec![DynAssociatedType {
            life: StorageLifeShape {
                binder_count: 0,
                slots: vec![],
                retained: vec![],
                nominal_arguments: vec![],
            },
            classes: vec![],
            member,
            ty: Type::I64,
        }],
        associated_constants: vec![],
        retained_life: RetainedLifeShape {
            binder_count: 1,
            ports: vec![RetainedBorrowPort {
                leaf: Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I64),
                },
                mode: StoredBorrowMode::SharedAddress,
                binder: 0,
            }],
            retained: shape.retained.clone(),
            nominal_arguments: shape.nominal_arguments.clone(),
        },
        slots: vec![slot],
    };
    let dyn_declaration = DynDeclaration {
        direct_supertraits: None,
        requirements: None,
        associated_definitions: None,
        method_signatures: None,
        method_definitions: None,
        self_pattern: None,
        implemented_trait: None,
        id: trait_id,
        identity: LifeDeclarationIdentity {
            package: "probe".into(),
            module_path: vec!["traits".into()],
            declaration_path: vec!["Reader".into()],
            generation_path: vec![],
        },
        kind: DynDeclarationKind::Trait,
        parameters: vec![LifeParameterKind::Lifetime, LifeParameterKind::Type],
        members: vec![DynMemberDeclaration {
            name: "read".into(),
            kind: DynMemberKind::Method(vec![LifeParameterKind::Lifetime]),
        }],
    };
    let vtable = DynVtable {
        self_life: None,
        implementation_witnesses: None,
        self_classes: vec![],
        id: DynVtableId {
            module: ModuleId(0),
            index: 0,
        },
        interface: dyn_id,
        self_type: Type::I64,
        implementation: LifeDeclarationId {
            module: ModuleId(0),
            declaration: zil_base::NodeId(101),
            compiler: false,
        },
        actuals: vec![],
        ports: vec![DynRetainedPortBinding {
            port: 0,
            sources: vec![StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![
                    ContractProjection::Place(PlaceProjection::Field(0)),
                    ContractProjection::RetainedPort(0),
                ],
            }],
        }],
        entries: vec![DynEntry {
            slot: 0,
            function: FunctionId(0),
        }],
        cleanup: None,
        owned_storage: None,
    };
    let module = TypedModule {
        dyn_declarations: vec![dyn_declaration],
        dyn_interfaces: vec![interface],
        dyn_vtables: vec![vtable],
        life_declarations: vec![declaration],
        life_instances: vec![instance],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::ZERO,
        target: TargetProfile::default(),
        functions: vec![f],
        closures: vec![ClosureDefinition {
            storage_life: Some(contract),
            id: ClosureId(0),
            function: FunctionId(0),
            captures: vec![stored.clone()],
            parameters: vec![],
            result: Type::Unit,
            kind: CallableKind::Fn,
            lending_captures: vec![],
        }],
        resources: vec![],
        unsupported: vec![],
    };

    let mut module = module;
    module.dyn_vtables[0].self_classes = vec![StorageBinder::Fixed(0)];
    module.dyn_interfaces[0].context = Some(stored.clone());
    module.dyn_interfaces[0]
        .retained_life
        .ports
        .push(RetainedBorrowPort {
            leaf: Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            mode: StoredBorrowMode::SharedAddress,
            binder: 0,
        });
    module.dyn_vtables[0].ports[0]
        .sources
        .push(StoragePosition {
            base: StorageBase::Parameter(1),
            path: vec![],
        });
    module.dyn_vtables[0].ports.push(DynRetainedPortBinding {
        port: 1,
        sources: vec![],
    });
    module.functions[0].retained_return_sources = Some(vec![StorageSourceBinding {
        position: StoragePosition {
            base: StorageBase::Result,
            path: vec![ContractProjection::RetainedPort(0)],
        },
        sources: (vec![StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![
                ContractProjection::Capture(0),
                ContractProjection::Place(PlaceProjection::Field(0)),
            ],
        }])
        .into_iter()
        .map(zil_ir::StorageSource::Exact)
        .collect(),
    }]);
    let retained = RetainedLifeShape {
        binder_count: 1,
        ports: vec![RetainedBorrowPort {
            leaf: Type::Address {
                mutable: false,
                inner: Box::new(Type::I64),
            },
            mode: StoredBorrowMode::SharedAddress,
            binder: 0,
        }],
        retained: vec![StorageRetainedBinding {
            path: vec![LifeTypeProjection::Result],
            class: 0,
            binder: 0,
        }],
        nominal_arguments: shape.nominal_arguments.clone(),
    };
    let task = Type::Task {
        result: Box::new(stored.clone()),
        retained_life: retained,
    };
    let closed = Type::Task {
        result: Box::new(Type::I32),
        retained_life: RetainedLifeShape {
            binder_count: 0,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
    };
    assert!(
        Type::Address {
            mutable: true,
            inner: Box::new(Type::Managed(Box::new(task.clone())))
        }
        .is_exclusive_task_head_address()
    );
    assert!(
        !Type::Address {
            mutable: false,
            inner: Box::new(Type::Managed(Box::new(task.clone())))
        }
        .is_exclusive_task_head_address()
    );
    assert_ne!(closed.fingerprint(), Type::TaskI32.fingerprint());
    module.closures[0]
        .captures
        .push(Type::Managed(Box::new(task.clone())));
    module.closures[0]
        .captures
        .push(Type::Managed(Box::new(closed)));
    module.functions[0].blocks[0]
        .instructions
        .push(Instruction {
            result: None,
            kind: InstructionKind::TaskCreate {
                worker: FunctionId(0),
                arguments: vec![ValueId(0), ValueId(1)],
                port_bindings: vec![],
            },
            span: Span::default(),
        });
    for n in 0..7 {
        let mut changed = task.clone();
        let Type::Task {
            result,
            retained_life: life,
        } = &mut changed
        else {
            unreachable!()
        };
        match n {
            0 => **result = Type::I32,
            1 => life.binder_count = 2,
            2 => life.ports[0].binder = 1,
            3 => life.ports[0].mode = StoredBorrowMode::ExclusiveAddress,
            4 => life.retained[0].binder = 1,
            5 => life.nominal_arguments[0].argument = 4,
            _ => {
                life.ports[0].leaf = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                }
            }
        };
        assert_ne!(changed.fingerprint(), task.fingerprint());
    }
    for n in 0..4 {
        let mut changed = module.clone();
        let InstructionKind::TaskCreate {
            worker,
            arguments,
            port_bindings,
        } = &mut changed.functions[0].blocks[0]
            .instructions
            .last_mut()
            .unwrap()
            .kind
        else {
            unreachable!()
        };
        match n {
            0 => *worker = FunctionId(1),
            1 => arguments.reverse(),
            2 => {
                arguments.push(ValueId(0));
            }
            _ => port_bindings.push(RetainedPortBinding {
                port: 0,
                source: StoragePosition {
                    base: StorageBase::Parameter(0),
                    path: vec![],
                },
            }),
        }
        assert_ne!(changed.fingerprint(), module.fingerprint());
    }
    let plan = CandidatePlan {
        schema_version: SCHEMA_VERSION,
        module_fingerprint: module.fingerprint(),
        target_fingerprint: module.target.fingerprint(),
        functions: vec![],
        origins: vec![],
        responsibilities: vec![],
        unsupported: vec![],
    };
    for i in 0..8 {
        let mut changed = module.clone();
        if let Type::BorrowedStorage { life, .. } = &mut changed.closures[0].captures[0] {
            match i {
                0 => life.retained[0].class = 1,
                1 => life.retained[0].binder = 1,
                2 => life.retained[0].path = vec![LifeTypeProjection::Element],
                3 => life.nominal_arguments[0].argument = 3,
                4 => life.nominal_arguments[0].binder = 1,
                5 => life.nominal_arguments[0].declaration.compiler = true,
                6 => life.nominal_arguments[0].declaration.declaration = zil_base::NodeId(7),
                _ => life.nominal_arguments[0].path = vec![LifeTypeProjection::Result],
            }
        }
        assert_ne!(changed.fingerprint(), module.fingerprint());
    }

    for i in 0..3 {
        let mut changed = module.clone();
        match i {
            0 => {
                changed.dyn_interfaces[0].retained_life.ports[0].mode =
                    StoredBorrowMode::ReadOnlyDescription
            }
            1 => changed.dyn_interfaces[0].retained_life.retained[0].class = 1,
            _ => {
                changed.dyn_vtables[0].ports[0].sources[0].path =
                    vec![ContractProjection::RetainedClass(0)]
            }
        };
        assert_ne!(changed.fingerprint(), module.fingerprint());
    }
    for i in 0..5 {
        let mut changed = module.clone();
        match i {
            0 => changed.life_declarations[0].identity.package = "other".into(),
            1 => changed.life_declarations[0].parameters[0] = LifeParameterKind::Type,
            2 => changed.life_instances[0].arguments[0] = LifeNominalActual::Lifetime(1),
            3 => changed.life_instances[0].arguments[1] = LifeNominalActual::Type(Type::I32),
            _ => {
                changed.life_instances[0].arguments[2] = LifeNominalActual::Const(Constant::I64(3))
            }
        };
        assert_ne!(changed.fingerprint(), module.fingerprint());
    }
    let bytes = wire::encode_module_candidate(&module, &plan, &Limits::default()).unwrap();
    let (copy, p) = wire::decode_module_candidate(&bytes, &Limits::default()).unwrap();
    assert_eq!(copy, module);
    assert_eq!(p, plan);
    assert_eq!(copy.fingerprint(), module.fingerprint());
    assert_eq!(
        wire::encode_module_candidate(&copy, &p, &Limits::default()).unwrap(),
        bytes
    );

    for n in 0..8 {
        let mut changed = module.clone();
        match n {
            0 => changed.dyn_interfaces[0].context = None,
            1 => changed.dyn_vtables[0].ports[0]
                .sources
                .pop()
                .map(|_| ())
                .unwrap(),
            2 => {
                changed.dyn_vtables[0].ports.pop();
            }
            3 => changed.functions[0].retained_return_sources = Some(vec![]),
            4 => changed.functions[0].retained_return_sources = None,
            5 => changed.dyn_vtables[0].self_classes.clear(),
            6 => changed.dyn_vtables[0].self_classes[0] = StorageBinder::Fixed(1),
            _ => changed.dyn_vtables[0].self_classes[0] = StorageBinder::Invocation(0),
        }
        assert_ne!(changed.fingerprint(), module.fingerprint());
        let bytes = wire::encode_module_candidate(&changed, &plan, &Limits::default()).unwrap();
        assert_eq!(
            wire::decode_module_candidate(&bytes, &Limits::default())
                .unwrap()
                .0,
            changed
        );
    }

    for n in 0..5 {
        let mut changed = module.clone();
        let contract = changed.functions[0].storage_life.as_mut().unwrap();
        let row = &mut contract.classes[0];
        match n {
            0 => row.position.base = StorageBase::Result,
            1 => row.position.path = vec![LifeTypeProjection::TypeArgument(1)],
            2 => row.position.class = 1,
            3 => row.binder = StorageBinder::Invocation(0),
            _ => contract.classes.clear(),
        }
        assert_ne!(changed.fingerprint(), module.fingerprint());
        let encoded = wire::encode_module_candidate(&changed, &plan, &Limits::default()).unwrap();
        assert_eq!(
            wire::decode_module_candidate(&encoded, &Limits::default())
                .unwrap()
                .0,
            changed
        );
    }
    let mut too_deep = module.clone();
    too_deep.functions[0].storage_life.as_mut().unwrap().classes[0]
        .position
        .path = vec![LifeTypeProjection::Pointee; Limits::default().max_call_depth + 1];
    assert!(wire::encode_module_candidate(&too_deep, &plan, &Limits::default()).is_err());

    for n in 0..2 {
        let mut invalid = module.clone();
        let Type::Managed(payload) = &mut invalid.closures[0].captures[1] else {
            unreachable!()
        };
        let Type::Task {
            retained_life: life,
            ..
        } = payload.as_mut()
        else {
            unreachable!()
        };
        if n == 0 {
            life.binder_count = u32::MAX
        } else {
            life.retained[0].path =
                vec![LifeTypeProjection::Result; Limits::default().max_call_depth + 1]
        };
        assert!(wire::encode_module_candidate(&invalid, &plan, &Limits::default()).is_err());
    }
    let mut deep = Type::I32;
    for _ in 0..20 {
        deep = Type::Task {
            result: Box::new(Type::I32),
            retained_life: RetainedLifeShape {
                binder_count: 1,
                ports: vec![RetainedBorrowPort {
                    leaf: Type::Address {
                        mutable: false,
                        inner: Box::new(deep),
                    },
                    mode: StoredBorrowMode::SharedAddress,
                    binder: 0,
                }],
                retained: vec![],
                nominal_arguments: vec![],
            },
        };
    }
    let mut nesting = module.clone();
    nesting.closures[0]
        .captures
        .push(Type::Managed(Box::new(deep)));
    let larger = Limits {
        max_call_depth: 96,
        ..Limits::default()
    };
    let smaller = Limits {
        max_call_depth: 16,
        ..Limits::default()
    };
    let nestedbytes = wire::encode_module_candidate(&nesting, &plan, &larger).unwrap();
    assert!(wire::decode_module_candidate(&nestedbytes, &smaller).is_err());
    assert!(wire::encode_module_candidate(&nesting, &plan, &smaller).is_err());
    println!(
        "Task39 retained/result/port/nominal/opcode128 identity and shared bounded codec controls passed; raw data only"
    );
    let mut old = bytes.clone();
    old[8..12].copy_from_slice(&38u32.to_le_bytes());
    assert!(wire::decode_module_candidate(&old, &Limits::default()).is_err());
    use ct_artifact::*;
    assert_eq!(CT_ARTIFACT_SCHEMA_VERSION, 8);
    let value = CtValue::Type(CtTypeRef {
        ambient_classes: Vec::new(),
        nodes: vec![
            CtTypeNode::Primitive(CtScalarTy::I64),
            CtTypeNode::Address {
                mutable: false,
                inner: CtTypeNodeId(0),
            },
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Tuple(vec![CtTypeNodeId(1), CtTypeNodeId(2)]),
            CtTypeNode::BorrowedStorage {
                storage: CtTypeNodeId(3),
                life: shape.clone(),
            },
        ],
        root: CtTypeNodeId(4),
    });
    let bytes = ct_wire::encode_value(&value, &Limits::default()).unwrap();
    assert_eq!(
        ct_wire::decode_value(&bytes, &Limits::default()).unwrap(),
        value
    );
    let mut huge = module.clone();
    if let Type::BorrowedStorage { life, .. } = &mut huge.closures[0].captures[0] {
        life.binder_count = u32::MAX;
    }
    assert!(
        wire::encode_module_candidate(&huge, &plan, &Limits::default())
            .unwrap_err()
            .iter()
            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    let mut huge = value.clone();
    let CtValue::Type(ty) = &mut huge else {
        panic!("expected fixture Type");
    };
    let CtTypeNode::BorrowedStorage { life, .. } = &mut ty.nodes[4] else {
        panic!("expected fixture stored type");
    };
    life.binder_count = u32::MAX;
    assert!(ct_wire::encode_value(&huge, &Limits::default()).is_err());
    let mut deep = value;
    let CtValue::Type(ty) = &mut deep else {
        panic!("expected fixture Type");
    };
    let CtTypeNode::BorrowedStorage { life, .. } = &mut ty.nodes[4] else {
        panic!("expected fixture stored type");
    };
    life.slots[0].path = vec![PlaceProjection::Field(0); Limits::default().max_call_depth + 1];
    assert!(ct_wire::encode_value(&deep, &Limits::default()).is_err());
    for life_path in [
        LifeTypeProjection::Pointee,
        LifeTypeProjection::TypeArgument(0),
        LifeTypeProjection::Place(PlaceProjection::Field(0)),
    ] {
        let mut long = module.clone();
        if let Type::BorrowedStorage { life, .. } = &mut long.closures[0].captures[0] {
            life.retained[0].path = vec![life_path; Limits::default().max_call_depth + 1];
        }
        assert!(wire::encode_module_candidate(&long, &plan, &Limits::default()).is_err());
        let mut long = CtValue::Type(CtTypeRef {
            ambient_classes: Vec::new(),
            nodes: vec![
                CtTypeNode::Primitive(CtScalarTy::I64),
                CtTypeNode::BorrowedStorage {
                    storage: CtTypeNodeId(0),
                    life: shape.clone(),
                },
            ],
            root: CtTypeNodeId(1),
        });
        let CtValue::Type(ty) = &mut long else {
            panic!("expected fixture Type");
        };
        let CtTypeNode::BorrowedStorage { life, .. } = &mut ty.nodes[1] else {
            panic!("expected fixture stored type");
        };
        life.nominal_arguments[0].path = vec![life_path; Limits::default().max_call_depth + 1];
        assert!(ct_wire::encode_value(&long, &Limits::default()).is_err());
    }

    let scope = CtLifetimeArgument {
        owner: CtTypeNodeId(4),
        binder: StorageBinder::Fixed(0),
    };
    let symbol = CtSymbolId {
        module: ModuleId(4),
        declaration: zil_base::NodeId(6),
        generation_path: None,
    };
    let context = CtTypeRef {
        ambient_classes: Vec::new(),
        nodes: vec![
            CtTypeNode::Primitive(CtScalarTy::I64),
            CtTypeNode::Address {
                mutable: false,
                inner: CtTypeNodeId(0),
            },
            CtTypeNode::Primitive(CtScalarTy::I32),
            CtTypeNode::Nominal {
                symbol: symbol.clone(),
                arguments: vec![
                    CtTypeArgument::Lifetime(scope),
                    CtTypeArgument::Type(CtTypeNodeId(0)),
                    CtTypeArgument::Scalar(Constant::I64(2)),
                ],
            },
            CtTypeNode::BorrowedStorage {
                storage: CtTypeNodeId(3),
                life: shape.clone(),
            },
        ],
        root: CtTypeNodeId(4),
    };
    let record = CtRecordSchema {
        name: symbol.clone(),
        life_context: Some(context.clone()),
        arguments: vec![
            CtSchemaArgument::Lifetime(scope),
            CtSchemaArgument::TypeInContext(CtTypeNodeId(0)),
            CtSchemaArgument::Scalar(Constant::I64(2)),
        ],
        fields: vec![],
        origin: CtSchemaOrigin::UserPure,
    };
    let enumeration = CtEnumSchema {
        name: symbol.clone(),
        life_context: None,
        arguments: vec![CtSchemaArgument::Type(CtTypeRef {
            ambient_classes: Vec::new(),
            nodes: vec![CtTypeNode::Primitive(CtScalarTy::I64)],
            root: CtTypeNodeId(0),
        })],
        variants: vec![vec![]],
        origin: CtSchemaOrigin::UserPure,
    };
    let function = CtFunction {
        id: CtFunctionId(0),
        definition: symbol,
        parameters: vec![CtLocalId(0)],
        parameter_types: vec![CtTy::Type],
        result_type: CtTy::Type,
        locals: vec![CtTy::Type],
        values: vec![CtTy::Type, CtTy::Type],
        blocks: vec![CtBlock {
            id: CtBlockId(0),
            instructions: vec![
                CtInstruction {
                    result: Some(CtValueId(0)),
                    kind: CtInstructionKind::Load(CtLocalId(0)),
                    span: Span::default(),
                },
                CtInstruction {
                    result: Some(CtValueId(1)),
                    kind: CtInstructionKind::TypeConstruct {
                        constructor: CtTypeConstructor::BorrowedStorage {
                            life: shape.clone(),
                        },
                        arguments: vec![CtValueId(0)],
                    },
                    span: Span::default(),
                },
            ],
            terminator: CtTerminator::Return(Some(CtValueId(1))),
        }],
        entry: CtBlockId(0),
        span: Span::default(),
    };
    let program = CtArtifactProgram {
        dynamic_catalogue: Default::default(),
        ambient_life_scopes: Vec::new(),
        schema_version: CT_ARTIFACT_SCHEMA_VERSION,
        target_pointer_width: 64,
        source_fingerprint: Fingerprint::ZERO,
        functions: vec![function],
        entry: CtFunctionId(0),
        constants: vec![],
        record_schemas: vec![record],
        enum_schemas: vec![enumeration],
        templates: vec![],
        input_tokens: CtInputTable {
            invocation_fingerprint: Fingerprint::ZERO,
            tokens: vec![],
        },
        snapshot: CtSnapshotRequest {
            fingerprint: Fingerprint::ZERO,
            ready: CtReadyStage::Shape,
            target_fingerprint: None,
            requested_queries: vec![],
        },
        budget: CtDeclaredBudget {
            max_steps: 1000,
            max_call_depth: 64,
            max_live_cells: 1000,
            max_temporary_bytes: 100000,
            max_generated_nodes: 1000,
            max_instances: 1000,
            max_analysis_work: 100000,
        },
    };
    let encoded = ct_wire::encode_program(&program, &Limits::default()).unwrap();
    assert_eq!(
        ct_wire::decode_program(&encoded, &Limits::default()).unwrap(),
        program
    );
    let fp = ct_wire::program_fingerprint(&program, &Limits::default()).unwrap();
    for i in 0..3 {
        let mut changed = program.clone();
        match i {
            0 => {
                changed.record_schemas[0].arguments[0] =
                    CtSchemaArgument::Lifetime(CtLifetimeArgument {
                        owner: CtTypeNodeId(3),
                        binder: StorageBinder::Fixed(0),
                    })
            }
            1 => {
                changed.record_schemas[0].arguments[1] =
                    CtSchemaArgument::TypeInContext(CtTypeNodeId(2))
            }
            _ => changed.record_schemas[0].life_context = None,
        };
        assert_ne!(
            ct_wire::program_fingerprint(&changed, &Limits::default()).unwrap(),
            fp
        );
    }
    let limits = Limits::default();
    let mut budget = ct_wire::WireBudget::from_limits(&limits);
    let (_, usage) = ct_wire::encode_program_with_usage(&program, &limits, &mut budget).unwrap();
    let exact = Limits {
        max_analysis_steps: usage.work,
        ..limits.clone()
    };
    assert!(ct_wire::encode_program(&program, &exact).is_ok());
    assert_eq!(
        ct_wire::encode_program(
            &program,
            &Limits {
                max_analysis_steps: usage.work - 1,
                ..limits.clone()
            }
        )
        .unwrap_err(),
        ct_wire::WireError::Limit
    );
    let mut budget = ct_wire::WireBudget::from_limits(&limits);
    let (_, used) = ct_wire::decode_program_with_usage(&encoded, &limits, &mut budget).unwrap();
    let exact = Limits {
        max_analysis_steps: used.work,
        ..limits.clone()
    };
    assert!(ct_wire::decode_program(&encoded, &exact).is_ok());
    assert_eq!(
        ct_wire::decode_program(
            &encoded,
            &Limits {
                max_analysis_steps: used.work - 1,
                ..limits.clone()
            }
        )
        .unwrap_err(),
        ct_wire::WireError::Limit
    );
    let mut too_long = module.clone();
    if let Type::BorrowedStorage { life, .. } = &mut too_long.closures[0].captures[0] {
        life.retained[0].path = vec![LifeTypeProjection::Port(0); limits.max_call_depth + 1];
    }
    let wide = Limits {
        max_call_depth: limits.max_call_depth + 10,
        ..limits.clone()
    };
    let bytes = wire::encode_module_candidate(&too_long, &plan, &wide).unwrap();
    assert!(
        wire::decode_module_candidate(&bytes, &limits)
            .unwrap_err()
            .iter()
            .all(|d| d.category == zil_base::ResultCategory::ResourceLimit)
    );
    let mut too_long = program.clone();
    let Some(context) = &mut too_long.record_schemas[0].life_context else {
        panic!("expected fixture Type");
    };
    let CtTypeNode::BorrowedStorage { life, .. } = &mut context.nodes[4] else {
        panic!("expected fixture stored type");
    };
    life.nominal_arguments[0].path =
        vec![LifeTypeProjection::AssociatedType(0); limits.max_call_depth + 1];
    let bytes = ct_wire::encode_program(&too_long, &wide).unwrap();
    assert_eq!(
        ct_wire::decode_program(&bytes, &limits).unwrap_err(),
        ct_wire::WireError::Limit
    );
    let mut old = encoded.clone();
    old[8..12].copy_from_slice(&3u32.to_le_bytes());
    let n = old.len() - 32;
    let mut hash = zil_base::FingerprintBuilder::new();
    hash.update(CT_ARTIFACT_FINGERPRINT_DOMAIN);
    hash.update(&old[..n]);
    old[n..].copy_from_slice(&hash.digest().0);
    assert_eq!(
        ct_wire::decode_program(&old, &limits).unwrap_err(),
        ct_wire::WireError::BadVersion
    );
    println!(
        "runtime40 full nominal inventory/retained metadata and CT5 joint numeric scopes/constructor12/node16 roundtrip; encoder N{} decoder N{} with N-1 Limit; old versions and long raw paths reject; data only, no kernel authority",
        usage.work, used.work
    );
}
