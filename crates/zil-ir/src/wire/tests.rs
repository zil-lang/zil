use super::*;
use zil_base::ResultCategory;

fn ct() -> CtModule {
    CtModule {
        schema_version: SCHEMA_VERSION,
        input_fingerprint: Fingerprint::of(b"wire test input"),
        target: TargetProfile::host_llvm(),
        instructions: vec![
            CtInstruction {
                result: ValueId(0),
                ty: Type::I64,
                kind: CtInstructionKind::Constant(Constant::I64(-41)),
            },
            CtInstruction {
                result: ValueId(1),
                ty: Type::I64,
                kind: CtInstructionKind::Unary {
                    op: UnaryOp::Negate,
                    operand: ValueId(0),
                },
            },
            CtInstruction {
                result: ValueId(2),
                ty: Type::I64,
                kind: CtInstructionKind::Binary {
                    op: BinaryOp::AddWrapping,
                    left: ValueId(0),
                    right: ValueId(1),
                },
            },
        ],
        result: ValueId(2),
        max_steps: 3,
        dependencies: vec![],
    }
}
fn first_opcode(bytes: &[u8]) -> usize {
    let limits = Limits::default();
    let mut reader = Reader::new(bytes, &limits).unwrap();
    reader.header(CT_MAGIC).unwrap();
    reader.u32().unwrap();
    reader.fingerprint().unwrap();
    reader.target().unwrap();
    reader.u64().unwrap();
    reader.u32().unwrap();
    assert_eq!(reader.u32().unwrap(), 0);
    reader.u32().unwrap();
    reader.u32().unwrap();
    reader.position
}
#[test]
fn ct_roundtrip_is_stable_and_preserves_raw_claims() {
    let limits = Limits::default();
    let original = ct();
    let bytes = encode_ct(&original, &limits).unwrap();
    let recovered = decode_ct(&bytes, &limits).unwrap();
    assert_eq!(recovered, original);
    assert_eq!(encode_ct(&recovered, &limits).unwrap(), bytes);
    let mut false_claim = original;
    false_claim.instructions[2].ty = Type::Bool;
    let decoded = decode_ct(&encode_ct(&false_claim, &limits).unwrap(), &limits).unwrap();
    assert_eq!(
        decoded.instructions[2].ty,
        Type::Bool,
        "decoder must not silently repair candidate facts"
    );
}
#[test]
fn every_truncated_ct_prefix_is_rejected_without_panicking() {
    let limits = Limits::default();
    let bytes = encode_ct(&ct(), &limits).unwrap();
    for length in 0..bytes.len() {
        let error = decode_ct(&bytes[..length], &limits).unwrap_err();
        assert_eq!(
            error[0].category,
            ResultCategory::SafeReject,
            "prefix length {length}"
        );
    }
}
#[test]
fn rejects_unknown_opcode_version_and_missing_typed_claims() {
    let limits = Limits::default();
    let bytes = encode_ct(&ct(), &limits).unwrap();
    let mut unknown = bytes.clone();
    unknown[first_opcode(&bytes)] = 88;
    assert_eq!(
        decode_ct(&unknown, &limits).unwrap_err()[0].code,
        "E-CTIR-OPCODE"
    );
    let mut version = bytes.clone();
    version[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        decode_ct(&version, &limits).unwrap_err()[0].code,
        "E-WIRE-VERSION"
    );
    let marker = bytes
        .windows(4)
        .position(|bytes| bytes == CT_CLAIMS)
        .unwrap();
    assert_eq!(
        decode_ct(&bytes[..marker], &limits).unwrap_err()[0].code,
        "E-CTIR-CERTIFICATE"
    );
    let mut incorrect = bytes.clone();
    incorrect[marker + 4..marker + 8].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        decode_ct(&incorrect, &limits).unwrap_err()[0].code,
        "E-CTIR-CERTIFICATE"
    );
}
#[test]
fn rejects_trailing_bytes_noncanonical_bool_and_unknown_type() {
    let limits = Limits::default();
    let mut module = ct();
    module.instructions[0].kind = CtInstructionKind::Constant(Constant::Bool(true));
    let bytes = encode_ct(&module, &limits).unwrap();
    let opcode = first_opcode(&bytes);
    let mut invalid = bytes.clone();
    invalid[opcode + 2] = 2;
    assert_eq!(
        decode_ct(&invalid, &limits).unwrap_err()[0].code,
        "E-WIRE-BOOL"
    );
    let mut invalid = bytes.clone();
    invalid.push(0);
    assert_eq!(
        decode_ct(&invalid, &limits).unwrap_err()[0].code,
        "E-WIRE-TRAILING"
    );
    let marker = bytes
        .windows(4)
        .position(|bytes| bytes == CT_CLAIMS)
        .unwrap();
    let mut invalid = bytes;
    invalid[marker + 12] = 222;
    assert_eq!(
        decode_ct(&invalid, &limits).unwrap_err()[0].code,
        "E-WIRE-TYPE"
    );
}
#[test]
fn byte_count_node_depth_and_execution_quotas_precede_allocation() {
    let limits = Limits::default();
    let bytes = encode_ct(&ct(), &limits).unwrap();
    for limited in [
        Limits {
            max_source_bytes: bytes.len() - 1,
            ..limits.clone()
        },
        Limits {
            max_instructions: 1,
            ..limits.clone()
        },
        Limits {
            max_nodes: 1,
            ..limits.clone()
        },
        Limits {
            max_call_depth: 0,
            ..limits.clone()
        },
        Limits {
            max_ct_steps: 2,
            ..limits.clone()
        },
    ] {
        assert_eq!(
            decode_ct(&bytes, &limited).unwrap_err()[0].category,
            ResultCategory::ResourceLimit
        );
    }
    let mut absurd_count = bytes.clone();
    let count_offset = first_opcode(&bytes) - 8;
    absurd_count[count_offset..count_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        decode_ct(&absurd_count, &limits).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
    let mut nested = ct();
    nested.instructions[0].ty = Type::Address {
        mutable: false,
        inner: Box::new(Type::Tuple(vec![Type::I32])),
    };
    let bytes = encode_ct(&nested, &limits).unwrap();
    let depth = Limits {
        max_call_depth: 2,
        ..limits
    };
    assert_eq!(
        decode_ct(&bytes, &depth).unwrap_err()[0].category,
        ResultCategory::ResourceLimit
    );
}
#[test]
fn forbidden_ops_and_dependencies_survive_as_untrusted_data() {
    let limits = Limits::default();
    let mut module = ct();
    module.instructions[1].kind = CtInstructionKind::Forbidden("filesystem".into());
    module.dependencies.push(CtDependency {
        name: "pretend-valid".into(),
        fingerprint: Fingerprint::ZERO,
        completed: true,
    });
    let bytes = encode_ct(&module, &limits).unwrap();
    assert_eq!(decode_ct(&bytes, &limits).unwrap(), module);
}
fn module_candidate() -> (TypedModule, CandidatePlan) {
    let span = Span::new(FileId(2), 4, 8);
    let module = TypedModule {
        resources: Vec::new(),
        closures: vec![],
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(9),
        source_fingerprint: Fingerprint::of(b"candidate"),
        target: TargetProfile::host_llvm(),
        functions: vec![TypedFunction {
            callable_return_contract: None,
            callable_contract: None,
            result_mode: PassingMode::Infer,
            foreign: None,
            environment: None,
            id: FunctionId(0),
            name: "identity".into(),
            parameters: vec![LocalId(0)],
            return_type: Type::I32,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "x".into(),
                ty: Type::I32,
                mutable: false,
                span,
            }],
            values: vec![Type::I32],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![Instruction {
                    result: Some(ValueId(0)),
                    kind: InstructionKind::Load(LocalId(0)),
                    span,
                }],
                terminator: Terminator::Return(Some(ValueId(0))),
            }],
            entry: BlockId(0),
            span,
        }],
        unsupported: vec![],
    };
    let mut plan = CandidatePlan::empty_for(&module);
    plan.functions.push(FunctionPlan {
        function: FunctionId(0),
        holds: vec![ValueHold {
            value: ValueId(0),
            roots: vec![],
        }],
        blocks: vec![BlockPlan {
            block: BlockId(0),
            initialized_in: vec![LocalId(0)],
            initialized_out: vec![LocalId(0)],
        }],
    });
    (module, plan)
}
#[test]
fn module_and_candidate_roundtrip_and_every_partial_record_rejects() {
    let limits = Limits::default();
    let (module, plan) = module_candidate();
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        decode_module_candidate(&bytes, &limits).unwrap(),
        (module, plan)
    );
    for length in 0..bytes.len() {
        assert!(
            decode_module_candidate(&bytes[..length], &limits).is_err(),
            "prefix {length}"
        );
    }
    let mut appended = bytes;
    appended.push(0);
    assert_eq!(
        decode_module_candidate(&appended, &limits).unwrap_err()[0].code,
        "E-WIRE-TRAILING"
    );
}

