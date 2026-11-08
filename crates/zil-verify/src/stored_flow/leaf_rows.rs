//! A lookup index over the original, ordered physical leaf rows.
//! The index carries no permission and never changes iteration or first-match
//! semantics. Small payloads keep their previous representation and fees.
use super::*;

const INDEX_THRESHOLD: usize = 32;

#[derive(Clone, Debug, Default)]
pub(super) struct Rows {
    rows: Vec<(Path, Permission)>,
    index: Option<Index>,
}

#[derive(Clone, Debug)]
struct Index {
    positions: Vec<(Path, usize)>,
    // Table entries plus the complete cloned keys, also used before deletion.
    copy_work: usize,
}

impl PartialEq for Rows {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
    }
}
impl Eq for Rows {}

impl std::ops::Deref for Rows {
    type Target = [(Path, Permission)];
    fn deref(&self) -> &Self::Target {
        &self.rows
    }
}
impl<'a> IntoIterator for &'a Rows {
    type Item = &'a (Path, Permission);
    type IntoIter = std::slice::Iter<'a, (Path, Permission)>;
    fn into_iter(self) -> Self::IntoIter {
        self.rows.iter()
    }
}

impl Index {
    fn search(&self, path: &[PlaceProjection], b: &mut Budget) -> Check<Result<usize, usize>> {
        let mut low = 0;
        let mut high = self.positions.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let existing = &self.positions[mid].0;
            // Prepay each real comparison, including a deep common prefix.
            b.spend(existing.len().saturating_add(path.len()).saturating_add(1))?;
            match existing.as_slice().cmp(path) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => return Ok(Ok(mid)),
            }
        }
        Ok(Err(low))
    }
    fn insert(&mut self, path: &[PlaceProjection], row: usize, b: &mut Budget) -> Check<()> {
        // Keep the oldest ordinal if direct construction repeats a key.
        if let Err(at) = self.search(path, b)? {
            let shift = self.positions.len().saturating_sub(at);
            let growth = if self.positions.len() == self.positions.capacity() {
                self.positions.len()
            } else {
                0
            };
            b.spend(
                path.len()
                    .saturating_add(1)
                    .saturating_add(shift)
                    .saturating_add(growth),
            )?;
            self.positions.insert(at, (path.to_vec(), row));
            self.copy_work = self.copy_work.saturating_add(path.len().saturating_add(1));
        }
        Ok(())
    }
    fn build<'a>(
        rows: impl Iterator<Item = &'a (Path, Permission)>,
        b: &mut Budget,
    ) -> Check<Self> {
        let mut index = Self {
            positions: Vec::new(),
            copy_work: 0,
        };
        for (ordinal, (path, _)) in rows.enumerate() {
            b.spend(1)?;
            index.insert(path, ordinal, b)?;
        }
        Ok(index)
    }
}

impl Rows {
    pub(super) fn copy_work(&self, b: &mut Budget) -> Check<()> {
        if let Some(index) = &self.index {
            b.spend(index.copy_work)?;
        }
        Ok(())
    }
    pub(super) fn position(
        &self,
        path: &[PlaceProjection],
        b: &mut Budget,
    ) -> Check<Option<usize>> {
        if let Some(index) = &self.index {
            let found = index.search(path, b)?.ok();
            b.spend(1)?;
            Ok(found.map(|at| index.positions[at].1))
        } else {
            b.spend(amount(
                self.rows.len().saturating_add(1),
                path.len().saturating_add(1),
            )?)?;
            Ok(self.rows.iter().position(|(existing, _)| existing == path))
        }
    }
    pub(super) fn permission_mut(&mut self, row: usize) -> &mut Permission {
        &mut self.rows[row].1
    }
    pub(super) fn push(&mut self, row: (Path, Permission), b: &mut Budget) -> Check<()> {
        if let Some(index) = &mut self.index {
            index.insert(&row.0, self.rows.len(), b)?;
        } else if self.rows.len().saturating_add(1) >= INDEX_THRESHOLD {
            // Build and pay before adding the row; failed prepayment cannot
            // expose a partially indexed new permission.
            self.index = Some(Index::build(
                self.rows.iter().chain(std::iter::once(&row)),
                b,
            )?);
        }
        self.rows.push(row);
        Ok(())
    }
    pub(super) fn retain(
        &mut self,
        keep: impl FnMut(&(Path, Permission)) -> bool,
        b: &mut Budget,
    ) -> Check<()> {
        self.copy_work(b)?;
        self.index = None;
        self.rows.retain(keep);
        if self.rows.len() >= INDEX_THRESHOLD {
            self.index = Some(Index::build(self.rows.iter(), b)?);
        }
        Ok(())
    }
    pub(super) fn into_vec(self, b: &mut Budget) -> Check<Vec<(Path, Permission)>> {
        self.copy_work(b)?;
        Ok(self.rows)
    }
    pub(super) fn from_vec(rows: Vec<(Path, Permission)>, b: &mut Budget) -> Check<Self> {
        let index = if rows.len() >= INDEX_THRESHOLD {
            Some(Index::build(rows.iter(), b)?)
        } else {
            None
        };
        Ok(Self { rows, index })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ordinal: usize) -> (Path, Permission) {
        (
            vec![PlaceProjection::Field(ordinal as u32)],
            Permission {
                sources: Vec::new(),
                loans: vec![ordinal],
            },
        )
    }

