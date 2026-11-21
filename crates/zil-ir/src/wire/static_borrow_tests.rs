//! Constant positions are transported explicitly; data transport grants no life.
use super::*;
#[test]
fn constant_occurrences_have_an_independent_byte_oracle_and_identity() {
    let limits = Limits::default();
    let life = StorageLifeContract {
        fixed_binder_count: 0,
        invocation_binder_count: 0,
        bindings: vec![],
        static_bindings: vec![
            StoragePosition {
                base: StorageBase::Capture(3),
                path: vec![],
            },
            StoragePosition {
                base: StorageBase::Result,
                path: vec![],
            },
        ],
        classes: vec![],
    };
    let mut expected = vec![];
    for n in [0u32, 0, 0, 2] {
        expected.extend_from_slice(&n.to_le_bytes());
    }
    expected.push(1);
    expected.extend_from_slice(&3u32.to_le_bytes());
    expected.extend_from_slice(&0u32.to_le_bytes());
    expected.push(2);
    expected.extend_from_slice(&0u32.to_le_bytes());
    expected.extend_from_slice(&0u32.to_le_bytes());
    let mut writer = Writer::new(&limits);
    writer.required_storage_contract(&life).unwrap();
    assert_eq!(writer.output, expected);
    let mut reader = Reader::new(&expected, &limits).unwrap();
    assert_eq!(reader.required_storage_contract().unwrap(), life);
    reader.finish().unwrap();
    assert!(
        life.to_flat(0).is_none(),
        "constant positions cannot disappear into legacy flat relations"
    );
    let ty = |life| Type::BorrowedFunctionPointer {
        parameters: vec![],
        result: Box::new(Type::I32),
        life,
    };
    let mut changed = life.clone();
    changed.static_bindings[0].base = StorageBase::Parameter(3);
    assert_ne!(ty(life.clone()).fingerprint(), ty(changed).fingerprint());
    let mut empty = life.clone();
    empty.static_bindings.clear();
    assert_ne!(ty(life.clone()).fingerprint(), ty(empty).fingerprint());
    let needed = crate::storage_relation_metadata_work(&life, 100, 100, 100_000).unwrap();
    assert_eq!(
        crate::storage_relation_metadata_work(&life, 100, 100, needed),
        Some(needed)
    );
    assert_eq!(
        crate::storage_relation_metadata_work(&life, 100, 100, needed - 1),
        None
    );
    assert_eq!(
        crate::storage_relation_metadata_work(&life, 100, 100, needed),
        Some(needed)
    );
}