#[test]
fn resource_raw_fields_roundtrip_bind_fingerprint_and_reject_old_wire() {
    // Transport accepts raw claims; this fixture is not a VerifiedModule and
    // intentionally does not claim to prove a complete source resource path.
    let limits = Limits::default();
    let (mut module, _) = module_candidate();
    let payload = Type::Record {
        name: "@module9:Token".into(),
        fields: vec![("code".into(), Type::I32)],
    };
    module.resources.push(ResourceDefinition {
        name: "@module9:Token".into(),
        payload,
        teardown: Some(FunctionId(7)),
    });
    module.functions[0].result_mode = PassingMode::Create;
    let span = module.functions[0].span;
    for kind in [
        InstructionKind::MakeManaged {
            fields: vec![ValueId(0)],
        },
        InstructionKind::DescribeLocal { local: LocalId(2) },
        InstructionKind::ReadDesc {
            desc: ValueId(4),
            index: 5,
        },
        InstructionKind::WriteDesc {
            desc: ValueId(6),
            index: 7,
            value: ValueId(8),
        },
        InstructionKind::DropManaged { local: LocalId(9) },
    ] {
        module.functions[0].blocks[0]
            .instructions
            .push(Instruction {
                result: None,
                kind,
                span,
            });
    }
    let plan = CandidatePlan::empty_for(&module);
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan)
    );
    for length in 0..bytes.len() {
        assert!(decode_module_candidate(&bytes[..length], &limits).is_err());
    }
    for old in 1u32..WIRE_VERSION {
        let mut old_bytes = bytes.clone();
        old_bytes[8..12].copy_from_slice(&old.to_le_bytes());
        assert!(decode_module_candidate(&old_bytes, &limits).is_err());
    }
    let identity = module.fingerprint();
    let mut changed = module.clone();
    changed.resources[0].name.push('x');
    assert_ne!(identity, changed.fingerprint());
    changed = module.clone();
    changed.resources[0].payload = Type::I64;
    assert_ne!(identity, changed.fingerprint());
    changed = module.clone();
    changed.resources[0].teardown = None;
    assert_ne!(identity, changed.fingerprint());
    changed = module;
    changed.functions[0].result_mode = PassingMode::Give;
    assert_ne!(identity, changed.fingerprint());
}

