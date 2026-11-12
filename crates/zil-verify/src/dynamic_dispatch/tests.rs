use super::*;
use zil_base::{ModuleId, NodeId};

fn span() -> Span {
    Span::default()
}

fn declaration(id: u32, name: &str, kind: DynDeclarationKind) -> DynDeclaration {
    DynDeclaration {
        implicit_lifetimes: None,
        direct_supertraits: None,
        requirements: None,
        associated_definitions: None,
        method_signatures: None,
        method_definitions: None,
        self_pattern: None,
        implemented_trait: (kind == DynDeclarationKind::Impl).then_some(DynSelfPattern {
            declaration: LifeDeclarationId {
                module: ModuleId(0),
                declaration: NodeId(1),
                compiler: false,
            },
            arguments: vec![],
        }),
        id: LifeDeclarationId {
            module: ModuleId(0),
            declaration: NodeId(id),
            compiler: false,
        },
        identity: LifeDeclarationIdentity {
            package: "test".into(),
            module_path: vec![],
            declaration_path: vec![name.into()],
            generation_path: vec![],
        },
        kind,
        parameters: vec![],
        members: vec![DynMemberDeclaration {
            name: "read".into(),
            kind: DynMemberKind::Method(vec![]),
        }],
    }
}

fn fixture() -> TypedModule {
    let (mut module, _) = crate::tests::baseline();
    let trait_decl = declaration(1, "Read", DynDeclarationKind::Trait);
    let impl_decl = declaration(2, "ReadForA", DynDeclarationKind::Impl);
    let interface_id = DynInterfaceId {
        module: ModuleId(0),
        index: 0,
    };
    let table_id = DynVtableId {
        module: ModuleId(0),
        index: 0,
    };
    let self_type = Type::Record {
        name: "A".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    let application = DynTraitApplication {
        declaration: trait_decl.id,
        actuals: vec![],
    };
    let life = StorageLifeContract {
        static_bindings: Vec::new(),
        fixed_binder_count: 0,
        invocation_binder_count: 1,
        bindings: vec![StorageLifeBinding {
            position: receiver_position(),
            binder: StorageBinder::Invocation(0),
        }],
        classes: vec![],
    };
    let slot = DynSlot {
        member: DynMember {
            application: application.clone(),
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
        result_mode: PassingMode::Infer,
        call_kind: CallKind::Direct,
        storage_life: Some(life),
        contract: DynContract {
            requires: Some(vec![DynRequirement::Read(receiver_position())]),
            guarantees: Some(vec![]),
            hold: Some(vec![]),
            call: Some(DynCallAccess {
                reads: vec![receiver_position()],
                writes: vec![],
            }),
            result: Some(DynResultSource::Value),
            cleanup: Some(DynCleanupContract {
                environment: false,
                positions: vec![],
                access: DynCallAccess {
                    reads: vec![],
                    writes: vec![],
                },
                effects: vec![],
            }),
            effects: Some(vec![]),
            resume: Some(DynResumeBound::Local),
            threads: Some(DynThreadRequirements {
                transfer: false,
                share: false,
            }),
        },
    };
    let receiver = Type::Address {
        mutable: false,
        inner: Box::new(self_type.clone()),
    };
    module.dyn_declarations = vec![trait_decl, impl_decl.clone()];
    module.dyn_interfaces = vec![DynInterface {
        context: None,
        owning_cleanup: None,
        id: interface_id,
        application,
        inherited: vec![],
        associated_types: vec![],
        associated_constants: vec![],
        retained_life: RetainedLifeShape {
            binder_count: 0,
            ports: vec![],
            retained: vec![],
            nominal_arguments: vec![],
        },
        slots: vec![slot],
    }];
    module.dyn_vtables = vec![DynVtable {
        self_life: None,
        implementation_witnesses: None,
        id: table_id,
        interface: interface_id,
        self_type: self_type.clone(),
        self_classes: Vec::new(),
        implementation: impl_decl.id,
        actuals: vec![],
        ports: vec![],
        entries: vec![DynEntry {
            slot: 0,
            function: FunctionId(1),
        }],
        cleanup: None,
        owned_storage: None,
    }];
    module.functions.push(TypedFunction {
        dyn_instance: None,
        retained_return_sources: None,
        storage_life: None,
        call_kind: CallKind::Direct,
        resume_bound: zil_ir::TaskResumeBound::Local,
        reference_return_bound: None,
        callable_return_contract: None,
        callable_contract: None,
        protocol_contracts: Vec::new(),
        id: FunctionId(1),
        name: "A_read".into(),
        foreign: None,
        environment: None,
        parameters: vec![LocalId(0)],
        return_type: Type::I32,
        result_mode: PassingMode::Infer,
        locals: vec![LocalDecl {
            id: LocalId(0),
            name: "self".into(),
            ty: receiver.clone(),
            mutable: false,
            span: span(),
        }],
        values: vec![
            receiver,
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            Type::I32,
        ],
        blocks: vec![BasicBlock {
            id: BlockId(0),
            instructions: vec![
                Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::Load(LocalId(0)),
                    span: span(),
                },
                Instruction {
                    result: Some(ValueId(1)),
                    kind: InstructionKind::AddressField {
                        address: ValueId(0),
                        index: 0,
                    },
                    span: span(),
                },
                Instruction {
                    result: Some(ValueId(2)),
                    kind: InstructionKind::ReadAddress {
                        address: ValueId(1),
                    },
                    span: span(),
                },
                Instruction {
                    result: None,
                    kind: InstructionKind::StorageDead(LocalId(0)),
                    span: span(),
                },
            ],
            terminator: Terminator::Return(Some(ValueId(2))),
        }],
        entry: BlockId(0),
        span: span(),
    });
    module
}

fn check(module: &TypedModule) -> Check<()> {
    inventory(module, &mut Budget::new(&Limits::default()))
}

#[test]
fn accepts_raw_candidate_with_independent_provider_proof() {
    let (mut module, mut plan) = crate::tests::baseline();
    let dyn_module = fixture();
    module.dyn_declarations = dyn_module.dyn_declarations;
    module.dyn_interfaces = dyn_module.dyn_interfaces;
    module.dyn_vtables = dyn_module.dyn_vtables;
    module.functions.push(dyn_module.functions[1].clone());
    plan.functions.push(FunctionPlan {
        function: FunctionId(1),
        holds: vec![
            ValueHold {
                value: ValueId(0),
                roots: vec![HoldRoot::Parameter(0)],
            },
            ValueHold {
                value: ValueId(1),
                roots: vec![HoldRoot::Parameter(0)],
            },
            ValueHold {
                value: ValueId(2),
                roots: vec![],
            },
        ],
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![LocalId(0)],
            initialized_out: vec![],
        }],
    });
    plan.module_fingerprint = module.fingerprint();
    verify_module(
        module,
        plan,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap();
}

