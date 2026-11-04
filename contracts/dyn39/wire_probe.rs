use zil_base::{Fingerprint, Limits, ModuleId, NodeId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;

fn fixture() -> TypedModule {
    let module = ModuleId(7);
    let declaration = LifeDeclarationId {
        module,
        declaration: NodeId(3),
        compiler: false,
    };
    let interface = DynInterfaceId { module, index: 0 };
    let vtable = DynVtableId { module, index: 0 };
    let application = DynTraitApplication {
        declaration,
        actuals: vec![
            DynGenericActual::Type(Type::Bool),
            DynGenericActual::Const(Constant::U64(u64::MAX)),
            DynGenericActual::Life(StorageBinder::Fixed(0)),
        ],
    };
    let position = StoragePosition {
        base: StorageBase::Parameter(0),
        path: vec![ContractProjection::RetainedPort(0)],
    };
    let binding = StorageLifeContract {
        fixed_binder_count: 1,
        invocation_binder_count: 1,
        bindings: vec![StorageLifeBinding {
            position: StoragePosition {
                base: StorageBase::Parameter(0),
                path: vec![ContractProjection::RetainedClass(0)],
            },
            binder: StorageBinder::Fixed(0),
        }],
    };
    let address = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    let mut result = TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: module,
        source_fingerprint: Fingerprint::of(b"dyn39-raw-codec"),
        target: TargetProfile::host_llvm(),
        life_declarations: vec![],
        life_instances: vec![],
        dyn_declarations: vec![DynDeclaration {
            id: declaration,
            identity: LifeDeclarationIdentity {
                package: "probe".into(),
                module_path: vec!["read".into()],
                declaration_path: vec!["Read".into()],
                generation_path: vec![],
            },
            kind: DynDeclarationKind::Trait,
            parameters: vec![
                LifeParameterKind::Type,
                LifeParameterKind::Const(Type::U64),
                LifeParameterKind::Lifetime,
            ],
            members: vec![DynMemberDeclaration {
                name: "read".into(),
                kind: DynMemberKind::Method(vec![]),
            }],
        }],
        dyn_interfaces: vec![DynInterface {
            id: interface,
            application: application.clone(),
            inherited: vec![],
            associated_types: vec![],
            associated_constants: vec![],
            retained_life: RetainedLifeShape {
                binder_count: 1,
                ports: vec![RetainedBorrowPort {
                    leaf: address.clone(),
                    mode: StoredBorrowMode::SharedAddress,
                    binder: 0,
                }],
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
                    mutable: false,
                    mode: PassingMode::Borrow,
                },
                parameters: vec![],
                parameter_modes: vec![],
                result: Type::I32,
                result_mode: PassingMode::Copy,
                call_kind: CallKind::Direct,
                invocation_life: Some(FunctionLifeContract {
                    binder_count: 1,
                    parameter_lives: vec![Some(0)],
                    result_life: None,
                }),
                storage_life: Some(binding),
                contract: DynContract {
                    requires: Some(vec![
                        DynRequirement::Read(position.clone()),
                        DynRequirement::Write(position.clone()),
                        DynRequirement::Manage(position.clone()),
                        DynRequirement::Apart(position.clone(), position.clone()),
                        DynRequirement::State {
                            position: position.clone(),
                            ty: Type::I32,
                            variant: 0,
                        },
                    ]),
                    guarantees: Some(vec![
                        DynGuarantee::ManageResult,
                        DynGuarantee::State {
                            position: position.clone(),
                            ty: Type::I32,
                            variant: 0,
                        },
                    ]),
                    hold: Some(vec![position.clone()]),
                    call: Some(DynCallAccess {
                        reads: vec![position.clone()],
                        writes: vec![],
                    }),
                    result: Some(DynResultSource::Borrow {
                        sources: vec![position.clone()],
                        mutable: false,
                    }),
                    cleanup: Some(DynCleanupContract {
                        environment: true,
                        positions: vec![position.clone()],
                        access: DynCallAccess {
                            reads: vec![],
                            writes: vec![position.clone()],
                        },
                        effects: vec![DynEffect::Cleanup(Type::I32)],
                    }),
                    effects: Some(vec![
                        DynEffect::Alloc,
                        DynEffect::Io,
                        DynEffect::Block,
                        DynEffect::Suspend,
                        DynEffect::Spawn,
                        DynEffect::Ffi,
                        DynEffect::Reenter,
                        DynEffect::Panic,
                        DynEffect::Call(position.clone()),
                        DynEffect::Cleanup(Type::I32),
                    ]),
                    resume: Some(DynResumeBound::Transfer),
                    threads: Some(DynThreadRequirements {
                        transfer: true,
                        share: true,
                    }),
                },
            }],
        }],
        dyn_vtables: vec![DynVtable {
            id: vtable,
            interface,
            self_type: Type::I32,
            implementation: declaration,
            actuals: vec![],
            ports: vec![RetainedPortBinding {
                port: 0,
                source: StoragePosition {
                    base: StorageBase::Capture(0),
                    path: vec![],
                },
            }],
            entries: vec![DynEntry {
                slot: 0,
                function: FunctionId(1),
            }],
            cleanup: Some(DynEntry {
                slot: 0,
                function: FunctionId(1),
            }),
        }],
        functions: vec![],
        closures: vec![],
        resources: vec![],
        unsupported: vec![],
    };
    let fat = Type::Address {
        mutable: false,
        inner: Box::new(Type::Dyn { interface }),
    };
    result.functions.push(TypedFunction {
        storage_life: None,
        call_kind: CallKind::Direct,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        id: FunctionId(0),
        name: "transport".into(),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0)],
        return_type: Type::I32,
        result_mode: PassingMode::Copy,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "self".into(),
            ty: address.clone(),
            mutable: false,
            span: Span::default(),
        }],
        values: vec![address, fat, Type::I32],
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
                    kind: InstructionKind::MakeDynBorrow {
                        interface,
                        vtable,
                        source: ValueId(0),
                        mutable: false,
                    },
                    span: Span::default(),
                },
                Instruction {
                    result: Some(ValueId(2)),
                    kind: InstructionKind::CallDynSlot {
                        callee: ValueId(1),
                        interface,
                        slot: 0,
                        arguments: vec![],
                    },
                    span: Span::default(),
                },
            ],
            terminator: Terminator::Return(Some(ValueId(2))),
        }],
        entry: BlockId(0),
        span: Span::default(),
    });
    result
}