#[test]
fn shared_address_operations_and_raw_hold_claims_roundtrip() {
    let limits = Limits::default();
    let (mut module, mut plan) = module_candidate();
    let span = module.functions[0].span;
    module.functions[0].values.push(Type::Address {
        mutable: false,
        inner: Box::new(Type::I32),
    });
    module.functions[0].values.push(Type::I32);
    module.functions[0].blocks[0].instructions.extend([
        Instruction {
            result: None,
            kind: InstructionKind::StorageLive(LocalId(0)),
            span,
        },
        Instruction {
            result: Some(ValueId(1)),
            kind: InstructionKind::BorrowLocal { local: LocalId(0) },
            span,
        },
        Instruction {
            result: Some(ValueId(2)),
            kind: InstructionKind::ReadAddress {
                address: ValueId(1),
            },
            span,
        },
        Instruction {
            result: None,
            kind: InstructionKind::StorageDead(LocalId(0)),
            span,
        },
    ]);
    // These are intentionally raw claims, including an invalid incoming root.
    // Serialization must preserve them; only independent verification grants trust.
    plan.functions[0].holds.extend([
        ValueHold {
            value: ValueId(1),
            roots: vec![HoldRoot::Local(LocalId(0)), HoldRoot::Capture(7)],
        },
        ValueHold {
            value: ValueId(2),
            roots: vec![],
        },
    ]);
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        decode_module_candidate(&bytes, &limits).unwrap(),
        (module, plan)
    );
    for version in [1u32, 2, 3, 4] {
        let mut old = bytes.clone();
        old[8..12].copy_from_slice(&version.to_le_bytes());
        assert_eq!(
            decode_module_candidate(&old, &limits).unwrap_err()[0].code,
            "E-WIRE-VERSION"
        );
    }
}
#[test]
fn raw_cache_keeps_resource_claims_without_endorsing_them() {
    let limits = Limits::default();
    let (mut module, mut plan) = module_candidate();
    module.functions[0].locals[0].ty = Type::Managed(Box::new(Type::Named("Socket".into())));
    module.unsupported.push(UnsupportedFeature {
        feature: "resource operations".into(),
        span: None,
    });
    plan.origins.push(OriginClaim {
        origin: OriginId(1),
        source: NodeId(2),
        multiplicity: OriginMultiplicity::Many,
    });
    plan.responsibilities.push(ResponsibilityClaim {
        function: FunctionId(0),
        carrier: LocalId(0),
        origin: OriginId(1),
        state: ResponsibilityState::Done,
    });
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        decode_module_candidate(&bytes, &limits).unwrap(),
        (module, plan)
    );
}