fn call_candidate() -> (TypedModule, CandidatePlan) {
    let (mut module, mut plan) = crate::tests::baseline();
    let dyn_module = fixture();
    module.dyn_declarations = dyn_module.dyn_declarations;
    module.dyn_interfaces = dyn_module.dyn_interfaces;
    module.dyn_vtables = dyn_module.dyn_vtables;
    module.functions.push(dyn_module.functions[1].clone());
    let self_type = module.dyn_vtables[0].self_type.clone();
    let concrete = Type::Address {
        mutable: false,
        inner: Box::new(self_type.clone()),
    };
    let erased = Type::Address {
        mutable: false,
        inner: Box::new(Type::Dyn {
            interface: module.dyn_interfaces[0].id,
        }),
    };
    let main = &mut module.functions[0];
    main.locals[0].ty = self_type.clone();
    main.values = vec![Type::I32, self_type, concrete, erased, Type::I32];
    main.blocks[0].instructions = vec![
        Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::Constant(Constant::I32(20)),
            span: span(),
        },
        Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::MakeAggregate {
                fields: vec![ValueId(0)],
            },
            span: span(),
        },
        Instruction {
            result: None,
            kind: InstructionKind::Store {
                local: LocalId(0),
                value: ValueId(1),
            },
            span: span(),
        },
        Instruction {
            result: Some(ValueId(2)),
            kind: InstructionKind::BorrowLocal { local: LocalId(0) },
            span: span(),
        },
        Instruction {
            result: Some(ValueId(3)),
            kind: InstructionKind::MakeDynBorrow {
                interface: module.dyn_interfaces[0].id,
                vtable: module.dyn_vtables[0].id,
                source: ValueId(2),
                mutable: false,
            },
            span: span(),
        },
        Instruction {
            result: Some(ValueId(4)),
            kind: InstructionKind::CallDynSlot {
                callee: ValueId(3),
                interface: module.dyn_interfaces[0].id,
                slot: 0,
                arguments: vec![],
            },
            span: span(),
        },
    ];
    main.blocks[0].terminator = Terminator::Return(Some(ValueId(4)));
    plan.functions[0].holds = vec![
        ValueHold {
            value: ValueId(0),
            roots: vec![],
        },
        ValueHold {
            value: ValueId(1),
            roots: vec![],
        },
        ValueHold {
            value: ValueId(2),
            roots: vec![HoldRoot::Local(LocalId(0))],
        },
        ValueHold {
            value: ValueId(3),
            roots: vec![HoldRoot::Local(LocalId(0))],
        },
        ValueHold {
            value: ValueId(4),
            roots: vec![],
        },
    ];
    plan.functions.push(FunctionPlan {
        function: FunctionId(1),
        holds: vec![
            ValueHold {
                value: ValueId(0),
                roots: vec![HoldRoot::Parameter(0)],
            },
            ValueHold {
                value: ValueId(1),
                roots: vec![HoldRoot::Parameter(0)],
            },
            ValueHold {
                value: ValueId(2),
                roots: vec![],
            },
        ],
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![LocalId(0)],
            initialized_out: vec![],
        }],
    });
    plan.module_fingerprint = module.fingerprint();
    (module, plan)
}

