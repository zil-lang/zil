//! Raw transport tests: no source expression is a permission certificate.
use super::*;
fn position(base: StorageBase, field: u32) -> StoragePosition {
    StoragePosition {
        base,
        path: vec![ContractProjection::Place(PlaceProjection::Field(field))],
    }
}
fn atoms() -> Vec<StorageSource> {
    vec![
        StorageSource::Exact(position(StorageBase::Parameter(2), 7)),
        StorageSource::PlaceAddress {
            place: position(StorageBase::Capture(3), 8),
            mode: StoredBorrowMode::ExclusiveAddress,
        },
        StorageSource::OwnedView {
            head: position(StorageBase::Parameter(4), 9),
            leaf: Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            mode: StoredBorrowMode::SharedAddress,
        },
        StorageSource::ReborrowedLeaf {
            source: position(StorageBase::Parameter(5), 10),
            head: position(StorageBase::Parameter(6), 11),
        },
        StorageSource::Static {
            leaf: Type::Unit,
            mode: StoredBorrowMode::ReadOnlyDescription,
        },
    ]
}
fn raw_position(out: &mut Vec<u8>, base: u8, index: u32, field: u32) {
    out.push(base);
    out.extend_from_slice(&index.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&field.to_le_bytes());
}
#[test]
fn every_source_discriminant_has_an_independent_byte_oracle() {
    let limits = Limits::default();
    let mut expected = Vec::new();
    expected.push(0);
    raw_position(&mut expected, 0, 2, 7);
    expected.push(1);
    raw_position(&mut expected, 1, 3, 8);
    expected.push(1);
    expected.push(2);
    raw_position(&mut expected, 0, 4, 9);
    expected.extend_from_slice(&[6, 0, 2, 0]);
    expected.push(3);
    raw_position(&mut expected, 0, 5, 10);
    raw_position(&mut expected, 0, 6, 11);
    expected.extend_from_slice(&[4, 0, 2]);
    let values = atoms();
    let mut writer = Writer::new(&limits);
    for atom in &values {
        writer.storage_source(atom, 0).unwrap();
    }
    assert_eq!(writer.output, expected);
    let mut reader = Reader::new(&expected, &limits).unwrap();
    for atom in values {
        assert_eq!(reader.storage_source(0).unwrap(), atom);
    }
    reader.finish().unwrap();
    for tag in [5, 254, 255] {
        assert!(
            Reader::new(&[tag], &limits)
                .unwrap()
                .storage_source(0)
                .is_err()
        );
    }
}
#[test]
fn complete_leaf_and_mode_survive_transport_and_bound_type_depth() {
    let leaf = Type::BorrowedStorage {
        storage: Box::new(Type::Record {
            name: "qualified".into(),
            fields: vec![(
                "ptr".into(),
                Type::Address {
                    mutable: false,
                    inner: Box::new(Type::I32),
                },
            )],
        }),
        life: StorageLifeShape {
            binder_count: 1,
            slots: vec![StorageLifeSlot {
                path: vec![PlaceProjection::Field(0)],
                binder: 0,
                mode: StoredBorrowMode::SharedAddress,
            }],
            retained: vec![],
            nominal_arguments: vec![],
        },
    };
    for mode in [
        StoredBorrowMode::SharedAddress,
        StoredBorrowMode::ExclusiveAddress,
        StoredBorrowMode::ReadOnlyDescription,
        StoredBorrowMode::WritableDescription,
    ] {
        let source = StorageSource::Static {
            leaf: leaf.clone(),
            mode,
        };
        let limits = Limits::default();
        let mut writer = Writer::new(&limits);
        writer.storage_source(&source, 0).unwrap();
        let mut reader = Reader::new(&writer.output, &limits).unwrap();
        assert_eq!(reader.storage_source(0).unwrap(), source);
        reader.finish().unwrap();
        let limits = Limits {
            max_call_depth: 2,
            ..limits
        };
        let mut shallow = Writer::new(&limits);
        assert!(shallow.storage_source(&source, 0).is_err());
        assert!(
            Reader::new(&writer.output, &limits)
                .unwrap()
                .storage_source(0)
                .is_err()
        );
    }
}

#[test]
fn source_identity_distinguishes_regions_places_static_and_empty() {
    let position = StoragePosition {
        base: StorageBase::Result,
        path: vec![],
    };
    let baseline = retained_return_sources_fingerprint(&[StorageSourceBinding {
        position: position.clone(),
        sources: vec![],
    }]);
    let mut seen = std::collections::BTreeSet::new();
    for atom in atoms() {
        let key = retained_return_sources_fingerprint(&[StorageSourceBinding {
            position: position.clone(),
            sources: vec![atom],
        }]);
        assert_ne!(key, baseline);
        assert!(seen.insert(key));
    }
    let a = StorageSource::Static {
        leaf: Type::Unit,
        mode: StoredBorrowMode::SharedAddress,
    };
    let b = StorageSource::Static {
        leaf: Type::Unit,
        mode: StoredBorrowMode::ExclusiveAddress,
    };
    let c = StorageSource::Static {
        leaf: Type::Bool,
        mode: StoredBorrowMode::SharedAddress,
    };
    let keys = [a, b, c].map(|source| {
        retained_return_sources_fingerprint(&[StorageSourceBinding {
            position: position.clone(),
            sources: vec![source],
        }])
    });
    assert_ne!(keys[0], keys[1]);
    assert_ne!(keys[0], keys[2]);
}