#[test]
fn dyn39_raw_tables_ops_and_unknown_claims_roundtrip() {
    // Intentionally untrusted candidate exercises transport of all claim tags.
    // It is not a safety witness and is never passed to a backend or VM.
    let limits = Limits::default();
    let module = fixture();
    let plan = CandidatePlan::empty_for(&module);
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan)
    );
    let mut unknown = module.clone();
    unknown.dyn_interfaces[0].slots[0].contract.hold = None;
    assert_ne!(unknown.fingerprint(), module.fingerprint());
    let plan = CandidatePlan::empty_for(&unknown);
    let encoded = wire::encode_module_candidate(&unknown, &plan, &limits).unwrap();
    let (decoded, _) = wire::decode_module_candidate(&encoded, &limits).unwrap();
    assert!(decoded.dyn_interfaces[0].slots[0].contract.hold.is_none());
    let mut changed = module.clone();
    changed.dyn_interfaces[0].application.actuals[1] =
        DynGenericActual::Const(Constant::U64(u64::MAX - 1));
    assert_ne!(changed.fingerprint(), module.fingerprint());
    for cut in [0, bytes.len() / 2, bytes.len() - 1] {
        assert!(wire::decode_module_candidate(&bytes[..cut], &limits).is_err());
    }
    let mut old_version = bytes.clone();
    old_version[8..12].copy_from_slice(&38u32.to_le_bytes());
    assert!(wire::decode_module_candidate(&old_version, &limits).is_err());
}

#[test]
fn dyn39_raw_codec_charges_exact_node_boundary() {
    let module = fixture();
    let plan = CandidatePlan::empty_for(&module);
    let mut limits = Limits::default();
    let (mut low, mut high) = (0, 10000);
    while low < high {
        let middle = low + (high - low) / 2;
        limits.max_nodes = middle;
        if wire::encode_module_candidate(&module, &plan, &limits).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert!(low > 0 && low < 10000);
    limits.max_nodes = low;
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    limits.max_nodes = low - 1;
    assert!(wire::encode_module_candidate(&module, &plan, &limits).is_err());
    let (mut bottom, mut top) = (0, 10000);
    while bottom < top {
        let middle = bottom + (top - bottom) / 2;
        limits.max_nodes = middle;
        if wire::decode_module_candidate(&bytes, &limits).is_ok() {
            top = middle
        } else {
            bottom = middle + 1
        }
    }
    limits.max_nodes = bottom;
    assert!(wire::decode_module_candidate(&bytes, &limits).is_ok());
    limits.max_nodes = bottom - 1;
    assert!(wire::decode_module_candidate(&bytes, &limits).is_err());
    println!("encode_nodes={low} decode_nodes={bottom}");
}
