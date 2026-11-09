//! Request-local search structure over actual immutable module shape metadata.
//! A row is not a qualification, class solution, source or loan certificate.
use super::*;
use zil_ir::{PlaceProjection, StorageLifeShape};

impl Budget {
    /// Callers borrow a shape from this request's frozen TypedModule. Preserve
    /// first-row order and duplicates separately: each consumer retains its own
    /// existing policy, including rejecting only a queried duplicate path.
    pub(crate) fn physical_slot(
        &mut self,
        shape: &StorageLifeShape,
        path: &[PlaceProjection],
    ) -> CompileResult<Option<(u32, bool)>> {
        let key = shape as *const StorageLifeShape as usize;
        self.lookup(
            self.physical_slot_paths.len(),
            "physical slot cache presence",
        )?;
        if !self.physical_slot_paths.contains_key(&key) {
            self.spend(
                shape.slots.len().saturating_add(1),
                "physical slot index records",
            )?;
            let mut rows: BTreeMap<Vec<PlaceProjection>, (u32, bool)> = BTreeMap::new();
            for slot in &shape.slots {
                self.product(
                    rows.len().checked_ilog2().map_or(1, |n| n as usize + 2),
                    slot.path.len().saturating_add(1),
                    1,
                    "physical slot index build search",
                )?;
                self.spend(
                    slot.path.len().saturating_add(1),
                    "physical slot index owned key",
                )?;
                match rows.entry(slot.path.clone()) {
                    std::collections::btree_map::Entry::Vacant(row) => {
                        row.insert((slot.binder, false));
                    }
                    std::collections::btree_map::Entry::Occupied(mut row) => {
                        row.get_mut().1 = true;
                    }
                }
            }
            // Publish only the complete index. A failed build leaves no row
            // that a later query could mistake for a complete shape scan.
            self.lookup(
                self.physical_slot_paths.len(),
                "physical slot cache publication",
            )?;
            self.spend(1, "physical slot cache record")?;
            self.physical_slot_paths.insert(key, rows);
        }
        self.lookup(
            self.physical_slot_paths.len(),
            "physical slot cache query size",
        )?;
        let count = self.physical_slot_paths[&key].len();
        self.product(
            count.checked_ilog2().map_or(1, |n| n as usize + 2),
            path.len().saturating_add(1),
            1,
            "physical slot exact path query",
        )?;
        self.lookup(
            self.physical_slot_paths.len(),
            "physical slot cache query fetch",
        )?;
        Ok(self.physical_slot_paths[&key].get(path).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zil_ir::{StorageLifeSlot, StoredBorrowMode, VariantProjection};
    fn shape(rows: Vec<(Vec<PlaceProjection>, u32)>) -> StorageLifeShape {
        StorageLifeShape {
            binder_count: 4,
            slots: rows
                .into_iter()
                .map(|(path, binder)| StorageLifeSlot {
                    path,
                    binder,
                    mode: StoredBorrowMode::SharedAddress,
                })
                .collect(),
            retained: vec![],
            nominal_arguments: vec![],
        }
    }
    fn scan(shape: &StorageLifeShape, path: &[PlaceProjection]) -> Option<(u32, bool)> {
        let mut found = None;
        for row in &shape.slots {
            if row.path == path {
                match &mut found {
                    None => found = Some((row.binder, false)),
                    Some((_, duplicate)) => *duplicate = true,
                }
            }
        }
        found
    }
    #[test]
    fn complete_paths_duplicates_and_first_discovery_equal_old_scans() {
        let a = vec![PlaceProjection::Field(1), PlaceProjection::Field(511)];
        let z = vec![PlaceProjection::Field(1), PlaceProjection::Field(0)];
        for second_binder in [0, 3] {
            let s = shape(vec![
                (a.clone(), 0),
                (z.clone(), 1),
                (a.clone(), second_binder),
                (
                    vec![PlaceProjection::ErrorArm(false), PlaceProjection::Field(1)],
                    2,
                ),
                (
                    vec![PlaceProjection::Variant(VariantProjection {
                        variant: 0,
                        index: 1,
                    })],
                    3,
                ),
            ]);
            let mut b = Budget::new(&Limits::default());
            for path in [
                a.clone(),
                z.clone(),
                vec![],
                vec![PlaceProjection::Field(1)],
                vec![PlaceProjection::Field(511), PlaceProjection::Field(1)],
                vec![PlaceProjection::ErrorArm(true), PlaceProjection::Field(1)],
                vec![PlaceProjection::ErrorArm(false), PlaceProjection::Field(1)],
                vec![PlaceProjection::Variant(VariantProjection {
                    variant: 0,
                    index: 1,
                })],
            ] {
                assert_eq!(b.physical_slot(&s, &path).unwrap(), scan(&s, &path));
            }
            assert_eq!(
                b.physical_slot(&s, &a).unwrap(),
                Some((0, true)),
                "duplicate with same binder still remains duplicate"
            );
            assert_eq!(
                b.physical_slot(&s, &z).unwrap(),
                Some((1, false)),
                "an unrelated duplicate does not reject this path"
            );
        }
    }
    #[test]
    fn shape_identity_and_new_requests_never_share_changed_metadata() {
        let path = vec![PlaceProjection::Field(1)];
        let mut first = Box::new(shape(vec![(path.clone(), 0)]));
        let second = Box::new(shape(vec![(path.clone(), 3)]));
        {
            let mut b = Budget::new(&Limits::default());
            assert_eq!(b.physical_slot(&first, &path).unwrap(), Some((0, false)));
            assert_eq!(b.physical_slot(&second, &path).unwrap(), Some((3, false)));
        }
        first.slots[0].binder = 2;
        let mut fresh = Budget::new(&Limits::default());
        assert_eq!(
            fresh.physical_slot(&first, &path).unwrap(),
            Some((2, false))
        );
        assert_eq!(
            fresh.physical_slot_paths.len(),
            1,
            "a new request reconstructs its own shape index"
        );
    }
    #[test]
    fn failed_build_never_publishes_a_partial_index() {
        let shape = shape(
            (0..512)
                .map(|i| {
                    (
                        vec![PlaceProjection::Field(1), PlaceProjection::Field(i)],
                        i % 4,
                    )
                })
                .collect(),
        );
        let path = [PlaceProjection::Field(1), PlaceProjection::Field(511)];
        let mut b = Budget::new(&Limits {
            max_analysis_steps: 1000,
            ..Limits::default()
        });
        assert!(b.physical_slot(&shape, &path).is_err());
        assert!(b.physical_slot_paths.is_empty());
        let mut fresh = Budget::new(&Limits::default());
        assert_eq!(
            fresh.physical_slot(&shape, &path).unwrap(),
            scan(&shape, &path)
        );
    }
}
