//! Only a complete empty Rows value is absent. Logical local coordinates and
//! every path/constraint discovery order remain those of the dense proof.
use super::*;
static EMPTY: Rows = Vec::new();
#[derive(Debug, PartialEq, Eq)]
pub(super) struct LocalRows {
    width: usize,
    entries: Vec<(usize, Rows)>,
}
impl LocalRows {
    pub(super) fn new(width: usize, b: &mut Budget) -> Check<Self> {
        charge(b, 1)?;
        Ok(Self {
            width,
            entries: Vec::new(),
        })
    }
    fn locate(&self, id: usize, b: &mut Budget) -> Check<Result<usize, usize>> {
        if id >= self.width {
            return Err(reject(
                "E-VERIFY-HEAD-ACCESS",
                None,
                "local row index exceeds actual local width",
            ));
        }
        charge(
            b,
            self.entries
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
        )?;
        Ok(self.entries.binary_search_by_key(&id, |(key, _)| *key))
    }
    pub(super) fn get(&self, id: usize, b: &mut Budget) -> Check<&Rows> {
        Ok(match self.locate(id, b)? {
            Ok(at) => &self.entries[at].1,
            Err(_) => &EMPTY,
        })
    }
    fn insert(&mut self, at: usize, id: usize, row: Rows, b: &mut Budget) -> Check<()> {
        charge(b, self.entries.len() - at + 1)?;
        if self.entries.len() == self.entries.capacity() {
            charge(b, self.entries.len().saturating_add(1))?;
        }
        self.entries.insert(at, (id, row));
        Ok(())
    }
    pub(super) fn set(&mut self, id: usize, row: Rows, b: &mut Budget) -> Check<()> {
        match (self.locate(id, b)?, row.is_empty()) {
            (Ok(at), true) => {
                charge(b, self.entries.len() - at)?;
                rows_cost(&self.entries[at].1, b)?;
                self.entries.remove(at);
            }
            (Ok(at), false) => {
                charge(b, 1)?;
                rows_cost(&self.entries[at].1, b)?;
                self.entries[at].1 = row;
            }
            (Err(at), false) => {
                self.insert(at, id, row, b)?;
            }
            (Err(_), true) => {}
        }
        Ok(())
    }
    pub(super) fn duplicate(&self, b: &mut Budget) -> Check<Self> {
        charge(b, self.entries.len().saturating_add(1))?;
        let mut entries = Vec::with_capacity(self.entries.len());
        for (id, row) in &self.entries {
            entries.push((*id, super::duplicate(row, b)?));
        }
        Ok(Self {
            width: self.width,
            entries,
        })
    }
    pub(super) fn merge_from(&mut self, other: &Self, b: &mut Budget) -> Check<()> {
        if self.width != other.width {
            return Err(reject(
                "E-VERIFY-HEAD-ACCESS",
                None,
                "local tables have different logical widths",
            ));
        }
        charge(b, other.entries.len().saturating_add(1))?;
        for (id, from) in &other.entries {
            match self.locate(*id, b)? {
                Ok(at) => {
                    merge(&mut self.entries[at].1, from, b)?;
                }
                Err(at) => {
                    // Run the original row union even on an absent target;
                    // no path/constraint normalization is introduced here.
                    let mut row = Rows::new();
                    merge(&mut row, from, b)?;
                    if !row.is_empty() {
                        self.insert(at, *id, row, b)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn same(&self, other: &Self, b: &mut Budget) -> Check<bool> {
        charge(
            b,
            self.entries
                .len()
                .saturating_add(other.entries.len())
                .saturating_add(1),
        )?;
        for (_, row) in self.entries.iter().chain(&other.entries) {
            rows_cost(row, b)?;
        }
        Ok(self == other)
    }
    #[cfg(test)]
    pub(super) fn dense(&self) -> Vec<Rows> {
        let mut out = vec![Rows::new(); self.width];
        for (id, row) in &self.entries {
            out[*id] = row.clone();
        }
        out
    }
}
