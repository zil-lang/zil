//! Transport preserves untrusted ownership claims; it never proves them.
use super::*;

fn operations() -> Vec<InstructionKind> {
    vec![
        InstructionKind::TakeErrorUnion {
            value: ValueId(7),
            is_error: true,
        },
        InstructionKind::DropErrorUnion { local: LocalId(11) },
        InstructionKind::MakeOwnedArray {
            elements: vec![ValueId(13), ValueId(17)],
        },
        InstructionKind::ProjectOwnedArrayField {
            local: LocalId(19),
            index: 23,
            field: 29,
        },
        InstructionKind::TakeOwnedArrayElement {
            local: LocalId(31),
            index: 37,
        },
        InstructionKind::DropOwnedArrayElement {
            local: LocalId(41),
            index: 43,
        },
        InstructionKind::StoreOwnedArrayElement {
            local: LocalId(47),
            index: 53,
            value: ValueId(59),
        },
        InstructionKind::DropOwnedArray { local: LocalId(61) },
        InstructionKind::BorrowCapture {
            index: 67,
            mutable: true,
        },
        InstructionKind::BorrowOwnedArrayElement {
            local: LocalId(73),
            index: 79,
            mutable: true,
        },
    ]
}

fn instruction(kind: InstructionKind) -> Instruction {
    Instruction {
        result: Some(ValueId(71)),
        kind,
        span: Span::new(FileId(3), 5, 11),
    }
}

#[test]
fn schema19_owned_claims_roundtrip_with_distinct_fixed_opcodes() {
    let limits = Limits::default();
    for (kind, opcode) in operations().into_iter().zip(70..=79) {
        let original = instruction(kind);
        let mut writer = Writer::new(&limits);
        writer.instruction(&original).unwrap();
        // Presence byte and result ID precede the fixed operation tag.
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
    }
}

#[test]
fn schema19_owned_claims_fingerprint_binds_every_operand_and_flag() {
    let mut encodings = std::collections::BTreeSet::new();
    for original in operations() {
        let mut bytes = vec![];
        encode_instruction(&mut bytes, &original);
        assert!(
            encodings.insert(bytes.clone()),
            "operation identity collision"
        );
        let mutations = match original.clone() {
            InstructionKind::TakeErrorUnion { value, is_error } => vec![
                InstructionKind::TakeErrorUnion {
                    value: ValueId(value.0 + 1),
                    is_error,
                },
                InstructionKind::TakeErrorUnion {
                    value,
                    is_error: !is_error,
                },
            ],
            InstructionKind::DropErrorUnion { local } => vec![InstructionKind::DropErrorUnion {
                local: LocalId(local.0 + 1),
            }],
            InstructionKind::MakeOwnedArray { elements } => vec![
                InstructionKind::MakeOwnedArray {
                    elements: vec![elements[1], elements[0]],
                },
                InstructionKind::MakeOwnedArray {
                    elements: vec![elements[0]],
                },
                InstructionKind::MakeOwnedArray {
                    elements: vec![ValueId(elements[0].0 + 1), elements[1]],
                },
                InstructionKind::MakeOwnedArray {
                    elements: vec![elements[0], ValueId(elements[1].0 + 1)],
                },
            ],
            InstructionKind::ProjectOwnedArrayField {
                local,
                index,
                field,
            } => vec![
                InstructionKind::ProjectOwnedArrayField {
                    local: LocalId(local.0 + 1),
                    index,
                    field,
                },
                InstructionKind::ProjectOwnedArrayField {
                    local,
                    index: index + 1,
                    field,
                },
                InstructionKind::ProjectOwnedArrayField {
                    local,
                    index,
                    field: field + 1,
                },
            ],
            InstructionKind::TakeOwnedArrayElement { local, index } => vec![
                InstructionKind::TakeOwnedArrayElement {
                    local: LocalId(local.0 + 1),
                    index,
                },
                InstructionKind::TakeOwnedArrayElement {
                    local,
                    index: index + 1,
                },
            ],
            InstructionKind::DropOwnedArrayElement { local, index } => vec![
                InstructionKind::DropOwnedArrayElement {
                    local: LocalId(local.0 + 1),
                    index,
                },
                InstructionKind::DropOwnedArrayElement {
                    local,
                    index: index + 1,
                },
            ],
            InstructionKind::StoreOwnedArrayElement {
                local,
                index,
                value,
            } => vec![
                InstructionKind::StoreOwnedArrayElement {
                    local: LocalId(local.0 + 1),
                    index,
                    value,
                },
                InstructionKind::StoreOwnedArrayElement {
                    local,
                    index: index + 1,
                    value,
                },
                InstructionKind::StoreOwnedArrayElement {
                    local,
                    index,
                    value: ValueId(value.0 + 1),
                },
            ],
            InstructionKind::DropOwnedArray { local } => vec![InstructionKind::DropOwnedArray {
                local: LocalId(local.0 + 1),
            }],
            InstructionKind::BorrowCapture { index, mutable } => vec![
                InstructionKind::BorrowCapture {
                    index: index + 1,
                    mutable,
                },
                InstructionKind::BorrowCapture {
                    index,
                    mutable: !mutable,
                },
            ],
            InstructionKind::BorrowOwnedArrayElement {
                local,
                index,
                mutable,
            } => vec![
                InstructionKind::BorrowOwnedArrayElement {
                    local: LocalId(local.0 + 1),
                    index,
                    mutable,
                },
                InstructionKind::BorrowOwnedArrayElement {
                    local,
                    index: index + 1,
                    mutable,
                },
                InstructionKind::BorrowOwnedArrayElement {
                    local,
                    index,
                    mutable: !mutable,
                },
            ],
            _ => unreachable!(),
        };
        for mutation in mutations {
            let mut changed = vec![];
            encode_instruction(&mut changed, &mutation);
            assert_ne!(bytes, changed, "fingerprint omitted a claim");
        }
    }
}

#[test]
fn schema19_owned_claims_reject_noncanonical_flags_and_array_allocation_bombs() {
    let limits = Limits::default();
    for kind in [
        InstructionKind::TakeErrorUnion {
            value: ValueId(0),
            is_error: false,
        },
        InstructionKind::BorrowCapture {
            index: 0,
            mutable: false,
        },
    ] {
        let mut writer = Writer::new(&limits);
        writer.instruction(&instruction(kind)).unwrap();
        writer.output[10] = 2;
        assert_eq!(
            Reader::new(&writer.output, &limits)
                .unwrap()
                .instruction()
                .unwrap_err()
                .code,
            "E-WIRE-BOOL"
        );
    }
    let mut writer = Writer::new(&limits);
    writer
        .instruction(&instruction(InstructionKind::MakeOwnedArray {
            elements: vec![ValueId(0)],
        }))
        .unwrap();
    writer.output[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(
        Reader::new(&writer.output, &limits)
            .unwrap()
            .instruction()
            .is_err()
    );
    let mut writer = Writer::new(&limits);
    writer
        .instruction(&instruction(InstructionKind::BorrowOwnedArrayElement {
            local: LocalId(0),
            index: 0,
            mutable: false,
        }))
        .unwrap();
    writer.output[14] = 2;
    assert_eq!(
        Reader::new(&writer.output, &limits)
            .unwrap()
            .instruction()
            .unwrap_err()
            .code,
        "E-WIRE-BOOL"
    );
}