    #[test]
    fn lookup_keeps_first_match_order_after_clone_delete_and_reinsert() {
        let mut b = Budget::new(&Limits::default());
        let mut rows = Rows::default();
        let order: Vec<_> = (0..48).map(|i| i * 17 % 48).collect();
        for ordinal in &order {
            rows.push(row(*ordinal), &mut b).unwrap();
        }
        rows.push(row(17), &mut b).unwrap();
        assert_eq!(
            rows.iter().map(|(_, p)| p.loans[0]).collect::<Vec<_>>(),
            [order.clone(), vec![17]].concat()
        );
        rows.copy_work(&mut b).unwrap();
        let mut copied = rows.clone();
        copied.retain(|(_, p)| p.loans[0] != 0, &mut b).unwrap();
        copied.push(row(0), &mut b).unwrap();
        for ordinal in 0..48 {
            let path = [PlaceProjection::Field(ordinal)];
            let actual = copied.position(&path, &mut b).unwrap();
            assert_eq!(actual, copied.iter().position(|(p, _)| p == &path));
            assert_eq!(copied[actual.unwrap()].1.loans, [ordinal as usize]);
        }
        let plain = copied.clone().into_vec(&mut b).unwrap();
        assert_eq!(Rows::from_vec(plain, &mut b).unwrap(), copied);
        assert_eq!(
            rows.position(&[PlaceProjection::Field(17)], &mut b)
                .unwrap(),
            Some(1)
        );
    }

    #[test]
    fn threshold_index_growth_requires_prepaid_work() {
        let mut b = Budget::new(&Limits {
            max_analysis_steps: 0,
            ..Limits::default()
        });
        let mut rows = Rows::default();
        for ordinal in 0..INDEX_THRESHOLD - 1 {
            rows.push(row(ordinal), &mut b).unwrap();
        }
        let before = rows.clone();
        assert!(rows.push(row(INDEX_THRESHOLD - 1), &mut b).is_err());
        assert_eq!(rows, before);
        assert!(rows.index.is_none());
    }

    #[test]
    fn deep_common_prefix_comparison_is_prepaid_in_full() {
        let path = vec![PlaceProjection::Field(0); 17];
        let mut rows = Rows::default();
        let mut b = Budget::new(&Limits::default());
        for ordinal in 0..INDEX_THRESHOLD {
            rows.push((path.clone(), row(ordinal).1), &mut b).unwrap();
        }
        // One unique indexed key: both complete paths (17 + 17), the
        // comparison visit and the final vector-position read cost 36.
        let mut exact = Budget::new(&Limits {
            max_analysis_steps: 36,
            ..Limits::default()
        });
        assert_eq!(rows.position(&path, &mut exact).unwrap(), Some(0));
        let mut short = Budget::new(&Limits {
            max_analysis_steps: 35,
            ..Limits::default()
        });
        assert!(rows.position(&path, &mut short).is_err());
    }
}
