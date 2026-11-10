//! Same-function inventories indexed by their actual canonical IR positions.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Locals(Vec<Arc<Tree>>);
impl Locals {
    pub(super) fn build(
        f: &TypedFunction,
        budget: &mut Budget,
        mut tree: impl FnMut(&LocalDecl, &mut Budget) -> CompileResult<Arc<Tree>>,
    ) -> CompileResult<Self> {
        budget.spend(
            f.locals.len().saturating_add(1),
            "Sum direct local allocation",
        )?;
        let mut slots = Vec::with_capacity(f.locals.len());
        for (index, local) in f.locals.iter().enumerate() {
            budget.spend(1, "Sum direct local identity")?;
            if local.id.0 as usize != index {
                return Err(error("Sum local inventory is not canonical"));
            }
            slots.push(tree(local, budget)?);
        }
        Ok(Self(slots))
    }
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(super) fn get(&self, id: &LocalId) -> Option<&Arc<Tree>> {
        self.0.get(id.0 as usize)
    }
    pub(super) fn get_mut(&mut self, id: &LocalId) -> Option<&mut Arc<Tree>> {
        self.0.get_mut(id.0 as usize)
    }
    pub(super) fn contains_key(&self, id: &LocalId) -> bool {
        self.get(id).is_some()
    }
    pub(super) fn set(&mut self, id: LocalId, tree: Arc<Tree>) -> CompileResult<()> {
        *self.get_mut(&id).ok_or_else(|| error("Sum local absent"))? = tree;
        Ok(())
    }
    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = (LocalId, &mut Arc<Tree>)> {
        self.0
            .iter_mut()
            .enumerate()
            .map(|(index, tree)| (LocalId(index as u32), tree))
    }
    pub(super) fn join(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(1, "Sum direct local inventory comparison")?;
        if self.len() != other.len() {
            return Err(error("Sum joined state inventory differs"));
        }
        budget.spend(self.len(), "Sum direct local target walk")?;
        budget.spend(other.len(), "Sum direct local incoming walk")?;
        let mut changed = false;
        for (left, right) in self.0.iter_mut().zip(&other.0) {
            changed |= left.merge(right, budget)?;
        }
        Ok(changed)
    }
    #[cfg(test)]
    pub(super) fn keyed(&self) -> BTreeMap<LocalId, Arc<Tree>> {
        self.0
            .iter()
            .enumerate()
            .map(|(index, tree)| (LocalId(index as u32), tree.clone()))
            .collect()
    }
    #[cfg(test)]
    pub(super) fn from_keyed(
        len: usize,
        keyed: BTreeMap<LocalId, Arc<Tree>>,
    ) -> CompileResult<Self> {
        if len != keyed.len() {
            return Err(error("Sum local inventory differs"));
        }
        let mut slots = Vec::with_capacity(len);
        for (index, (key, tree)) in keyed.into_iter().enumerate() {
            if key.0 as usize != index {
                return Err(error("Sum local inventory key differs"));
            }
            slots.push(tree);
        }
        Ok(Self(slots))
    }
}
impl std::ops::Index<&LocalId> for Locals {
    type Output = Arc<Tree>;
    fn index(&self, id: &LocalId) -> &Self::Output {
        &self.0[id.0 as usize]
    }
}

type Observation = (LocalId, Vec<VariantProjection>);
pub(super) struct Observations {
    slots: Vec<Option<Observation>>,
    occupied: usize,
}
impl Observations {
    pub(super) fn new(len: usize, budget: &mut Budget) -> CompileResult<Self> {
        budget.spend(len.saturating_add(1), "Sum direct observation allocation")?;
        Ok(Self {
            slots: vec![None; len],
            occupied: 0,
        })
    }
    pub(super) fn len(&self) -> usize {
        self.occupied
    }
    pub(super) fn get(&self, id: &ValueId) -> Option<&Observation> {
        self.slots.get(id.0 as usize)?.as_ref()
    }
    pub(super) fn contains_key(&self, id: &ValueId) -> bool {
        self.get(id).is_some()
    }
    pub(super) fn insert(&mut self, id: ValueId, observation: Observation) -> CompileResult<()> {
        let slot = self
            .slots
            .get_mut(id.0 as usize)
            .ok_or_else(|| error("Sum observation identity absent"))?;
        if slot.is_none() {
            self.occupied += 1;
        }
        *slot = Some(observation);
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn keyed(&self) -> BTreeMap<ValueId, Observation> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| slot.clone().map(|slot| (ValueId(i as u32), slot)))
            .collect()
    }
}
impl std::ops::Index<&ValueId> for Observations {
    type Output = Observation;
    fn index(&self, id: &ValueId) -> &Self::Output {
        self.get(id).expect("Sum observation absent")
    }
}
