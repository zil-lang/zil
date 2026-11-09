//! Complete SSA positions. Shared pages change storage, never source permission.
use super::*;
const WIDTH: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Page {
    base: usize,
    roots: Vec<Arc<Content>>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Values {
    len: usize,
    pages: Vec<Arc<Page>>,
}
impl Values {
    pub(super) fn build(
        len: usize,
        budget: &mut Budget,
        mut root: impl FnMut(usize, &mut Budget) -> Check<Arc<Content>>,
    ) -> Check<Self> {
        let count = len.div_ceil(WIDTH);
        // This method owns the original N SSA reference allocation fee.
        // Additional real page references, headers and bases are prepaid.
        budget.spend(len)?;
        budget.spend(count)?;
        budget.spend(count)?;
        budget.spend(count)?;
        let mut pages = Vec::with_capacity(count);
        for index in 0..count {
            let base = index * WIDTH;
            let width = (len - base).min(WIDTH);
            let mut roots = Vec::with_capacity(width);
            for offset in 0..width {
                roots.push(root(base + offset, budget)?);
            }
            pages.push(Arc::new(Page { base, roots }));
        }
        Ok(Self { len, pages })
    }
    pub(super) fn page_count(&self) -> usize {
        self.pages.len()
    }
    fn validate(&self, budget: &mut Budget) -> Check<()> {
        budget.spend(1)?;
        if self.pages.len() != self.len.div_ceil(WIDTH) {
            return Err(bad(None, "Sum SSA page inventory differs"));
        }
        for (index, page) in self.pages.iter().enumerate() {
            budget.spend(3)?;
            let base = index * WIDTH;
            if page.base != base || page.roots.len() != (self.len - base).min(WIDTH) {
                return Err(bad(None, "Sum SSA page shape differs"));
            }
        }
        Ok(())
    }
    fn check_index(&self, index: usize, budget: &mut Budget) -> Check<usize> {
        // One checked page lookup here, then the caller's page and slot
        // lookups. Prepay all three before any lookup or mutation.
        budget.spend(3)?;
        if index >= self.len || self.pages.len() != self.len.div_ceil(WIDTH) {
            return Err(bad(None, "Sum SSA value identity absent"));
        }
        let page_index = index / WIDTH;
        let page = self
            .pages
            .get(page_index)
            .ok_or_else(|| bad(None, "Sum SSA page absent"))?;
        let base = page_index * WIDTH;
        if page.base != base || page.roots.len() != (self.len - base).min(WIDTH) {
            return Err(bad(None, "Sum SSA page shape differs"));
        }
        Ok(page_index)
    }
    pub(super) fn get(&self, index: usize, budget: &mut Budget) -> Check<&Arc<Content>> {
        let page = self.check_index(index, budget)?;
        self.pages[page]
            .roots
            .get(index % WIDTH)
            .ok_or_else(|| bad(None, "Sum SSA slot absent"))
    }
    fn edit_page<'a>(page: &'a mut Arc<Page>, budget: &mut Budget) -> Check<&'a mut Page> {
        if Arc::strong_count(page) > 1 {
            budget.spend(1)?;
            budget.spend(page.roots.len())?;
        }
        Ok(Arc::make_mut(page))
    }
    pub(super) fn set(
        &mut self,
        index: usize,
        root: Arc<Content>,
        budget: &mut Budget,
    ) -> Check<()> {
        let page = self.check_index(index, budget)?;
        // No publication until all actual page-COW allocation fees succeed.
        Self::edit_page(&mut self.pages[page], budget)?.roots[index % WIDTH] = root;
        Ok(())
    }
    pub(super) fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        budget.spend(1)?;
        if self.len != other.len {
            return Err(bad(None, "Sum SSA logical inventories differ"));
        }
        // BOTH complete inventories precede every identity shortcut.
        self.validate(budget)?;
        other.validate(budget)?;
        let mut changed = false;
        for (left, right) in self.pages.iter_mut().zip(&other.pages) {
            budget.spend(1)?;
            if Arc::ptr_eq(left, right) {
                continue;
            }
            // State::cost now pays for page references. All actual different-
            // page slot walks must therefore be charged here independently.
            budget.spend(left.roots.len())?;
            budget.spend(right.roots.len())?;
            let page = Self::edit_page(left, budget)?;
            for (a, b) in page.roots.iter_mut().zip(&right.roots) {
                // Keep this kernel's ORIGINAL per-Content alias/COW/merge.
                // Page sharing cannot replace its external alias test.
                if !Arc::ptr_eq(a, b) {
                    if Arc::strong_count(a) > 1 {
                        a.cost(budget)?;
                    }
                    changed |= Arc::make_mut(a).merge(b, budget)?;
                }
            }
        }
        Ok(changed)
    }
    #[cfg(test)]
    pub(super) fn flat(&self) -> Vec<Arc<Content>> {
        self.pages
            .iter()
            .flat_map(|page| page.roots.iter().cloned())
            .collect()
    }
    #[cfg(test)]
    pub(super) fn from_keyed(
        len: usize,
        values: BTreeMap<ValueId, Arc<Content>>,
        budget: &mut Budget,
    ) -> Check<Self> {
        budget.spend(1)?;
        if values.len() != len {
            return Err(bad(None, "Sum SSA keyed inventory differs"));
        }
        let mut iter = values.into_iter();
        Self::build(len, budget, |index, budget| {
            budget.spend(1)?;
            let (key, root) = iter
                .next()
                .ok_or_else(|| bad(None, "Sum SSA keyed slot absent"))?;
            if key.0 as usize != index {
                return Err(bad(None, "Sum SSA keyed identity differs"));
            }
            Ok(root)
        })
    }
    #[cfg(test)]
    pub(super) fn corrupt_shape(&mut self, kind: usize) {
        match kind {
            0 => self.pages.push(Arc::new(Page {
                base: self.len,
                roots: Vec::new(),
            })),
            1 => Arc::make_mut(&mut self.pages[0]).base += 1,
            _ => {
                Arc::make_mut(&mut self.pages[0]).roots.pop();
            }
        }
    }
}