#[test]
fn accepts_real_dyn_make_and_indirect_call_with_exact_hold() {
    let (module, plan) = call_candidate();
    verify_module(
        module,
        plan,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .unwrap();
}

#[test]
fn rejects_dyn_parent_root_substitution_even_with_fresh_fingerprint() {
    let (module, mut plan) = call_candidate();
    plan.functions[0].holds[3].roots = vec![];
    assert_eq!(
        verify_module(
            module,
            plan,
            &TargetProfile::host_llvm(),
            &Limits::default()
        )
        .unwrap_err()[0]
            .code,
        "E-VERIFY-HOLD"
    );
}

#[test]
fn accepts_real_shared_receiver_body_and_constant_upper_bound() {
    let mut module = fixture();
    check(&module).unwrap();
    module.functions[1].values = vec![Type::I32];
    module.functions[1].blocks[0].instructions = vec![Instruction {
        result: Some(ValueId(0)),
        kind: InstructionKind::Constant(Constant::I32(9)),
        span: span(),
    }];
    module.functions[1].blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    check(&module).unwrap();
}

#[test]
fn follows_actual_default_wrapper_to_finite_literal_target() {
    let mut module = fixture();
    let mut hidden = module.functions[1].clone();
    hidden.id = FunctionId(2);
    hidden.name = "literal_default".into();
    hidden.values = vec![Type::I32];
    hidden.blocks[0].instructions = vec![Instruction {
        result: Some(ValueId(0)),
        kind: InstructionKind::Constant(Constant::I32(9)),
        span: span(),
    }];
    hidden.blocks[0].terminator = Terminator::Return(Some(ValueId(0)));
    module.functions.push(hidden);
    let receiver = module.functions[1].locals[0].ty.clone();
    module.functions[1].values = vec![receiver.clone(), receiver, Type::I32];
    module.functions[1].blocks[0].instructions = vec![
        Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::Load(LocalId(0)),
            span: span(),
        },
        Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::ReborrowAddress {
                address: ValueId(0),
                mutable: false,
            },
            span: span(),
        },
        Instruction {
            result: Some(ValueId(2)),
            kind: InstructionKind::Call {
                function: FunctionId(2),
                arguments: vec![ValueId(1)],
            },
            span: span(),
        },
        Instruction {
            result: None,
            kind: InstructionKind::StorageDead(LocalId(0)),
            span: span(),
        },
    ];
    module.functions[1].blocks[0].terminator = Terminator::Return(Some(ValueId(2)));
    check(&module).unwrap();
    module.functions[2].values = vec![module.functions[2].locals[0].ty.clone(), Type::I32];
    module.functions[2].blocks[0].instructions = vec![
        Instruction {
            result: Some(ValueId(0)),
            kind: InstructionKind::Load(LocalId(0)),
            span: span(),
        },
        Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![ValueId(0)],
            },
            span: span(),
        },
    ];
    module.functions[2].blocks[0].terminator = Terminator::Return(Some(ValueId(1)));
    assert_eq!(check(&module).unwrap_err().code, "E-VERIFY-DYN");
}

