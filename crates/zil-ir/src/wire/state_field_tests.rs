//! Transport oracles do not grant a loan, field authority or an owning result.
use super::*;

fn cases() -> [(InstructionKind, Vec<u8>); 4] {
    [
        (
            InstructionKind::OwnedStoreBorrowMut {
                store: ValueId(11),
                index: ValueId(13),
            },
            vec![1, 7, 0, 0, 0, 88, 11, 0, 0, 0, 13, 0, 0, 0],
        ),
        (
            InstructionKind::OwnedStoreBorrow {
                store: ValueId(11),
                index: ValueId(13),
            },
            vec![1, 7, 0, 0, 0, 86, 11, 0, 0, 0, 13, 0, 0, 0],
        ),
        (
            InstructionKind::BorrowPackageField {
                local: LocalId(17),
                index: 19,
                mutable: false,
            },
            vec![1, 7, 0, 0, 0, 87, 17, 0, 0, 0, 19, 0, 0, 0, 0],
        ),
        (
            InstructionKind::BorrowPackageField {
                local: LocalId(17),
                index: 19,
                mutable: true,
            },
            vec![1, 7, 0, 0, 0, 87, 17, 0, 0, 0, 19, 0, 0, 0, 1],
        ),
    ]
}

#[test]
fn new_window_instructions_match_manual_bytes_and_all_truncations_fail() {
    let limits = Limits::default();
    for (kind, mut expected) in cases() {
        // FileId u32 then two u64 source offsets: these bytes are the oracle.
        expected.extend([1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0]);
        let instruction = Instruction {
            result: Some(ValueId(7)),
            kind,
            span: Span::new(FileId(1), 2, 3),
        };
        let mut writer = Writer::new(&limits);
        writer.instruction(&instruction).unwrap();
        assert_eq!(writer.output, expected);
        let mut reader = Reader::new(&expected, &limits).unwrap();
        assert_eq!(reader.instruction().unwrap(), instruction);
        assert_eq!(reader.remaining(), 0);
        for length in 0..expected.len() {
            assert!(
                Reader::new(&expected[..length], &limits)
                    .unwrap()
                    .instruction()
                    .is_err()
            );
        }
        for tag in [90, 91, 254] {
            let mut bad = expected.clone();
            bad[5] = tag;
            assert_eq!(
                Reader::new(&bad, &limits)
                    .unwrap()
                    .instruction()
                    .unwrap_err()
                    .code,
                "E-WIRE-OPCODE"
            );
        }
        if expected[5] == 87 {
            for invalid_bool in [2, 127, 255] {
                let mut bad = expected.clone();
                bad[14] = invalid_bool;
                assert!(Reader::new(&bad, &limits).unwrap().instruction().is_err());
            }
        }
    }
}

#[test]
fn window_fingerprints_bind_each_index_operand_and_mutability() {
    let originals = [
        InstructionKind::OwnedStoreBorrowMut {
            store: ValueId(11),
            index: ValueId(13),
        },
        InstructionKind::OwnedStoreBorrow {
            store: ValueId(11),
            index: ValueId(13),
        },
        InstructionKind::BorrowPackageField {
            local: LocalId(17),
            index: 19,
            mutable: false,
        },
    ];
    for original in originals {
        let mutations = match original {
            InstructionKind::OwnedStoreBorrowMut { store, index } => vec![
                InstructionKind::OwnedStoreBorrowMut {
                    store: ValueId(store.0 + 1),
                    index,
                },
                InstructionKind::OwnedStoreBorrowMut {
                    store,
                    index: ValueId(index.0 + 1),
                },
                InstructionKind::OwnedStoreBorrow { store, index },
            ],
            InstructionKind::OwnedStoreBorrow { store, index } => vec![
                InstructionKind::OwnedStoreBorrow {
                    store: ValueId(store.0 + 1),
                    index,
                },
                InstructionKind::OwnedStoreBorrow {
                    store,
                    index: ValueId(index.0 + 1),
                },
            ],
            InstructionKind::BorrowPackageField {
                local,
                index,
                mutable,
            } => vec![
                InstructionKind::BorrowPackageField {
                    local: LocalId(local.0 + 1),
                    index,
                    mutable,
                },
                InstructionKind::BorrowPackageField {
                    local,
                    index: index + 1,
                    mutable,
                },
                InstructionKind::BorrowPackageField {
                    local,
                    index,
                    mutable: !mutable,
                },
            ],
            _ => unreachable!(),
        };
        let mut base = vec![];
        encode_instruction(&mut base, &original);
        for mutation in mutations {
            let mut changed = vec![];
            encode_instruction(&mut changed, &mutation);
            assert_ne!(base, changed);
        }
    }
}

#[test]
fn shared_package_shape_does_not_grant_copy_or_general_pointee_support() {
    let item = Type::Package {
        name: "Item".into(),
        fields: vec![],
    };
    assert!(!item.is_address_pointee());
    assert!(!item.is_copy_value());
    let shared = Type::Address {
        mutable: false,
        inner: Box::new(item.clone()),
    };
    assert!(shared.is_shared_package_address());
    assert!(shared.is_scalar_or_shared_address());
    assert!(shared.is_scalar_or_address());
    let exclusive = Type::Address {
        mutable: true,
        inner: Box::new(item),
    };
    assert!(exclusive.is_package_address());
    assert!(exclusive.is_scalar_or_address());
    assert!(!exclusive.is_shared_package_address());
    assert!(!exclusive.is_scalar_or_shared_address());
    assert!(!exclusive.is_copy_value());
    for inner in [
        Type::ByteBuffer,
        Type::OwnedStore {
            element: Box::new(Type::I32),
        },
        Type::Closure(ClosureId(0)),
    ] {
        assert!(
            !Type::Address {
                mutable: false,
                inner: Box::new(inner)
            }
            .is_shared_package_address()
        );
    }
}
