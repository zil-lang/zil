//! Complete, canonical SSA inventory. Pages share only immutable Tree refs;
//! neither their identity nor an unchanged page is an ownership permission.
use super::*;

const WIDTH: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Page {
    base: usize,
    trees: Vec<Arc<Tree>>,
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
        mut tree: impl FnMut(usize, &mut Budget) -> CompileResult<Arc<Tree>>,
    ) -> CompileResult<Self> {
        let count = len.div_ceil(WIDTH);
        // Prepay every actual page reference and every Tree reference before
        // allocating; unknown trees retain their own original allocation cost.
        budget.spend(count, "Sum SSA page inventory allocation")?;
        budget.spend(len, "Sum SSA slot inventory allocation")?;
        let mut pages = Vec::with_capacity(count);
        for page in 0..count {
            let base = page * WIDTH;
            let n = (len - base).min(WIDTH);
            let mut trees = Vec::with_capacity(n);
            for offset in 0..n {
                trees.push(tree(base + offset, budget)?);
            }
            pages.push(Arc::new(Page { base, trees }));
        }
        Ok(Self { len, pages })
    }
    pub(super) fn len(&self) -> usize {
        self.len
    }
    pub(super) fn page_count(&self) -> usize {
        self.pages.len()
    }
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }
    fn validate(&self, budget: &mut Budget) -> CompileResult<()> {
        budget.spend(1, "Sum SSA page inventory size comparison")?;
        if self.pages.len() != self.len.div_ceil(WIDTH) {
            return Err(error("Sum SSA page inventory differs"));
        }
        budget.product(self.pages.len(), 3, 1, "Sum SSA page shape validation")?;
        for (index, page) in self.pages.iter().enumerate() {
            let base = index * WIDTH;
            if page.base != base || page.trees.len() != (self.len - base).min(WIDTH) {
                return Err(error("Sum SSA page shape differs"));
            }
        }
        Ok(())
    }
    pub(super) fn get(&self, id: &ValueId) -> Option<&Arc<Tree>> {
        let index = id.0 as usize;
        if index >= self.len {
            return None;
        }
        let page = self.pages.get(index / WIDTH)?;
        if page.base != index / WIDTH * WIDTH
            || page.trees.len() != (self.len - page.base).min(WIDTH)
        {
            return None;
        }
        page.trees.get(index % WIDTH)
    }
    pub(super) fn get_mut(
        &mut self,
        id: &ValueId,
        budget: &mut Budget,
    ) -> CompileResult<Option<&mut Arc<Tree>>> {
        budget.spend(1, "Sum SSA checked mutable index")?;
        let index = id.0 as usize;
        if index >= self.len {
            return Ok(None);
        }
        let page = self
            .pages
            .get_mut(index / WIDTH)
            .ok_or_else(|| error("Sum SSA page absent"))?;
        if page.base != index / WIDTH * WIDTH
            || page.trees.len() != (self.len - page.base).min(WIDTH)
        {
            return Err(error("Sum SSA page shape differs"));
        }
        Ok(Self::edit_page(page, budget)?.trees.get_mut(index % WIDTH))
    }
    fn edit_page<'a>(page: &'a mut Arc<Page>, budget: &mut Budget) -> CompileResult<&'a mut Page> {
        if Arc::strong_count(page) > 1 {
            budget.spend(1, "Sum SSA COW page header")?;
            budget.spend(page.trees.len(), "Sum SSA COW page Tree refs")?;
        }
        Ok(Arc::make_mut(page))
    }
    pub(super) fn set(
        &mut self,
        id: ValueId,
        tree: Arc<Tree>,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        *self
            .get_mut(&id, budget)?
            .ok_or_else(|| error("Sum SSA value absent"))? = tree;
        Ok(())
    }
    pub(super) fn join(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(1, "Sum SSA logical inventory comparison")?;
        if self.len != other.len {
            return Err(error("Sum joined state inventory differs"));
        }
        // Validate ALL pages on both sides before even the identity fast path.
        self.validate(budget)?;
        other.validate(budget)?;
        let mut changed = false;
        for (left, right) in self.pages.iter_mut().zip(&other.pages) {
            budget.spend(1, "Sum SSA shared page identity")?;
            if Arc::ptr_eq(left, right) {
                continue;
            }
            let page = Self::edit_page(left, budget)?;
            budget.product(page.trees.len(), 2, 1, "Sum SSA page slot walk")?;
            for (target, incoming) in page.trees.iter_mut().zip(&right.trees) {
                changed |= target.merge(incoming, budget)?;
            }
        }
        Ok(changed)
    }
    #[cfg(test)]
    pub(super) fn from_keyed(
        len: usize,
        keyed: BTreeMap<ValueId, Arc<Tree>>,
        budget: &mut Budget,
    ) -> CompileResult<Self> {
        budget.spend(1, "Sum SSA keyed inventory length")?;
        if keyed.len() != len {
            return Err(error("Sum SSA keyed inventory differs"));
        }
        let mut keyed = keyed.into_iter();
        Self::build(len, budget, |index, budget| {
            budget.spend(1, "Sum SSA keyed inventory identity")?;
            let (id, tree) = keyed
                .next()
                .ok_or_else(|| error("Sum SSA keyed value absent"))?;
            if id.0 as usize != index {
                return Err(error("Sum SSA keyed inventory key differs"));
            }
            Ok(tree)
        })
    }
    #[cfg(test)]
    pub(super) fn keyed(&self) -> BTreeMap<ValueId, Arc<Tree>> {
        self.pages
            .iter()
            .flat_map(|page| {
                page.trees
                    .iter()
                    .enumerate()
                    .map(|(offset, tree)| (ValueId((page.base + offset) as u32), tree.clone()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
