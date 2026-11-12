//! Raw transport only: these deliberately unverified candidates grant no
//! allocation, initialization, borrow or cleanup authority.
use zil_base::{FileId, Fingerprint, Limits, ModuleId, SCHEMA_VERSION, Span, TargetProfile};
use zil_ir::*;

fn candidate() -> TypedModule {
    let span = Span::new(FileId(0), 0, 1);
    TypedModule {
        dyn_declarations: Vec::new(),
        dyn_interfaces: Vec::new(),
        dyn_vtables: Vec::new(),
        life_declarations: Vec::new(),
        life_instances: Vec::new(),
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
            storage_life: None,
            call_kind: zil_ir::CallKind::Direct,
            resume_bound: crate::TaskResumeBound::Local,
            reference_return_bound: None,
            id: FunctionId(0),
            name: "raw".into(),
            foreign: None,
            environment: None,
            callable_contract: None,
            protocol_contract: None,
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

#[test]
fn scoped_foreign_callback_wire32_fields_and_boundaries() {
    use zil_base::{Limits, ResultCategory};
    assert_eq!(SCHEMA_VERSION, 48);
    assert_eq!(wire::WIRE_VERSION, 48);
    assert_eq!(ct_artifact::CT_ARTIFACT_SCHEMA_VERSION, 8);
    let limits = Limits::default();
    let mut raw = candidate();
    raw.functions[0].foreign = Some(ForeignDeclaration {
        abi: "C".into(),
        symbol: "unique_scoped_callback_marker".into(),
        noescape_parameters: vec![],
        static_callbacks: true,
        thread_driver_transfer: false,
        scoped_callback: None,
    });
    let kind = InstructionKind::WithForeignCallback {
        register: FunctionId(0x12345671),
        close: FunctionId(0x23456782),
        callback: ValueId(0x34567893),
        body: ValueId(0x456789a4),
        region: 0x56789ab5,
    };
    raw.functions[0].blocks[0].instructions.push(Instruction {
        result: Some(ValueId(3)),
        kind: kind.clone(),
        span: Span::default(),
    });
    let plan = CandidatePlan::empty_for(&raw);
    let encode = |m: &TypedModule| wire::encode_module_candidate(m, &plan, &limits).unwrap();
    let none = encode(&raw);
    assert_eq!(
        wire::decode_module_candidate(&none, &limits).unwrap(),
        (raw.clone(), plan.clone())
    );
    let none_fp = raw.fingerprint();
    raw.functions[0].foreign.as_mut().unwrap().scoped_callback = Some(ScopedCallbackRole::Register);
    let register = encode(&raw);
    let register_fp = raw.fingerprint();
    assert_ne!(none_fp, register_fp);
    raw.functions[0].foreign.as_mut().unwrap().scoped_callback = Some(ScopedCallbackRole::Close);
    let close = encode(&raw);
    assert_ne!(register_fp, raw.fingerprint());
    assert_eq!(
        wire::decode_module_candidate(&close, &limits).unwrap(),
        (raw.clone(), plan.clone())
    );
    let role_changes = none
        .iter()
        .zip(&register)
        .enumerate()
        .filter_map(|(i, (a, b))| (a != b).then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(role_changes.len(), 1);
    let role = role_changes[0];
    assert_eq!((none[role], register[role], close[role]), (0, 1, 2));
    // These are untrusted role declarations, not valid callback operations.
    // The new roles retain their own byte and semantic identity independently
    // of the legacy synchronous 0/1/2 values above.
    let mut identities =
        std::collections::BTreeSet::from([none_fp, register_fp, raw.fingerprint()]);
    for (tag, declared) in [
        (6, ScopedCallbackRole::ThreadAsyncRegister),
        (7, ScopedCallbackRole::ThreadAsyncRequestClose),
        (8, ScopedCallbackRole::ThreadAsyncPollDrain),
    ] {
        let mut threaded = raw.clone();
        threaded.functions[0]
            .foreign
            .as_mut()
            .unwrap()
            .scoped_callback = Some(declared);
        let bytes = encode(&threaded);
        assert_eq!(bytes[role], tag);
        assert!(identities.insert(threaded.fingerprint()));
        assert_eq!(
            wire::decode_module_candidate(&bytes, &limits).unwrap(),
            (threaded, plan.clone())
        );
    }
    let mut bad_role = register.clone();
    bad_role[role] = 9;
    assert_eq!(
        wire::decode_module_candidate(&bad_role, &limits).unwrap_err()[0].code,
        "E-WIRE-TAG"
    );
    let mut old = register.clone();
    old[8..12].copy_from_slice(&31u32.to_le_bytes());
    assert_eq!(
        wire::decode_module_candidate(&old, &limits).unwrap_err()[0].code,
        "E-WIRE-VERSION"
    );
    let needle = [
        vec![94],
        0x12345671u32.to_le_bytes().to_vec(),
        0x23456782u32.to_le_bytes().to_vec(),
        0x34567893u32.to_le_bytes().to_vec(),
        0x456789a4u32.to_le_bytes().to_vec(),
        0x56789ab5u32.to_le_bytes().to_vec(),
    ]
    .concat();
    let matches = register
        .windows(needle.len())
        .enumerate()
        .filter_map(|(i, x)| (x == needle).then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    let mut bad_op = register.clone();
    bad_op[matches[0]] = 254;
    assert_eq!(
        wire::decode_module_candidate(&bad_op, &limits).unwrap_err()[0].code,
        "E-WIRE-OPCODE"
    );
    let before = raw.fingerprint();
    for field in 0..5 {
        let mut m = raw.clone();
        let InstructionKind::WithForeignCallback {
            register,
            close,
            callback,
            body,
            region,
        } = &mut m.functions[0].blocks[0]
            .instructions
            .last_mut()
            .unwrap()
            .kind
        else {
            panic!()
        };
        match field {
            0 => register.0 += 1,
            1 => close.0 += 1,
            2 => callback.0 += 1,
            3 => body.0 += 1,
            _ => *region += 1,
        };
        assert_ne!(before, m.fingerprint(), "field {field}")
    }
    let exact = Limits {
        max_source_bytes: register.len(),
        ..limits.clone()
    };
    assert_eq!(
        wire::encode_module_candidate(&raw, &plan, &exact).unwrap(),
        close
    );
    assert!(wire::decode_module_candidate(&close, &exact).is_ok());
    let below = Limits {
        max_source_bytes: close.len() - 1,
        ..exact
    };
    for e in [
        wire::encode_module_candidate(&raw, &plan, &below).unwrap_err(),
        wire::decode_module_candidate(&close, &below).unwrap_err(),
    ] {
        assert!(
            e.iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
    }
    for decoding in [false, true] {
        let works = |nodes| {
            let l = Limits {
                max_nodes: nodes,
                ..limits.clone()
            };
            if decoding {
                wire::decode_module_candidate(&close, &l).map(|_| ())
            } else {
                wire::encode_module_candidate(&raw, &plan, &l).map(|_| ())
            }
        };
        let mut low = 0;
        let mut high = limits.max_nodes;
        while low < high {
            let mid = low + (high - low) / 2;
            match works(mid) {
                Ok(()) => high = mid,
                Err(e) => {
                    assert!(
                        e.iter()
                            .all(|e| e.category == ResultCategory::ResourceLimit),
                        "{e:?}"
                    );
                    low = mid + 1
                }
            }
        }
        assert!(works(low).is_ok());
        assert!(
            works(low - 1)
                .unwrap_err()
                .iter()
                .all(|e| e.category == ResultCategory::ResourceLimit)
        );
        println!(
            "{} node boundary N={low}",
            if decoding { "decode" } else { "encode" }
        );
    }
}

#[test]
fn declared_reference_return_bound_is_distinct_bounded_wire33_data() {
    let limits = Limits::default();
    let original = candidate();
    let mut identities = std::collections::BTreeSet::new();
    for bound in [
        None,
        Some(vec![]),
        Some(vec![0]),
        Some(vec![1]),
        Some(vec![0, 1]),
    ] {
        let mut raw = original.clone();
        raw.functions[0].reference_return_bound = bound;
        assert!(identities.insert(raw.fingerprint().to_hex()));
        let bytes =
            wire::encode_module_candidate(&raw, &CandidatePlan::empty_for(&raw), &limits).unwrap();
        let decoded = wire::decode_module_candidate(&bytes, &limits).unwrap().0;
        assert_eq!(decoded, raw);
        let mut old = bytes.clone();
        old[8..12].copy_from_slice(&32u32.to_le_bytes());
        assert!(
            wire::decode_module_candidate(&old, &limits)
                .unwrap_err()
                .iter()
                .any(|d| d.code == "E-WIRE-VERSION")
        );
    }
    // These are raw declaration bytes, not verified provenance claims.
    let mut oversized = original;
    oversized.functions[0].reference_return_bound = Some(vec![0; limits.max_locals + 1]);
    assert!(
        wire::encode_module_candidate(&oversized, &CandidatePlan::empty_for(&oversized), &limits)
            .is_err()
    );
}

#[test]
fn structured_thread_wire34_keeps_three_actual_operands() {
    let limits = Limits::default();
    let mut raw = candidate();
    raw.functions[0].blocks[0].instructions.push(Instruction {
        result: Some(ValueId(100)),
        span: Span::default(),
        kind: InstructionKind::WithScopedThread {
            worker: FunctionId(0x12345678),
            argument: ValueId(0x23456789),
            body: ValueId(0x3456789a),
        },
    });
    let bytes =
        wire::encode_module_candidate(&raw, &CandidatePlan::empty_for(&raw), &limits).unwrap();
    assert_eq!(
        wire::decode_module_candidate(&bytes, &limits).unwrap().0,
        raw
    );
    let marker = [
        vec![95],
        0x12345678u32.to_le_bytes().to_vec(),
        0x23456789u32.to_le_bytes().to_vec(),
        0x3456789au32.to_le_bytes().to_vec(),
    ]
    .concat();
    let offset = bytes
        .windows(marker.len())
        .position(|window| window == marker)
        .unwrap();
    let mut unknown = bytes.clone();
    unknown[offset] = 96;
    assert!(
        wire::decode_module_candidate(&unknown, &limits)
            .unwrap_err()
            .iter()
            .any(|d| d.code == "E-WIRE-OPCODE")
    );
    for version in [32u32, 33] {
        let mut old = bytes.clone();
        old[8..12].copy_from_slice(&version.to_le_bytes());
        assert!(
            wire::decode_module_candidate(&old, &limits)
                .unwrap_err()
                .iter()
                .any(|d| d.code == "E-WIRE-VERSION")
        );
    }
}
