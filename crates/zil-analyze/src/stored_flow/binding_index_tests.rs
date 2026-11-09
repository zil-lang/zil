use super::*;
#[allow(dead_code)]
mod fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/language-readiness/terminal/higher-rank-borrows/stored_flow_fixture.rs"
    ));
}
#[test]
fn physical_index_keeps_complete_original_binding_rules_and_errors() {
    for module in [
        fixture::module(),
        fixture::mutable_holder(),
        fixture::shared_two_slots(),
        fixture::wide_shared(8),
    ] {
        for f in &module.functions {
            for enums in [false, true] {
                let mut indexed = Budget::new(&Limits::default());
                indexed.stored_enum_addresses = enums;
                for base in [
                    StorageBase::Result,
                    StorageBase::Parameter(0),
                    StorageBase::Parameter(9),
                ] {
                    for path in [
                        vec![],
                        vec![PlaceProjection::Field(0)],
                        vec![PlaceProjection::Field(1), PlaceProjection::Field(0)],
                        vec![PlaceProjection::ErrorArm(false), PlaceProjection::Field(0)],
                    ] {
                        let mut old = Budget::new(&Limits::default());
                        old.stored_enum_addresses = enums;
                        assert_eq!(
                            binding(f, base, &path, &mut indexed),
                            binding_by_scan(f, base, &path, &mut old),
                            "complete binding and diagnostics"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn queried_duplicate_and_unrelated_duplicate_preserve_binding_policy() {
    for duplicate in [
        vec![PlaceProjection::Field(0)],
        vec![PlaceProjection::Field(99)],
    ] {
        let mut f = fixture::module().functions.remove(0);
        let Type::BorrowedStorage { life, .. } = &mut f.return_type else {
            panic!("qualified return")
        };
        let mut row = life.slots[0].clone();
        row.path = duplicate;
        life.slots.push(row.clone());
        life.slots.push(row);
        for path in [
            vec![PlaceProjection::Field(0)],
            vec![PlaceProjection::Field(99)],
            vec![PlaceProjection::Field(1)],
        ] {
            let mut indexed = Budget::new(&Limits::default());
            let mut old = Budget::new(&Limits::default());
            assert_eq!(
                binding(&f, StorageBase::Result, &path, &mut indexed),
                binding_by_scan(&f, StorageBase::Result, &path, &mut old)
            );
        }
    }
}