#[test]
fn rejects_same_layout_wrong_self_and_unknown_effects() {
    let mut module = fixture();
    module.dyn_vtables[0].self_type = Type::Record {
        name: "B".into(),
        fields: vec![("n".into(), Type::I32)],
    };
    assert_eq!(check(&module).unwrap_err().code, "E-VERIFY-DYN");
    let mut module = fixture();
    module.dyn_interfaces[0].slots[0].contract.effects = None;
    assert_eq!(check(&module).unwrap_err().code, "E-VERIFY-DYN");
}

#[test]
fn rejects_wrong_entry_body_interface_and_call_slot() {
    let mut module = fixture();
    module.functions[1].blocks[0].instructions[2].kind = InstructionKind::WriteAddress {
        address: ValueId(1),
        value: ValueId(2),
    };
    assert_eq!(check(&module).unwrap_err().code, "E-VERIFY-DYN");
    let mut module = fixture();
    module.dyn_vtables[0].interface.index = 1;
    assert_eq!(check(&module).unwrap_err().code, "E-VERIFY-DYN");
    let (mut module, mut plan) = call_candidate();
    let last = module.functions[0].blocks[0]
        .instructions
        .last_mut()
        .unwrap();
    let InstructionKind::CallDynSlot { slot, .. } = &mut last.kind else {
        unreachable!()
    };
    *slot = 1;
    plan.module_fingerprint = module.fingerprint();
    assert_eq!(
        verify_module(
            module,
            plan,
            &TargetProfile::host_llvm(),
            &Limits::default()
        )
        .unwrap_err()[0]
            .code,
        "E-VERIFY-DYN"
    );
}

#[test]
fn rejects_dyn_make_from_unrelated_scalar_operand() {
    let (mut module, mut plan) = call_candidate();
    let make = &mut module.functions[0].blocks[0].instructions[4];
    let InstructionKind::MakeDynBorrow { source, .. } = &mut make.kind else {
        unreachable!()
    };
    *source = ValueId(0);
    plan.module_fingerprint = module.fingerprint();
    assert_eq!(
        verify_module(
            module,
            plan,
            &TargetProfile::host_llvm(),
            &Limits::default()
        )
        .unwrap_err()[0]
            .code,
        "E-VERIFY-DYN"
    );
}

#[test]
fn rejects_unused_bad_table_and_refingerprinted_wire() {
    let (mut module, mut plan) = call_candidate();
    let mut bad = module.dyn_vtables[0].clone();
    bad.id.index = 1;
    bad.entries[0].slot = 1;
    module.dyn_vtables.push(bad);
    assert_eq!(check(&module).unwrap_err().code, "E-VERIFY-DYN");
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    let target = module.target.clone();
    assert_eq!(
        verify_module_bytes(&bytes, &target, &limits).unwrap_err()[0].code,
        "E-VERIFY-DYN"
    );
}