#[test]
fn resource_closure_operations_bind_wire_and_fingerprint() {
    let limits = Limits::default();
    let (mut module, _) = module_candidate();
    let span = module.functions[0].span;
    let ops = vec![
        InstructionKind::DescribeCapture { index: 2 },
        InstructionKind::TakeCapture { index: 3 },
        InstructionKind::CallClosureShared {
            callee: LocalId(4),
            arguments: vec![ValueId(5)],
        },
        InstructionKind::CallClosureOnce {
            callee: ValueId(6),
            arguments: vec![ValueId(7)],
        },
        InstructionKind::DropClosure { local: LocalId(8) },
    ];
    for kind in &ops {
        module.functions[0].blocks[0]
            .instructions
            .push(Instruction {
                result: None,
                kind: kind.clone(),
                span,
            });
    }
    let plan = CandidatePlan::empty_for(&module);
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan)
    );
    for length in 0..bytes.len() {
        assert!(decode_module_candidate(&bytes[..length], &limits).is_err());
    }
    let fingerprint = module.fingerprint();
    let first = module.functions[0].blocks[0].instructions.len() - ops.len();
    for index in first..first + ops.len() {
        let mut changed = module.clone();
        changed.functions[0].blocks[0].instructions.remove(index);
        assert_ne!(fingerprint, changed.fingerprint());
    }
    let mut old = bytes;
    old[8..12].copy_from_slice(&6u32.to_le_bytes());
    assert!(decode_module_candidate(&old, &limits).is_err());
}

#[test]
fn error_cleanup_raw_contract_roundtrips_and_binds_each_claim() {
    // Raw transport preserves, but does not trust, scope and error claims.
    let limits = Limits::default();
    let (mut module, _) = module_candidate();
    let span = module.functions[0].span;
    module.functions[0].return_type = Type::ErrorUnion {
        error: Box::new(Type::Bool),
        result: Box::new(Type::I32),
    };
    let union_type = module.functions[0].return_type.clone();
    module.functions[0].values.push(union_type);
    let operations = [
        InstructionKind::ScopeEnter { scope: ScopeId(3) },
        InstructionKind::MakeErrorUnion {
            is_error: false,
            value: ValueId(0),
        },
        InstructionKind::ErrorTag { value: ValueId(1) },
        InstructionKind::ReadErrorUnion {
            value: ValueId(1),
            is_error: false,
        },
        InstructionKind::ScopeExit {
            scope: ScopeId(3),
            kind: CleanupExitKind::Error,
        },
    ];
    for kind in operations {
        module.functions[0].blocks[0]
            .instructions
            .push(Instruction {
                result: None,
                kind,
                span,
            });
    }
    let plan = CandidatePlan::empty_for(&module);
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan)
    );
    for length in 0..bytes.len() {
        assert!(
            decode_module_candidate(&bytes[..length], &limits).is_err(),
            "prefix {length}"
        );
    }
    let original = module.fingerprint();
    let mut changed = module.clone();
    changed.functions[0].return_type = Type::ErrorUnion {
        error: Box::new(Type::I32),
        result: Box::new(Type::Bool),
    };
    assert_ne!(original, changed.fingerprint());
    for (offset, replacement) in [
        InstructionKind::ScopeEnter { scope: ScopeId(4) },
        InstructionKind::MakeErrorUnion {
            is_error: true,
            value: ValueId(0),
        },
        InstructionKind::ErrorTag { value: ValueId(0) },
        InstructionKind::ReadErrorUnion {
            value: ValueId(1),
            is_error: true,
        },
        InstructionKind::ScopeExit {
            scope: ScopeId(3),
            kind: CleanupExitKind::Return,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let mut changed = module.clone();
        changed.functions[0].blocks[0].instructions[1 + offset].kind = replacement;
        assert_ne!(original, changed.fingerprint());
    }
    let mut old = bytes;
    old[8..12].copy_from_slice(&7u32.to_le_bytes());
    assert!(decode_module_candidate(&old, &limits).is_err());
}

