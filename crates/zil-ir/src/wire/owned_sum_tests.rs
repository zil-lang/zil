//! Schema30 raw transport does not confer tag, ownership or pattern authority.
use super::*;

fn inst(kind: InstructionKind) -> Instruction {
    Instruction {
        result: None,
        kind,
        span: Span::new(FileId(1), 2, 3),
    }
}
fn encode(kind: InstructionKind, limits: &Limits) -> Vec<u8> {
    let mut writer = Writer::new(limits);
    writer.instruction(&inst(kind)).unwrap();
    writer.output
}
fn path() -> Vec<VariantProjection> {
    vec![
        VariantProjection {
            variant: u32::MAX,
            index: 0,
        },
        VariantProjection {
            variant: 1,
            index: u32::MAX,
        },
    ]
}

#[test]
fn owned_sum_literal_opcodes_preserve_all_raw_fields_and_truncation_rejects() {
    let limits = Limits::default();
    let cases = [
        (
            InstructionKind::VariantTagLocal {
                local: LocalId(u32::MAX),
                path: vec![],
            },
            vec![90, 255, 255, 255, 255, 0, 0, 0, 0],
        ),
        (
            InstructionKind::ReadVariantLocal {
                local: LocalId(0),
                path: vec![VariantProjection {
                    variant: u32::MAX,
                    index: u32::MAX,
                }],
            },
            vec![
                91, 0, 0, 0, 0, 1, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255,
            ],
        ),
        (
            InstructionKind::UnpackVariant {
                local: LocalId(u32::MAX),
                variant: u32::MAX,
                fields: vec![LocalId(0), LocalId(u32::MAX)],
            },
            vec![
                92, 255, 255, 255, 255, 255, 255, 255, 255, 2, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255,
                255,
            ],
        ),
        (
            InstructionKind::UnpackVariant {
                local: LocalId(1),
                variant: 0,
                fields: vec![],
            },
            vec![92, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
        (
            InstructionKind::DropEnum {
                local: LocalId(u32::MAX),
            },
            vec![93, 255, 255, 255, 255],
        ),
    ];
    for (kind, body) in cases {
        let bytes = encode(kind.clone(), &limits);
        assert_eq!(&bytes[1..1 + body.len()], body);
        let mut reader = Reader::new(&bytes, &limits).unwrap();
        assert_eq!(reader.instruction().unwrap(), inst(kind));
        assert_eq!(reader.remaining(), 0);
        for cut in 0..bytes.len() {
            assert!(
                Reader::new(&bytes[..cut], &limits)
                    .unwrap()
                    .instruction()
                    .is_err(),
                "cut={cut}"
            );
        }
        for unknown in [94, 254] {
            let mut bad = bytes.clone();
            bad[1] = unknown;
            assert_eq!(
                Reader::new(&bad, &limits)
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
fn projection_and_output_vectors_are_precharged_before_allocation() {
    let limits = Limits {
        max_call_depth: 2,
        max_locals: 2,
        ..Limits::default()
    };
    for kind in [
        InstructionKind::VariantTagLocal {
            local: LocalId(0),
            path: path(),
        },
        InstructionKind::ReadVariantLocal {
            local: LocalId(0),
            path: path(),
        },
        InstructionKind::UnpackVariant {
            local: LocalId(0),
            variant: 0,
            fields: vec![LocalId(1), LocalId(2)],
        },
    ] {
        let bytes = encode(kind.clone(), &limits);
        let mut reader = Reader::new(&bytes, &limits).unwrap();
        reader.instruction().unwrap();
        let mut writer = Writer::new(&limits);
        writer.instruction(&inst(kind.clone())).unwrap();
        assert_eq!(reader.nodes, writer.nodes);
        let used = limits.max_nodes - reader.nodes;
        for nodes in [used, used - 1, used] {
            let tight = Limits {
                max_nodes: nodes,
                ..limits.clone()
            };
            let read = Reader::new(&bytes, &tight).unwrap().instruction();
            let write = Writer::new(&tight).instruction(&inst(kind.clone()));
            assert_eq!(read.is_ok(), nodes == used);
            assert_eq!(write.is_ok(), nodes == used);
        }
        let shallow = Limits {
            max_call_depth: 1,
            max_locals: 1,
            ..limits.clone()
        };
        assert!(
            Reader::new(&bytes, &shallow)
                .unwrap()
                .instruction()
                .is_err()
        );
        assert!(Writer::new(&shallow).instruction(&inst(kind)).is_err());
    }
}

#[test]
fn malicious_collection_lengths_do_not_allocate_or_consume_fake_records() {
    let limits = Limits::default();
    for (kind, offset) in [
        (
            InstructionKind::ReadVariantLocal {
                local: LocalId(0),
                path: path(),
            },
            6,
        ),
        (
            InstructionKind::UnpackVariant {
                local: LocalId(0),
                variant: 0,
                fields: vec![LocalId(1)],
            },
            10,
        ),
    ] {
        let mut bytes = encode(kind, &limits);
        bytes[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let errors = Reader::new(&bytes, &limits)
            .unwrap()
            .instruction()
            .unwrap_err();
        assert_eq!(errors.category, zil_base::ResultCategory::ResourceLimit);
        // A small allowed count that cannot fit must fail before Vec allocation.
        bytes[offset..offset + 4].copy_from_slice(&200u32.to_le_bytes());
        assert_eq!(
            Reader::new(&bytes, &limits)
                .unwrap()
                .instruction()
                .unwrap_err()
                .code,
            "E-WIRE-TRUNCATED"
        );
    }
}

#[test]
fn local_enum_fingerprint_binds_opcodes_every_projection_and_output_order() {
    let step = VariantProjection {
        variant: 1,
        index: 2,
    };
    let cases = [
        InstructionKind::VariantTagLocal {
            local: LocalId(0),
            path: vec![],
        },
        InstructionKind::VariantTagLocal {
            local: LocalId(1),
            path: vec![],
        },
        InstructionKind::VariantTagLocal {
            local: LocalId(0),
            path: vec![step],
        },
        InstructionKind::VariantTagLocal {
            local: LocalId(0),
            path: vec![VariantProjection { variant: 3, ..step }],
        },
        InstructionKind::VariantTagLocal {
            local: LocalId(0),
            path: vec![VariantProjection { index: 3, ..step }],
        },
        InstructionKind::VariantTagLocal {
            local: LocalId(0),
            path: vec![step, step],
        },
        InstructionKind::ReadVariantLocal {
            local: LocalId(0),
            path: vec![step],
        },
        InstructionKind::ReadVariantLocal {
            local: LocalId(1),
            path: vec![step],
        },
        InstructionKind::ReadVariantLocal {
            local: LocalId(0),
            path: vec![VariantProjection { variant: 3, ..step }],
        },
        InstructionKind::ReadVariantLocal {
            local: LocalId(0),
            path: vec![VariantProjection { index: 3, ..step }],
        },
        InstructionKind::ReadVariantLocal {
            local: LocalId(0),
            path: vec![step, step],
        },
        InstructionKind::UnpackVariant {
            local: LocalId(0),
            variant: 1,
            fields: vec![LocalId(2), LocalId(3)],
        },
        InstructionKind::UnpackVariant {
            local: LocalId(1),
            variant: 1,
            fields: vec![LocalId(2), LocalId(3)],
        },
        InstructionKind::UnpackVariant {
            local: LocalId(0),
            variant: 2,
            fields: vec![LocalId(2), LocalId(3)],
        },
        InstructionKind::UnpackVariant {
            local: LocalId(0),
            variant: 1,
            fields: vec![LocalId(3), LocalId(2)],
        },
        InstructionKind::UnpackVariant {
            local: LocalId(0),
            variant: 1,
            fields: vec![LocalId(2)],
        },
        InstructionKind::DropEnum { local: LocalId(0) },
        InstructionKind::DropEnum { local: LocalId(1) },
    ];
    let mut fingerprints = std::collections::BTreeSet::new();
    for kind in cases {
        let mut stream = crate::FingerprintStream::new();
        crate::encode_instruction(&mut stream, &kind);
        assert!(fingerprints.insert(stream.finish().to_hex()), "{kind:?}");
    }
}
