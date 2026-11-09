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
