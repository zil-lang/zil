//! Frozen c23d full shared transfer AND positive-cache implementation.
//! Per-Flow positive shape proofs, never Copy or resource authorization.
use super::*;

pub(super) struct OldCache<'f> {
    function: &'f TypedFunction,
    // No allocation until an actual successful, qualified proof is recorded.
    slots: Option<Vec<Option<Arc<Content>>>>,
}
impl<'f> OldCache<'f> {
    pub(super) fn new(function: &'f TypedFunction) -> Self {
        Self {
            function,
            slots: None,
        }
    }
    pub(super) fn known(
        &mut self,
        local: usize,
        value: &Arc<Content>,
        actual: &Type,
        budget: &mut Budget,
    ) -> Check<bool> {
        // Bind even internal callers to this borrowed function's actual local
        // Type. Equal temporary Types and other locals cannot reuse its proof.
        budget.spend(2)?; // checked local lookup and actual-Type identity
        let Some(declaration) = self.function.locals.get(local) else {
            return value.tags_unknown(actual, budget);
        };
        if !std::ptr::eq(actual, &declaration.ty) {
            return value.tags_unknown(actual, budget);
        }
        budget.spend(1)?; // optional slot inventory
        if let Some(slots) = &self.slots {
            budget.spend(3)?; // direct slot lookup, presence and root identity
            if slots[local]
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, value))
            {
                return Ok(true);
            }
        }
        if !value.tags_unknown(actual, budget)? {
            return Ok(false);
        }
        if self.slots.is_none() {
            // Pay both initialization and final destruction's slot walk before
            // allocation. The field cannot publish a partly reserved table.
            budget.spend(
                self.function
                    .locals
                    .len()
                    .saturating_mul(2)
                    .saturating_add(2),
            )?;
        }
        // Holding the last reference can delay recursive Content destruction.
        // Reserve that complete future traversal now, while the proof succeeds;
        // replacement or Flow drop never hides an uncharged deep destructor.
        value.cost(budget)?;
        budget.spend(5)?; // table presence, slot lookup, Arc clone, write and old ref release
        let slots = self
            .slots
            .get_or_insert_with(|| vec![None; self.function.locals.len()]);
        slots[local] = Some(value.clone());
        Ok(true)
    }
}

pub(super) fn shared(
    value: &mut Arc<Content>,
    ty: &Type,
    budget: &mut Budget,
    cached: Option<(&std::cell::RefCell<OldCache<'_>>, usize)>,
) -> Check<()> {
    budget.spend(1)?;
    if Arc::strong_count(value) > 1
        && matches!(value.as_ref(), Content::Sum(..))
        && matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
    {
        // Keep the original entry and qualification order. In particular,
        // charge_payload rejects invalid Task contexts before any shape proof.
        budget.spend(1)?;
        let actual = shared_enum_physical(ty, budget)?;
        let copy_sum = if indexed_invalidation(budget) {
            budget.addressed_copy_sum(actual)?
        } else {
            resources::charge_payload(actual, budget)?;
            actual.is_copy_value()
        };
        if copy_sum {
            let unknown = if let Some((cache, local)) = cached {
                budget.spend(1)?; // actual RefCell borrow before its slot work
                cache.borrow_mut().known(local, value, actual, budget)?
            } else {
                value.tags_unknown(actual, budget)?
            };
            if unknown {
                return Ok(());
            }
        }
    }
    value.cost(budget)?;
    Arc::make_mut(value).forget_copy_tags(ty, budget)
}

impl OldCache<'_> {
    pub(super) fn logical(&self) -> Vec<Option<Content>> {
        self.slots.as_ref().map_or_else(
            || vec![None; self.function.locals.len()],
            |slots| {
                slots
                    .iter()
                    .map(|slot| slot.as_ref().map(|root| root.as_ref().clone()))
                    .collect()
            },
        )
    }
}
