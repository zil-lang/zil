//! Per-State retirement sources: only the actual copied page is detached.
use super::*;
const PAGE: usize = 16;
// One shared page's shallow clone has two Arc operations plus Vec/len headers.
// Four or fewer empty rows are cheaper in their actual original representation.
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
    b.spend(1)?; // actual strong-count test
    if Arc::strong_count(page) > 1 {
        // Page length read, all actual rows, Vec/Arc create/release headers. Every deep
        // Source copy also prepays its eventual deep cleanup, not a later Drop.
        b.spend(page.len().saturating_add(5))?;
        for source in page.iter() {
            source.cost(b)?;
            source.cost(b)?;
        }
    }
    Ok(Arc::make_mut(page))
}
// A sufficient proof that the original union leaves every Source field intact.
// Compare complete ordered recipes, including base, full path and mutability.
fn keeps_targets(to: &actual::Targets, from: &actual::Targets, b: &mut Budget) -> Check<bool> {
    b.spend(to.len().saturating_add(from.len()))?;
    let mut existing = to.iter();
    let mut next = existing.next();
    for incoming in from {
        loop {
            let Some(current) = next else {
                return Ok(false);
            };
            b.spend(
                current
                    .steps
                    .len()
                    .saturating_add(incoming.steps.len())
                    .saturating_add(2),
            )?;
            match current.cmp(incoming) {
                std::cmp::Ordering::Less => next = existing.next(),
                std::cmp::Ordering::Equal => {
                    next = existing.next();
                    break;
                }
                std::cmp::Ordering::Greater => return Ok(false),
            }
        }
    }
    Ok(true)
}
fn keeps_sources(to: &Sources, from: &Sources, b: &mut Budget) -> Check<bool> {
    let Sources { direct, captures } = to;
    let Sources {
        direct: incoming_direct,
        captures: incoming_captures,
    } = from;
    if !keeps_targets(direct, incoming_direct, b)? {
        return Ok(false);
    }
    b.spend(captures.len().saturating_add(incoming_captures.len()))?;
    let mut existing = captures.iter();
    let mut next = existing.next();
    for (incoming_key, incoming) in incoming_captures {
        loop {
            let Some((key, current)) = next else {
                return Ok(false);
            };
            b.spend(1)?;
            match key.cmp(incoming_key) {
                std::cmp::Ordering::Less => next = existing.next(),
                std::cmp::Ordering::Equal => {
                    if !keeps_targets(current, incoming, b)? {
                        return Ok(false);
                    }
                    next = existing.next();
                    break;
                }
                std::cmp::Ordering::Greater => return Ok(false),
            }
        }
    }
    // An absent capture key must fail even if its incoming row is empty:
    // merge_captures inserts that key without reporting a changed target set.
    Ok(true)
}
fn keeps_page(to: &[Sources], from: &[Sources], b: &mut Budget) -> Check<bool> {
    b.spend(1)?;
    if to.len() != from.len() {
        return Ok(false);
    }
    b.spend(to.len().saturating_add(from.len()))?;
    for (target, source) in to.iter().zip(from) {
        if !keeps_sources(target, source, b)? {
            return Ok(false);
        }
    }
    Ok(true)
}
impl Rows {
    pub(in crate::resources) fn new(len: usize, b: &mut Budget) -> Check<Self> {
        if len <= SMALL {
            b.spend(len)?;
            return Ok(Self {
                storage: Storage::Dense(vec![Sources::default(); len]),
            });
        }
        let count = len.div_ceil(PAGE);
        // Source initialization; outer capacity and writes; each page's Arc
        // and Vec allocation/release headers; outer Vec headers and logical len.
        b.spend(
            len.saturating_add(count.saturating_mul(6))
                .saturating_add(3),
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
    pub(in crate::resources) fn len(&self) -> usize {
        match &self.storage {
            Storage::Dense(rows) => rows.len(),
            Storage::Pages { len, .. } => *len,
        }
    }
    pub(in crate::resources) fn full_cost(&self, b: &mut Budget) -> Check<()> {
        match &self.storage {
            Storage::Dense(rows) => {
                b.spend(rows.len())?;
                for source in rows {
                    source.cost(b)?;
                }
            }
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
        match &self.storage {
            Storage::Dense(_) => self.full_cost(b),
            Storage::Pages { pages, .. } => {
                b.spend(pages.len().saturating_mul(2).saturating_add(3))
            }
        }
    }
    pub(in crate::resources) fn get(&self, index: usize, b: &mut Budget) -> Check<&Sources> {
        let count = self.len();
        match &self.storage {
            Storage::Dense(rows) => rows.get(index).ok_or_else(absent),
            Storage::Pages { pages, .. } => {
                b.spend(3)?; // logical bound, page and row
                if index >= count {
                    return Err(absent());
                }
                Ok(&pages[index / PAGE][index % PAGE])
            }
        }
    }
    // Only the three checked assignment methods below expose mutable sources.
    // Their caller has already evaluated the original RHS at the original point.
    fn row_mut(&mut self, index: usize, b: &mut Budget) -> Check<&mut Sources> {
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
                    b.spend(1)?; // actual strong-count test before the read-only proof
                    if Arc::strong_count(target) > 1 && keeps_page(target, source, b)? {
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
