//! Raw transport only: these deliberately unverified candidates grant no
//! allocation, initialization, borrow or cleanup authority.
use zil_base::{FileId, Fingerprint, Limits, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;

fn candidate() -> TypedModule {
    let span = Span::new(FileId(0), 0, 1);
    TypedModule {
        schema_version: SCHEMA_VERSION,
        module_id: ModuleId(0),
        source_fingerprint: Fingerprint::of(b"096 raw byte-storage transport"),
        target: TargetProfile::host_llvm(),
        resources: vec![ResourceDefinition {
            name: BYTE_BUFFER_RESOURCE_NAME.into(),
            payload: Type::ByteBuffer,
            teardown: None,
        }],
        closures: vec![],
        unsupported: vec![],
        functions: vec![TypedFunction {
            id: FunctionId(0),
            name: "raw".into(),
            foreign: None,
            environment: None,
            callable_contract: None,
            callable_return_contract: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            locals: vec![LocalDecl {
                id: LocalId(0),
                name: "owned".into(),
                ty: Type::Managed(Box::new(Type::ByteBuffer)),
                mutable: false,
                span,
            }],
            values: vec![
                Type::Managed(Box::new(Type::ByteBuffer)),
                Type::Desc(Box::new(Type::ByteBuffer)),
                Type::I64,
                Type::U8,
                Type::Bool,
                Type::Bool,
                Type::I64,
                Type::I64,
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::Slice {
                        element: Box::new(Type::U8),
                    }),
                },
            ],
            blocks: vec![BasicBlock {
                id: BlockId(0),
                instructions: vec![
                    Instruction {
                        result: Some(ValueId(0)),
                        kind: InstructionKind::ByteBufferNew,
                        span,
                    },
                    Instruction {
                        result: Some(ValueId(4)),
                        kind: InstructionKind::ByteBufferReserve {
                            buffer: ValueId(1),
                            capacity: ValueId(2),
                        },
                        span,
                    },
                    Instruction {
                        result: Some(ValueId(5)),
                        kind: InstructionKind::ByteBufferPush {
                            buffer: ValueId(1),
                            value: ValueId(3),
                        },
                        span,
                    },
                    Instruction {
                        result: Some(ValueId(6)),
                        kind: InstructionKind::ByteBufferLen { buffer: ValueId(1) },
                        span,
                    },
                    Instruction {
                        result: Some(ValueId(7)),
                        kind: InstructionKind::ByteBufferCapacity { buffer: ValueId(1) },
                        span,
                    },
                    Instruction {
                        result: Some(ValueId(8)),
                        kind: InstructionKind::ByteBufferBorrow {
                            buffer: ValueId(1),
                            mutable: false,
                        },
                        span,
                    },
                ],
                terminator: Terminator::Return(None),
            }],
            entry: BlockId(0),
            span,
        }],
    }
}

#[test]
fn raw_byte_storage_roundtrip_preserves_every_claim_and_old_versions_reject() {
    let limits = Limits::default();
    let module = candidate();
    let plan = CandidatePlan::empty_for(&module);
    let bytes = wire::encode_module_candidate(&module, &plan, &limits).unwrap();
    assert_eq!(
        wire::decode_module_candidate(&bytes, &limits).unwrap(),
        (module, plan)
    );
    for version in 0..wire::WIRE_VERSION {
        let mut old = bytes.clone();
        old[8..12].copy_from_slice(&version.to_le_bytes());
        assert_eq!(
            wire::decode_module_candidate(&old, &limits).unwrap_err()[0].code,
            "E-WIRE-VERSION"
        );
    }
    for size in 0..bytes.len() {
        assert!(
            wire::decode_module_candidate(&bytes[..size], &limits).is_err(),
            "partial byte record {size}"
        );
    }
    let mut excessive = bytes.clone();
    excessive.push(0);
    assert_eq!(
        wire::decode_module_candidate(&excessive, &limits).unwrap_err()[0].code,
        "E-WIRE-TRAILING"
    );
}

#[test]
fn each_byte_storage_operand_mutability_and_definition_bind_identity() {
    let module = candidate();
    let original = module.fingerprint();
    let mut variants = Vec::new();
    for (index, replacement) in [
        (
            1,
            InstructionKind::ByteBufferReserve {
                buffer: ValueId(8),
                capacity: ValueId(2),
            },
        ),
        (
            1,
            InstructionKind::ByteBufferReserve {
                buffer: ValueId(1),
                capacity: ValueId(8),
            },
        ),
        (
            2,
            InstructionKind::ByteBufferPush {
                buffer: ValueId(8),
                value: ValueId(3),
            },
        ),
        (
            2,
            InstructionKind::ByteBufferPush {
                buffer: ValueId(1),
                value: ValueId(8),
            },
        ),
        (3, InstructionKind::ByteBufferLen { buffer: ValueId(8) }),
        (
            4,
            InstructionKind::ByteBufferCapacity { buffer: ValueId(8) },
        ),
        (
            5,
            InstructionKind::ByteBufferBorrow {
                buffer: ValueId(8),
                mutable: false,
            },
        ),
        (
            5,
            InstructionKind::ByteBufferBorrow {
                buffer: ValueId(1),
                mutable: true,
            },
        ),
    ] {
        let mut changed = module.clone();
        changed.functions[0].blocks[0].instructions[index].kind = replacement;
        variants.push(changed);
    }
    let mut changed = module.clone();
    changed.resources[0].teardown = Some(FunctionId(0));
    variants.push(changed);
    let mut changed = module.clone();
    changed.resources[0].name.push('!');
    variants.push(changed);
    for changed in variants {
        assert_ne!(changed.fingerprint(), original);
        let plan = CandidatePlan::empty_for(&changed);
        let bytes = wire::encode_module_candidate(&changed, &plan, &Limits::default()).unwrap();
        assert_eq!(
            wire::decode_module_candidate(&bytes, &Limits::default())
                .unwrap()
                .0,
            changed
        );
    }
}
