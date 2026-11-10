//! Per-State retirement sources: only the actual copied page is detached.
use super::*;
const PAGE: usize = 16;
// Keep the small-domain representation; its added enum metadata is still paid.
const SMALL: usize = 4;
#[derive(Clone, Debug, PartialEq, Eq)]
enum Storage {
    Dense(Vec<Sources>),
    Pages {
        len: usize,
        pages: Vec<Arc<Vec<Sources>>>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::resources) struct Rows {
    storage: Storage,
}
fn words<T>() -> usize {
    std::mem::size_of::<T>().div_ceil(std::mem::size_of::<usize>())
}
fn dense_extra() -> usize {
    words::<Rows>().saturating_sub(words::<Vec<Sources>>())
}
fn flat_cost(rows: &[Sources], b: &mut Budget) -> Check<()> {
    b.spend(rows.len())?;
    for source in rows {
        source.cost(b)?;
    }
    Ok(())
}
fn absent() -> Diagnostic {
    reject("E-VERIFY-RESOURCE", None, "retirement source slot absent")
}
fn capture_cost(captures: &actual::Captures, b: &mut Budget) -> Check<()> {
    b.spend(captures.len())?;
    for sources in captures.values() {
        actual::charge_set(sources, b)?;
    }
    Ok(())
}
fn unique_page<'a>(page: &'a mut Arc<Vec<Sources>>, b: &mut Budget) -> Check<&'a mut Vec<Sources>> {
    b.spend(2)?; // explicit strong-count guard and make_mut entry
    if Arc::strong_count(page) > 1 {
        // Visit and copy every complete Source record, including empty maps.
        // Prepay actual page headers, both new counters, old-Arc release, final
        // strong/weak release and two allocations/frees. Dynamic Source copy
        // and eventual deep cleanup are charged independently before make_mut.
        b.spend(
            page.len()
                .saturating_mul(words::<Sources>().saturating_mul(2).saturating_add(1))
                .saturating_add(words::<Vec<Sources>>().saturating_mul(2))
                .saturating_add(words::<Arc<Vec<Sources>>>().saturating_mul(2))
                .saturating_add(9),
        )?;
        for source in page.iter() {
            source.cost(b)?;
            source.cost(b)?;
        }
    }
    Ok(Arc::make_mut(page))
}
impl Rows {
    pub(in crate::resources) fn new(len: usize, b: &mut Budget) -> Check<Self> {
        if len <= SMALL {
            b.spend(len.saturating_add(dense_extra()).saturating_add(2))?;
            return Ok(Self {
                storage: Storage::Dense(vec![Sources::default(); len]),
            });
        }
        let count = len.div_ceil(PAGE);
        // Original Source initialization plus actual Arc slot and page Vec
        // header read/write. Eight operations cover two counter initializations,
        // final strong/weak release and both page allocations/frees. Rows holds
        // the outer Vec and length: pay its full header, creation/drop choices
        // and outer allocation/free, without counting a second outer Vec.
        let page = words::<Arc<Vec<Sources>>>()
            .saturating_mul(2)
            .saturating_add(words::<Vec<Sources>>().saturating_mul(2))
            .saturating_add(8);
        b.spend(
            len.saturating_add(count.saturating_mul(page))
                .saturating_add(words::<Rows>().saturating_mul(2))
                .saturating_add(4),
        )?;
        let mut pages = Vec::with_capacity(count);
        for index in 0..count {
            let width = PAGE.min(len - index * PAGE);
            pages.push(Arc::new(vec![Sources::default(); width]));
        }
        Ok(Self {
            storage: Storage::Pages { len, pages },
        })
    }
    pub(in crate::resources) fn full_cost(&self, b: &mut Budget) -> Check<()> {
        b.spend(1)?; // actual storage dispatch
        match &self.storage {
            Storage::Dense(rows) => flat_cost(rows, b)?,
            Storage::Pages { len, pages } => {
                // Preserve every old ordinal visit and Source scan for merge.
                // Page records are an additional real traversal, not a discount.
                b.spend(len.saturating_add(pages.len()).saturating_add(1))?;
                for page in pages {
                    for source in page.iter() {
                        source.cost(b)?;
                    }
                }
            }
        }
        Ok(())
    }
    pub(in crate::resources) fn copy_cost(&self, b: &mut Budget) -> Check<()> {
        b.spend(3)?; // cost dispatch, derived clone dispatch, eventual enum drop
        match &self.storage {
            Storage::Dense(rows) => {
                b.spend(dense_extra().saturating_mul(2))?;
                flat_cost(rows, b)
            }
            Storage::Pages { pages, .. } => b.spend(
                pages
                    .len()
                    .saturating_mul(
                        words::<Arc<Vec<Sources>>>()
                            .saturating_mul(2)
                            .saturating_add(2),
                    )
                    .saturating_add(words::<Rows>().saturating_mul(2))
                    .saturating_add(2), // outer Vec allocation/free
            ),
        }
    }
    pub(in crate::resources) fn get(&self, index: usize, b: &mut Budget) -> Check<&Sources> {
        b.spend(1)?; // actual storage dispatch
        match &self.storage {
            Storage::Dense(rows) => rows.get(index).ok_or_else(absent),
            Storage::Pages { len, pages } => {
                b.spend(3)?; // logical bound, page and row
                if index >= *len {
                    return Err(absent());
                }
                Ok(&pages[index / PAGE][index % PAGE])
            }
        }
    }
    // Only the three checked assignment methods below expose mutable sources.
    // Their caller has already evaluated the original RHS at the original point.
    fn row_mut(&mut self, index: usize, b: &mut Budget) -> Check<&mut Sources> {
        b.spend(1)?; // actual storage dispatch
        match &mut self.storage {
            Storage::Dense(rows) => rows.get_mut(index).ok_or_else(absent),
            Storage::Pages { len, pages } => {
                b.spend(4)?; // logical bound, page, row, eventual assignment
                if index >= *len {
                    return Err(absent());
                }
                let page = unique_page(&mut pages[index / PAGE], b)?;
                Ok(&mut page[index % PAGE])
            }
        }
    }
    pub(super) fn replace(&mut self, index: usize, value: Sources, b: &mut Budget) -> Check<()> {
        b.spend(1)?; // representation choice, including Dense
        if matches!(self.storage, Storage::Pages { .. }) {
            value.cost(b)?;
        } // eventual deep cleanup
        *self.row_mut(index, b)? = value;
        Ok(())
    }
    pub(super) fn direct(
        &mut self,
        index: usize,
        value: actual::Targets,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(1)?; // representation choice, including Dense
        if matches!(self.storage, Storage::Pages { .. }) {
            actual::charge_set(&value, b)?;
        }
        self.row_mut(index, b)?.direct = value;
        Ok(())
    }
    pub(super) fn captures(
        &mut self,
        index: usize,
        value: actual::Captures,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(1)?; // representation choice, including Dense
        if matches!(self.storage, Storage::Pages { .. }) {
            capture_cost(&value, b)?;
        }
        self.row_mut(index, b)?.captures = value;
        Ok(())
    }
    pub(in crate::resources) fn merge(&mut self, other: &Self, b: &mut Budget) -> Check<bool> {
        fn merge_rows(
            a: &mut [Sources],
            from: &[Sources],
            paged: bool,
            b: &mut Budget,
        ) -> Check<bool> {
            let mut changed = false;
            for (target, source) in a.iter_mut().zip(from) {
                // Newly inserted variable-width recipes may survive in an Arc
                // page. Prepay cleanup before the unchanged actual merge body.
                if paged {
                    source.cost(b)?;
                }
                changed |= target.merge(source, b)?;
            }
            Ok(changed)
        }
        b.spend(2)?; // both actual storage variants
        let mut changed = false;
        match (&mut self.storage, &other.storage) {
            (Storage::Dense(a), Storage::Dense(from)) => changed |= merge_rows(a, from, false, b)?,
            (Storage::Dense(a), Storage::Pages { pages, .. }) => {
                for (target, source) in a.chunks_mut(PAGE).zip(pages) {
                    b.spend(2)?; // actual target chunk and source page records
                    changed |= merge_rows(target, source, false, b)?;
                }
            }
            (Storage::Pages { pages, .. }, Storage::Dense(from)) => {
                for (target, source) in pages.iter_mut().zip(from.chunks(PAGE)) {
                    b.spend(2)?;
                    changed |= merge_rows(unique_page(target, b)?, source, true, b)?;
                }
            }
            (Storage::Pages { pages, .. }, Storage::Pages { pages: from, .. }) => {
                for (target, source) in pages.iter_mut().zip(from) {
                    b.spend(2)?;
                    b.spend(1)?; // actual immutable Arc page identity comparison
                    if Arc::ptr_eq(target, source) {
                        continue;
                    }
                    changed |= merge_rows(unique_page(target, b)?, source, true, b)?;
                }
            }
        }
        Ok(changed)
    }
    #[cfg(test)]
    pub(in crate::resources) fn from_vec(values: Vec<Sources>, b: &mut Budget) -> Check<Self> {
        let mut rows = Self::new(values.len(), b)?;
        for (index, value) in values.into_iter().enumerate() {
            rows.replace(index, value, b)?;
        }
        Ok(rows)
    }
    #[cfg(test)]
    pub(in crate::resources) fn dense(&self) -> Vec<Sources> {
        match &self.storage {
            Storage::Dense(rows) => rows.clone(),
            Storage::Pages { pages, .. } => pages.iter().flat_map(|p| p.iter().cloned()).collect(),
        }
    }
}
#[cfg(test)]
mod tests;

#[cfg(test)]
mod identity_tests;