#[test]
fn package_wire_preserves_untrusted_shape_and_binds_all_field_operations() {
    let limits = Limits::default();
    let (mut module, _) = module_candidate();
    let span = module.functions[0].span;
    let package = Type::Package {
        name: "ui::Window".into(),
        fields: vec![
            ("counter".into(), Type::I32),
            (
                "surface".into(),
                Type::Managed(Box::new(Type::Record {
                    name: "ui::Surface".into(),
                    fields: vec![("handle".into(), Type::I64)],
                })),
            ),
        ],
    };
    assert!(!package.is_copy_value());
    assert!(package.aggregate_fields().is_none());
    module.functions[0].locals[0].ty = package.clone();
    module.functions[0].values.push(package);
    // Deliberately invalid operands: transport must preserve, not endorse, them.
    let operations = [
        InstructionKind::MakePackage {
            fields: vec![ValueId(7), ValueId(7)],
        },
        InstructionKind::ReadPackageField {
            local: LocalId(9),
            index: 0,
        },
        InstructionKind::DescribePackageField {
            local: LocalId(9),
            index: 1,
        },
        InstructionKind::DropPackage { local: LocalId(9) },
        InstructionKind::TakePackageField {
            local: LocalId(9),
            index: 1,
        },
        InstructionKind::DropPackageField {
            local: LocalId(9),
            index: 1,
        },
        InstructionKind::StorePackageField {
            local: LocalId(9),
            index: 1,
            value: ValueId(7),
        },
    ];
    let offset = module.functions[0].blocks[0].instructions.len();
    for kind in operations {
        module.functions[0].blocks[0]
            .instructions
            .push(Instruction {
                result: None,
                kind,
                span,
            });
    }
    let plan = CandidatePlan::empty_for(&module);
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    let recovered = decode_module_candidate(&bytes, &limits).unwrap();
    assert_eq!(recovered, (module.clone(), plan));
    assert_eq!(
        encode_module_candidate(&recovered.0, &recovered.1, &limits).unwrap(),
        bytes
    );
    for length in 0..bytes.len() {
        assert!(
            decode_module_candidate(&bytes[..length], &limits).is_err(),
            "prefix {length}"
        );
    }
    let fingerprint = module.fingerprint();
    for (index, kind) in [
        InstructionKind::MakePackage {
            fields: vec![ValueId(7), ValueId(8)],
        },
        InstructionKind::ReadPackageField {
            local: LocalId(9),
            index: 1,
        },
        InstructionKind::DescribePackageField {
            local: LocalId(8),
            index: 1,
        },
        InstructionKind::DropPackage { local: LocalId(8) },
        InstructionKind::TakePackageField {
            local: LocalId(9),
            index: 0,
        },
        InstructionKind::DropPackageField {
            local: LocalId(8),
            index: 1,
        },
        InstructionKind::StorePackageField {
            local: LocalId(9),
            index: 1,
            value: ValueId(8),
        },
    ]
    .into_iter()
    .enumerate()
    {
        let mut changed = module.clone();
        changed.functions[0].blocks[0].instructions[offset + index].kind = kind;
        assert_ne!(fingerprint, changed.fingerprint());
    }
    // Every operand of the new operations is part of the trusted snapshot.
    for (index, kinds) in [
        (
            4,
            vec![InstructionKind::TakePackageField {
                local: LocalId(8),
                index: 1,
            }],
        ),
        (
            5,
            vec![InstructionKind::DropPackageField {
                local: LocalId(9),
                index: 0,
            }],
        ),
        (
            6,
            vec![
                InstructionKind::StorePackageField {
                    local: LocalId(8),
                    index: 1,
                    value: ValueId(7),
                },
                InstructionKind::StorePackageField {
                    local: LocalId(9),
                    index: 0,
                    value: ValueId(7),
                },
            ],
        ),
    ] {
        for kind in kinds {
            let mut changed = module.clone();
            changed.functions[0].blocks[0].instructions[offset + index].kind = kind;
            assert_ne!(fingerprint, changed.fingerprint());
        }
    }
    for edit in 0..3 {
        let mut changed = module.clone();
        let Type::Package { name, fields } = &mut changed.functions[0].locals[0].ty else {
            unreachable!()
        };
        match edit {
            0 => name.push('x'),
            1 => fields.swap(0, 1),
            _ => fields[0].1 = Type::I64,
        }
        assert_ne!(fingerprint, changed.fingerprint());
    }
    for version in 1u32..WIRE_VERSION {
        let mut old = bytes.clone();
        old[8..12].copy_from_slice(&version.to_le_bytes());
        assert_eq!(
            decode_module_candidate(&old, &limits).unwrap_err()[0].code,
            "E-WIRE-VERSION"
        );
    }
}

