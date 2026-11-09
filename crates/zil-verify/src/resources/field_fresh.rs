//! Immutable SSA field coordinates; every State owns its dynamic field bytes.
use super::*;
// Flat clones pay two Arc reference operations and two data-buffer headers.
// At <=4 rows keep plain Vec headers and the old two-index field accesses.
const DENSE_ROWS: usize = 4;
fn invalid(message: &str) -> Diagnostic {
    reject("E-VERIFY-RESOURCE", None, message)
}
#[derive(Debug, PartialEq, Eq)]
struct Layout {
    offsets: Vec<usize>,
    width: usize,
}
fn layout(widths: &[usize], b: &mut Budget) -> Check<Option<Arc<Layout>>> {
    if widths.len() <= DENSE_ROWS {
        return Ok(None);
    }
    // Prefix capacity and writes (N+1 each), width reads and checked additions
    // (N each); Vec/Arc allocation and final primitive-buffer release headers.
    b.spend(widths.len().saturating_mul(4).saturating_add(6))?;
    let mut offsets = Vec::with_capacity(widths.len() + 1);
    offsets.push(0);
    let mut width = 0usize;
    for &n in widths {
        width = width
            .checked_add(n)
            .ok_or_else(|| limit("Package field inventory overflow"))?;
        offsets.push(width);
    }
    Ok(Some(Arc::new(Layout { offsets, width })))
}
// The only production constructor consumes the actual Slots width vector.
// Its immutable widths and offsets cannot be paired across different Slots.
pub(super) struct Plan {
    widths: Vec<usize>,
    shared: Option<Arc<Layout>>,
}
impl Plan {
    pub(super) fn new(widths: Vec<usize>, b: &mut Budget) -> Check<Self> {
        let shared = layout(&widths, b)?;
        Ok(Self { widths, shared })
    }
    pub(super) fn empty(&self, b: &mut Budget) -> Check<Fields> {
        Fields::new(&self.widths, self.shared.as_ref(), b)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Storage {
    Dense(Vec<Vec<u8>>),
    Flat { layout: Arc<Layout>, data: Vec<u8> },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Fields {
    storage: Storage,
}
impl Fields {
    fn new(widths: &[usize], shared: Option<&Arc<Layout>>, b: &mut Budget) -> Check<Self> {
        if let Some(layout) = shared {
            b.spend(1)?;
            if layout.offsets.len() != widths.len() + 1 {
                return Err(invalid("Package field row inventory differs"));
            }
            b.spend(layout.width.saturating_add(4))?; // bytes, Arc clone/release, data allocation/release
            Ok(Self {
                storage: Storage::Flat {
                    layout: layout.clone(),
                    data: vec![INPUT; layout.width],
                },
            })
        } else {
            // Preserve the actual old dense representation and fee path when
            // its row-header count is no larger than the shared flat headers.
            b.spend(widths.len())?;
            let mut rows = Vec::with_capacity(widths.len());
            for &width in widths {
                b.spend(width)?;
                rows.push(vec![INPUT; width]);
            }
            Ok(Self {
                storage: Storage::Dense(rows),
            })
        }
    }
    fn validate(&self, b: &mut Budget) -> Check<()> {
        if let Storage::Flat { layout, data } = &self.storage {
            b.spend(1)?;
            // Layout is private, immutable and constructed in actual SSA order.
            if layout.width != data.len() {
                return Err(invalid("Package field byte inventory differs"));
            }
        }
        Ok(())
    }
    pub(super) fn cost(&self, b: &mut Budget) -> Check<()> {
        match &self.storage {
            Storage::Dense(rows) => {
                b.spend(rows.len())?;
                for row in rows {
                    b.spend(row.len())?;
                }
            }
            Storage::Flat { data, .. } => {
                self.validate(b)?;
                b.spend(data.len().saturating_add(4))?;
            }
        }
        Ok(())
    }
    pub(super) fn get(&self, row: usize, field: usize, b: &mut Budget) -> Check<Option<u8>> {
        match &self.storage {
            Storage::Dense(rows) => {
                b.spend(2)?;
                Ok(rows.get(row).and_then(|r| r.get(field)).copied())
            }
            Storage::Flat { layout, data } => {
                self.validate(b)?;
                b.spend(5)?; // row bound, two offsets, field bound and byte read
                if row >= layout.offsets.len() - 1 {
                    return Ok(None);
                }
                let start = layout.offsets[row];
                let end = layout.offsets[row + 1];
                if field >= end - start {
                    return Ok(None);
                }
                Ok(Some(data[start + field]))
            }
        }
    }
    pub(super) fn value(&self, row: usize, field: usize, b: &mut Budget) -> Check<u8> {
        self.get(row, field, b)?
            .ok_or_else(|| invalid("Package field provenance absent"))
    }
    pub(super) fn set(&mut self, row: usize, field: usize, value: u8, b: &mut Budget) -> Check<()> {
        self.validate(b)?;
        match &mut self.storage {
            Storage::Dense(rows) => {
                b.spend(3)?;
                let at = rows
                    .get_mut(row)
                    .and_then(|r| r.get_mut(field))
                    .ok_or_else(|| invalid("Package field provenance absent"))?;
                *at = value;
            }
            Storage::Flat { layout, data } => {
                b.spend(6)?; // row bound, two offsets, field bound and indexed write
                if row >= layout.offsets.len() - 1 {
                    return Err(invalid("Package field provenance absent"));
                }
                let start = layout.offsets[row];
                let end = layout.offsets[row + 1];
                if field >= end - start {
                    return Err(invalid("Package field provenance absent"));
                }
                data[start + field] = value;
            }
        }
        Ok(())
    }
    pub(super) fn merge(&mut self, other: &Self, b: &mut Budget) -> Check<bool> {
        self.validate(b)?;
        other.validate(b)?;
        // Source cost already paid the same actual byte scan as the former
        // flatten().zip() loop. Do NOT require equal private row shapes: the
        // old flattened zip preserves the receiver's complete trailing bytes.
        fn join<'a>(
            left: impl Iterator<Item = &'a mut u8>,
            right: impl Iterator<Item = &'a u8>,
        ) -> bool {
            let mut changed = false;
            for (a, b) in left.zip(right) {
                let next = *a | *b;
                changed |= next != *a;
                *a = next;
            }
            changed
        }
        Ok(match (&mut self.storage, &other.storage) {
            (Storage::Dense(a), Storage::Dense(b)) => {
                join(a.iter_mut().flatten(), b.iter().flatten())
            }
            (Storage::Dense(a), Storage::Flat { data: from, .. }) => {
                // A flat source no longer pays any source row-header walk.
                // Prepay the ACTUAL target dense headers (including empty
                // rows) before its flatten iterator searches for a byte.
                b.spend(a.len())?;
                join(a.iter_mut().flatten(), from.iter())
            }
            (Storage::Flat { data: a, .. }, Storage::Dense(b)) => {
                join(a.iter_mut(), b.iter().flatten())
            }
            (Storage::Flat { data: a, .. }, Storage::Flat { data: b, .. }) => {
                join(a.iter_mut(), b.iter())
            }
        })
    }
    #[cfg(test)]
    pub(super) fn dense(&self) -> Vec<Vec<u8>> {
        match &self.storage {
            Storage::Dense(rows) => rows.clone(),
            Storage::Flat { layout, data } => layout
                .offsets
                .windows(2)
                .map(|range| data[range[0]..range[1]].to_vec())
                .collect(),
        }
    }
    #[cfg(test)]
    pub(super) fn from_dense(rows: &[Vec<u8>], b: &mut Budget) -> Check<Self> {
        let widths: Vec<_> = rows.iter().map(Vec::len).collect();
        let shared = layout(&widths, b)?;
        let mut out = Self::new(&widths, shared.as_ref(), b)?;
        for (row, fields) in rows.iter().enumerate() {
            for (field, &value) in fields.iter().enumerate() {
                out.set(row, field, value, b)?;
            }
        }
        Ok(out)
    }
}
#[cfg(test)]
mod tests;