fn reject_relocated_raw_and_wire(module: TypedModule, mut plan: CandidatePlan) {
    plan.module_fingerprint = module.fingerprint();
    let limits = Limits::default();
    let target = module.target.clone();
    assert_eq!(
        verify_module(module.clone(), plan.clone(), &target, &limits).unwrap_err()[0].code,
        "E-VERIFY-DYN"
    );
    let bytes = zil_ir::wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        verify_module_bytes(&bytes, &target, &limits).unwrap_err()[0].code,
        "E-VERIFY-DYN"
    );
}

#[test]
fn rejects_relocated_interface_and_table_module_with_matching_raw_references() {
    let (mut module, plan) = call_candidate();
    module.dyn_interfaces[0].id.module = ModuleId(7);
    module.dyn_vtables[0].interface.module = ModuleId(7);
    let Type::Address { inner, .. } = &mut module.functions[0].values[3] else {
        unreachable!()
    };
    let Type::Dyn { interface } = inner.as_mut() else {
        unreachable!()
    };
    interface.module = ModuleId(7);
    let InstructionKind::MakeDynBorrow { interface, .. } =
        &mut module.functions[0].blocks[0].instructions[4].kind
    else {
        unreachable!()
    };
    interface.module = ModuleId(7);
    let InstructionKind::CallDynSlot { interface, .. } =
        &mut module.functions[0].blocks[0].instructions[5].kind
    else {
        unreachable!()
    };
    interface.module = ModuleId(7);
    reject_relocated_raw_and_wire(module, plan);

    let (mut module, plan) = call_candidate();
    module.dyn_vtables[0].id.module = ModuleId(7);
    let InstructionKind::MakeDynBorrow { vtable, .. } =
        &mut module.functions[0].blocks[0].instructions[4].kind
    else {
        unreachable!()
    };
    vtable.module = ModuleId(7);
    reject_relocated_raw_and_wire(module, plan);
}

#[test]
fn context_directory_uses_complete_leaf_mode_and_fixed_class() {
    let mut module = fixture();
    let leaf = Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    };
    let context = Type::BorrowedStorage {
        storage: Box::new(Type::Record {
            name: "Context".into(),
            fields: vec![("one".into(), leaf.clone()), ("two".into(), leaf.clone())],
        }),
        life: StorageLifeShape {
            binder_count: 1,
            slots: (0..2)
                .map(|index| StorageLifeSlot {
                    path: vec![PlaceProjection::Field(index)],
                    binder: 0,
                    mode: StoredBorrowMode::SharedAddress,
                })
                .collect(),
            retained: vec![],
            nominal_arguments: vec![],
        },
    };
    module.life_instances.push(LifeNominalInstance {
        declaration: module.dyn_declarations[0].id,
        ty: context.clone(),
        arguments: vec![],
    });
    module.dyn_interfaces[0].context = Some(context);
    module.dyn_interfaces[0].retained_life = RetainedLifeShape {
        binder_count: 1,
        ports: vec![RetainedBorrowPort {
            leaf,
            binder: 0,
            mode: StoredBorrowMode::SharedAddress,
        }],
        retained: vec![],
        nominal_arguments: vec![],
    };
    let validate = |m: &TypedModule| {
        crate::dynamic_context::validate(
            m,
            &m.dyn_interfaces[0],
            &mut Budget::new(&Limits::default()),
        )
    };
    validate(&module).unwrap();
    for mutation in 0..6 {
        let mut invalid = module.clone();
        match mutation {
            0 => {
                invalid.dyn_interfaces[0].retained_life.ports[0].mode =
                    StoredBorrowMode::ExclusiveAddress
            }
            1 => invalid.dyn_interfaces[0].retained_life.ports[0].binder = 1,
            2 => {
                invalid.dyn_interfaces[0].retained_life.ports[0].leaf = Type::Address {
                    mutable: false,
                    inner: Box::new(Type::U8),
                }
            }
            3 => {
                let duplicate = invalid.dyn_interfaces[0].retained_life.ports[0].clone();
                invalid.dyn_interfaces[0]
                    .retained_life
                    .ports
                    .push(duplicate);
            }
            4 => invalid.life_instances.clear(),
            _ => invalid.dyn_interfaces[0].context = None,
        }
        assert!(validate(&invalid).is_err(), "mutation {mutation}");
    }
}