#[test]
fn concrete_helper_contract_roundtrips_and_binds_each_claim() {
    let limits = Limits::default();
    let (mut module, _) = module_candidate();
    let contract = CallableParameterContract {
        parameter: 0,
        closure: ClosureId(3),
        kind: CallableKind::Fn,
        hold: vec![0],
        call: vec![0, 2],
        writes: vec![2],
        cleanup: CallableCleanup::Borrowed,
        traces: vec![CallableTrace {
            calls: vec![CallableKind::Fn, CallableKind::Fn],
            exit: CallableExit::Return,
        }],
    };
    module.functions[0].callable_contract = Some(contract.clone());
    let plan = CandidatePlan::empty_for(&module);
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan)
    );
    let mut old = bytes.clone();
    old[8..12].copy_from_slice(&9u32.to_le_bytes());
    assert!(decode_module_candidate(&old, &limits).is_err());
    for length in 0..bytes.len() {
        assert!(decode_module_candidate(&bytes[..length], &limits).is_err());
    }
    let fingerprint = module.fingerprint();
    let mutate: [fn(&mut CallableParameterContract); 9] = [
        |c| c.parameter += 1,
        |c| c.closure.0 += 1,
        |c| c.kind = CallableKind::FnOnce,
        |c| c.hold.clear(),
        |c| c.call.pop().map(|_| ()).unwrap_or(()),
        |c| c.writes.clear(),
        |c| c.cleanup = CallableCleanup::Owned,
        |c| c.traces[0].calls.pop().map(|_| ()).unwrap_or(()),
        |c| c.traces[0].exit = CallableExit::Trap,
    ];
    for change in mutate {
        let mut changed = module.clone();
        change(changed.functions[0].callable_contract.as_mut().unwrap());
        assert_ne!(fingerprint, changed.fingerprint());
        assert_ne!(
            callable_contract_fingerprint(&contract),
            callable_contract_fingerprint(changed.functions[0].callable_contract.as_ref().unwrap())
        );
    }
    module.functions[0].callable_contract = None;
    assert_ne!(fingerprint, module.fingerprint());
}

#[test]
fn borrowed_callable_operation_binds_requested_mode_and_real_address() {
    let limits = Limits::default();
    let (mut module, _) = module_candidate();
    let span = module.functions[0].span;
    module.functions[0].blocks[0]
        .instructions
        .push(Instruction {
            result: None,
            span,
            kind: InstructionKind::CallClosureBorrowed {
                callee: ValueId(8),
                kind: CallableKind::Fn,
                arguments: vec![ValueId(9)],
            },
        });
    let plan = CandidatePlan::empty_for(&module);
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan)
    );
    let original = module.fingerprint();
    let instruction = module.functions[0].blocks[0]
        .instructions
        .last_mut()
        .unwrap();
    let InstructionKind::CallClosureBorrowed { kind, .. } = &mut instruction.kind else {
        unreachable!()
    };
    *kind = CallableKind::FnMut;
    assert_ne!(original, module.fingerprint());
}

