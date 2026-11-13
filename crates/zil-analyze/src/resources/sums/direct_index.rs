//! Same-function inventories indexed by their actual canonical IR positions.
use super::*;

#[derive(Clone, Debug, Default)]
pub(super) struct Locals(Vec<Arc<Tree>>, Option<PendingTags>);
// Scheduling is private. All logical state, including every moved marker,
// origin, alternative and child, remains in the complete Tree inventory.
impl PartialEq for Locals {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for Locals {}
#[derive(Clone, Debug)]
struct PendingTags {
    eligible: Arc<Vec<u64>>,
    pending: Vec<u64>,
}

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
        Ok(Self(slots, None))
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
    pub(super) fn get_mut(
        &mut self,
        id: &LocalId,
        budget: &mut Budget,
    ) -> CompileResult<Option<&mut Arc<Tree>>> {
        self.touch(*id, budget)?;
        Ok(self.0.get_mut(id.0 as usize))
    }
    fn touch(&mut self, id: LocalId, budget: &mut Budget) -> CompileResult<()> {
        if let Some(tags) = &mut self.1 {
            budget.spend(3, "Sum actual mutable local pending tag selection")?;
            let index = id.0 as usize;
            if let Some(word) = tags.pending.get_mut(index / 64) {
                *word |= tags.eligible[index / 64] & (1u64 << (index % 64));
            }
        }
        Ok(())
    }
    pub(super) fn enable_pending_tags(
        &mut self,
        eligible: &[LocalId],
        budget: &mut Budget,
    ) -> CompileResult<()> {
        let words = self.len().div_ceil(64);
        budget.spend(
            words.saturating_mul(4).saturating_add(3),
            "Sum pending tag bit inventories allocation initialization and release",
        )?;
        let mut mask = vec![0u64; words];
        for id in eligible {
            budget.spend(3, "Sum pending tag actual local inventory")?;
            let index = id.0 as usize;
            if index >= self.len() {
                return Err(error("pending tag local absent"));
            }
            mask[index / 64] |= 1u64 << (index % 64);
        }
        self.1 = Some(PendingTags {
            pending: mask.clone(),
            eligible: Arc::new(mask),
        });
        Ok(())
    }
    pub(super) fn take_pending_tags(
        &mut self,
        budget: &mut Budget,
    ) -> CompileResult<Option<Vec<LocalId>>> {
        let Some(tags) = &mut self.1 else {
            return Ok(None);
        };
        budget.spend(
            tags.pending.len().saturating_add(1),
            "Sum pending tag word inventory",
        )?;
        let count = tags
            .pending
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum::<usize>();
        budget.spend(
            tags.pending
                .len()
                .saturating_add(count.saturating_mul(3))
                .saturating_add(1),
            "Sum pending tag actual indices allocation extraction and release",
        )?;
        let mut selected = Vec::with_capacity(count);
        for (index, word) in tags.pending.iter_mut().enumerate() {
            let mut bits = *word;
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                selected.push(LocalId((index * 64 + bit) as u32));
                bits &= bits - 1;
            }
            *word = 0;
        }
        Ok(Some(selected))
    }
    pub(super) fn snapshot_cost(&self) -> usize {
        self.len().saturating_add(
            self.1
                .as_ref()
                .map_or(0, |tags| tags.pending.len().saturating_add(3)),
        )
    }
    // Only the complete tag transfer may bypass mutation registration, after
    // take_pending_tags has selected this actual root. Every other write must
    // use get_mut/set, including edge refinements and joined contents.
    pub(super) fn tag_transfer_mut(
        &mut self,
        id: LocalId,
        budget: &mut Budget,
    ) -> CompileResult<&mut Arc<Tree>> {
        budget.spend(1, "actual direct local slot selection")?;
        self.0
            .get_mut(id.0 as usize)
            .ok_or_else(|| error("mixed tagged local fact is absent"))
    }
    pub(super) fn contains_key(&self, id: &LocalId) -> bool {
        self.get(id).is_some()
    }
    pub(super) fn set(
        &mut self,
        id: LocalId,
        tree: Arc<Tree>,
        budget: &mut Budget,
    ) -> CompileResult<()> {
        *self
            .get_mut(&id, budget)?
            .ok_or_else(|| error("Sum local absent"))? = tree;
        Ok(())
    }
    pub(super) fn iter_mut<'a>(
        &'a mut self,
        budget: &mut Budget,
    ) -> CompileResult<impl Iterator<Item = (LocalId, &'a mut Arc<Tree>)> + use<'a>> {
        if let Some(tags) = &mut self.1 {
            budget.spend(
                tags.pending.len().saturating_mul(2),
                "Sum mutable local inventory pending tags",
            )?;
            for (pending, eligible) in tags.pending.iter_mut().zip(tags.eligible.iter()) {
                *pending |= eligible;
            }
        }
        Ok(self
            .0
            .iter_mut()
            .enumerate()
            .map(|(index, tree)| (LocalId(index as u32), tree)))
    }
    pub(super) fn join(&mut self, other: &Self, budget: &mut Budget) -> CompileResult<bool> {
        budget.spend(1, "Sum direct local inventory comparison")?;
        if self.len() != other.len() {
            return Err(error("Sum joined state inventory differs"));
        }
        budget.spend(self.len(), "Sum direct local target walk")?;
        budget.spend(other.len(), "Sum direct local incoming walk")?;
        let mut changed = false;
        for index in 0..self.len() {
            let changed_here = self.0[index].merge(&other.0[index], budget)?;
            if changed_here {
                self.touch(LocalId(index as u32), budget)?;
            }
            changed |= changed_here;
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
        Ok(Self(slots, None))
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
