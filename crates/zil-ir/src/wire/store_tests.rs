//! Raw transport identities and bounds; these tests do not certify ownership.
use super::*;

fn operation(kind: InstructionKind) -> Instruction {
    Instruction {
        result: Some(ValueId(7)),
        kind,
        span: Span::new(FileId(1), 2, 3),
    }
}

fn operations() -> Vec<InstructionKind> {
    vec![
        InstructionKind::OwnedStoreNew { element: Type::I32 },
        InstructionKind::OwnedStoreReserve {
            store: ValueId(11),
            capacity: ValueId(13),
        },
        InstructionKind::OwnedStorePush {
            store: ValueId(17),
            value: ValueId(19),
        },
        InstructionKind::OwnedStorePop { store: ValueId(23) },
        InstructionKind::OwnedStoreLen { store: ValueId(29) },
        InstructionKind::OwnedStoreCapacity { store: ValueId(31) },
    ]
}

#[test]
fn store_raw_ops_have_fixed_tags_and_reject_every_truncated_prefix() {
    let limits = Limits::default();
    for (kind, opcode) in operations().into_iter().zip(80..=85) {
        let original = operation(kind);
        let mut writer = Writer::new(&limits);
        writer.instruction(&original).unwrap();
        assert_eq!(writer.output[5], opcode);
        let mut reader = Reader::new(&writer.output, &limits).unwrap();
        assert_eq!(reader.instruction().unwrap(), original);
        assert_eq!(reader.remaining(), 0);
        for length in 0..writer.output.len() {
            assert!(
                Reader::new(&writer.output[..length], &limits)
                    .unwrap()
                    .instruction()
                    .is_err()
            );
        }
        for unknown in [90, 91] {
            let mut changed = writer.output.clone();
            changed[5] = unknown;
            assert_eq!(
                Reader::new(&changed, &limits)
                    .unwrap()
                    .instruction()
                    .unwrap_err()
                    .code,
                "E-WIRE-OPCODE"
            );
        }
    }
}

#[test]
fn store_fingerprint_binds_each_operand_and_full_element_type() {
    let mut encodings = std::collections::BTreeSet::new();
    for original in operations() {
        let mutations = match original.clone() {
            InstructionKind::OwnedStoreNew { .. } => vec![
                InstructionKind::OwnedStoreNew { element: Type::I64 },
                InstructionKind::OwnedStoreNew {
                    element: Type::Array {
                        element: Box::new(Type::I32),
                        length: 2,
                    },
                },
            ],
            InstructionKind::OwnedStoreReserve { store, capacity } => vec![
                InstructionKind::OwnedStoreReserve {
                    store: ValueId(store.0 + 1),
                    capacity,
                },
                InstructionKind::OwnedStoreReserve {
                    store,
                    capacity: ValueId(capacity.0 + 1),
                },
            ],
            InstructionKind::OwnedStorePush { store, value } => vec![
                InstructionKind::OwnedStorePush {
                    store: ValueId(store.0 + 1),
                    value,
                },
                InstructionKind::OwnedStorePush {
                    store,
                    value: ValueId(value.0 + 1),
                },
            ],
            InstructionKind::OwnedStorePop { store } => vec![InstructionKind::OwnedStorePop {
                store: ValueId(store.0 + 1),
            }],
            InstructionKind::OwnedStoreLen { store } => vec![InstructionKind::OwnedStoreLen {
                store: ValueId(store.0 + 1),
            }],
            InstructionKind::OwnedStoreCapacity { store } => {
                vec![InstructionKind::OwnedStoreCapacity {
                    store: ValueId(store.0 + 1),
                }]
            }
            _ => unreachable!(),
        };
        let mut encoded = vec![];
        encode_instruction(&mut encoded, &original);
        assert!(encodings.insert(encoded.clone()));
        for mutation in mutations {
            let mut changed = vec![];
            encode_instruction(&mut changed, &mutation);
            assert_ne!(encoded, changed);
        }
    }
}

#[test]
fn store_type_has_distinct_tag_and_nested_metadata_is_bounded_both_ways() {
    let normal = Limits::default();
    let raw = Type::OwnedStore {
        element: Box::new(Type::I32),
    };
    let mut writer = Writer::new(&normal);
    writer.ty(&raw, 0).unwrap();
    assert_eq!(writer.output, [25, 2]);
    assert_eq!(
        Reader::new(&writer.output, &normal).unwrap().ty(0).unwrap(),
        raw
    );
    let limits = Limits {
        max_call_depth: 4,
        ..normal.clone()
    };
    let mut deep = Type::I32;
    for _ in 0..5 {
        deep = Type::OwnedStore {
            element: Box::new(deep),
        };
    }
    assert!(Writer::new(&limits).ty(&deep, 0).is_err());
    let mut writer = Writer::new(&normal);
    writer.ty(&deep, 0).unwrap();
    assert!(Reader::new(&writer.output, &limits).unwrap().ty(0).is_err());
}

#[test]
fn current_wire_rejects_all_old_versions_and_next_unknown_version() {
    let limits = Limits::default();
    let mut writer = Writer::new(&limits);
    writer.header(MODULE_MAGIC).unwrap();
    assert_eq!(WIRE_VERSION, 28);
    assert_eq!(SCHEMA_VERSION, 28);
    Reader::new(&writer.output, &limits)
        .unwrap()
        .header(MODULE_MAGIC)
        .unwrap();
    for version in (0u32..WIRE_VERSION).chain([WIRE_VERSION + 1, u32::MAX]) {
        let mut bytes = writer.output.clone();
        bytes[8..12].copy_from_slice(&version.to_le_bytes());
        assert_eq!(
            Reader::new(&bytes, &limits)
                .unwrap()
                .header(MODULE_MAGIC)
                .unwrap_err()
                .code,
            "E-WIRE-VERSION"
        );
    }
}

#[test]
fn shared_head_shape_does_not_expand_general_pointees_or_sealed_heads() {
    let payload = Type::Record {
        name: "test::R".into(),
        fields: vec![("x".into(), Type::I32)],
    };
    let head = Type::Managed(Box::new(payload));
    let address = Type::Address {
        mutable: false,
        inner: Box::new(head.clone()),
    };
    assert!(address.is_shared_resource_head_address());
    assert!(address.is_scalar_or_shared_address());
    assert!(address.is_scalar_or_address());
    assert!(!head.is_address_pointee());
    assert!(
        !Type::Address {
            mutable: true,
            inner: Box::new(head)
        }
        .is_shared_resource_head_address()
    );
    for payload in [
        Type::ByteBuffer,
        Type::OwnedStore {
            element: Box::new(Type::I32),
        },
    ] {
        assert!(
            !Type::Address {
                mutable: false,
                inner: Box::new(Type::Managed(Box::new(payload)))
            }
            .is_shared_resource_head_address()
        );
    }
    assert!(
        !Type::Address {
            mutable: false,
            inner: Box::new(Type::Desc(Box::new(Type::I32)))
        }
        .is_shared_resource_head_address()
    );
}