#[test]
fn package_capture_view_current_wire_preserves_indices_end_kind_and_fingerprints() {
    // Wire transports untrusted structure. Safety is checked separately by Verify.
    let limits = Limits::default();
    let (mut module, _) = module_candidate();
    let span = module.functions[0].span;
    module.functions[0].blocks[0].instructions = vec![
        Instruction {
            result: None,
            kind: InstructionKind::BindPackageCapture {
                local: LocalId(7),
                index: 3,
            },
            span,
        },
        Instruction {
            result: None,
            kind: InstructionKind::EndPackageCapture {
                local: LocalId(7),
                index: 3,
            },
            span,
        },
    ];
    let plan = CandidatePlan::empty_for(&module);
    let bytes = encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(&bytes[8..12], &WIRE_VERSION.to_le_bytes());
    assert_eq!(
        decode_module_candidate(&bytes, &limits).unwrap(),
        (module.clone(), plan)
    );
    let original = module.fingerprint();
    for instruction in 0..2 {
        for target in 0..3 {
            let mut changed = module.clone();
            let kind = &mut changed.functions[0].blocks[0].instructions[instruction].kind;
            if target == 2 {
                *kind = if instruction == 0 {
                    InstructionKind::EndPackageCapture {
                        local: LocalId(7),
                        index: 3,
                    }
                } else {
                    InstructionKind::BindPackageCapture {
                        local: LocalId(7),
                        index: 3,
                    }
                };
            } else {
                match kind {
                    InstructionKind::BindPackageCapture { local, index }
                    | InstructionKind::EndPackageCapture { local, index } => {
                        if target == 0 {
                            *local = LocalId(8);
                        } else {
                            *index = 4;
                        }
                    }
                    _ => unreachable!(),
                }
            }
            assert_ne!(original, changed.fingerprint());
        }
    }
    for version in (0_u32..WIRE_VERSION).chain([WIRE_VERSION + 1, u32::MAX]) {
        let mut old = bytes.clone();
        old[8..12].copy_from_slice(&version.to_le_bytes());
        assert_eq!(
            decode_module_candidate(&old, &limits).unwrap_err()[0].code,
            "E-WIRE-VERSION"
        );
    }
    for length in 0..bytes.len() {
        assert!(
            decode_module_candidate(&bytes[..length], &limits).is_err(),
            "prefix {length}"
        );
    }
}

#[test]
fn bytes083_scalar_ct_current_wire_preserves_byte_conversion_and_all_prefixes_reject() {
    let mut raw = ct();
    raw.instructions = vec![
        CtInstruction {
            result: ValueId(0),
            ty: Type::U8,
            kind: CtInstructionKind::Constant(Constant::U8(255)),
        },
        CtInstruction {
            result: ValueId(1),
            ty: Type::I64,
            kind: CtInstructionKind::ConvertInteger {
                value: ValueId(0),
                target: Type::I64,
            },
        },
    ];
    raw.result = ValueId(1);
    raw.max_steps = 2;
    let limits = Limits::default();
    let bytes = encode_ct(&raw, &limits).unwrap();
    assert_eq!(&bytes[8..12], &WIRE_VERSION.to_le_bytes());
    assert_eq!(decode_ct(&bytes, &limits).unwrap(), raw);
    for length in 0..bytes.len() {
        assert!(
            decode_ct(&bytes[..length], &limits).is_err(),
            "prefix {length}"
        );
    }
    for version in (0_u32..WIRE_VERSION).chain([WIRE_VERSION + 1, u32::MAX]) {
        let mut stale = bytes.clone();
        stale[8..12].copy_from_slice(&version.to_le_bytes());
        assert_eq!(
            decode_ct(&stale, &limits).unwrap_err()[0].code,
            "E-WIRE-VERSION"
        );
    }
}
