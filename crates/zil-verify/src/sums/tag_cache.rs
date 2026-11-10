//! Per-Flow positive shape proofs, never Copy or resource authorization.
use super::*;

pub(super) struct Cache<'f> {
    function: &'f TypedFunction,
    // No allocation until an actual successful, qualified proof is recorded.
    slots: Option<Vec<Option<Arc<Content>>>>,
}
impl<'f> Cache<'f> {
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
#[cfg(test)]
mod tests;
