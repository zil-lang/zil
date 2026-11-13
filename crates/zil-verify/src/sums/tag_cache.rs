//! Per-Flow local indexing of static qualification and positive shape proofs.
use super::*;

struct Entry {
    // Only the complete original addressed_copy_sum can produce this bool.
    copy: bool,
    unknown: Option<Arc<Content>>,
}
pub(super) struct Cache<'f> {
    function: &'f TypedFunction,
    // No allocation until a complete qualification/proof is recorded.
    slots: Option<Vec<Option<Entry>>>,
    // Static layouts are indexed only by this borrowed function's own locals.
    layouts: Option<Vec<Option<u8>>>,
}
impl<'f> Cache<'f> {
    pub(super) fn new(function: &'f TypedFunction) -> Self {
        Self {
            function,
            slots: None,
            layouts: None,
        }
    }
    pub(super) fn layout(&mut self, local: usize, actual: &Type, budget: &mut Budget) -> Check<u8> {
        budget.spend(2)?; // exact immutable local and Type identity
        let Some(declaration) = self.function.locals.get(local) else {
            return tag_layout(actual, budget);
        };
        if !std::ptr::eq(actual, &declaration.ty) {
            return tag_layout(actual, budget);
        }
        budget.spend(1)?; // optional complete local-index inventory
        if let Some(layouts) = &self.layouts {
            budget.spend(1)?;
            if let Some(layout) = layouts[local] {
                return Ok(layout);
            }
        }
        // No success is published until the original complete independent
        // qualification (including all payload contexts) has succeeded.
        let layout = tag_layout(actual, budget)?;
        if self.layouts.is_none() {
            budget.spend(
                self.function
                    .locals
                    .len()
                    .saturating_mul(2)
                    .saturating_add(2),
            )?;
        }
        budget.spend(2)?; // actual index and publication
        self.layouts
            .get_or_insert_with(|| vec![None; self.function.locals.len()])[local] = Some(layout);
        Ok(layout)
    }
    pub(super) fn known(
        &mut self,
        local: usize,
        value: &Arc<Content>,
        actual: &Type,
        budget: &mut Budget,
    ) -> Check<bool> {
        budget.spend(2)?; // checked actual local lookup and Type identity
        let Some(declaration) = self.function.locals.get(local) else {
            return qualified_unknown(value, actual, budget);
        };
        if !std::ptr::eq(actual, &declaration.ty) {
            return qualified_unknown(value, actual, budget);
        }
        budget.spend(1)?; // optional slot inventory
        let previous = if let Some(slots) = &self.slots {
            budget.spend(2)?; // actual direct slot lookup and entry presence
            slots[local].as_ref()
        } else {
            None
        };
        let had_entry = previous.is_some();
        let copy = if let Some(entry) = previous {
            budget.spend(1)?; // read this actual local's static qualification
            entry.copy
        } else {
            // Same complete cold admission, metadata and errors as before.
            // The result is scoped to this Flow's borrowed immutable local Type.
            budget.addressed_copy_sum(actual)?
        };
        if !copy {
            if !had_entry {
                self.publish(local, false, None, budget)?;
            }
            return Ok(false);
        }
        if let Some(entry) = previous {
            budget.spend(1)?; // positive root presence
            if let Some(old) = &entry.unknown {
                budget.spend(1)?; // actual immutable root identity
                if Arc::ptr_eq(old, value) {
                    return Ok(true);
                }
            }
        }
        let unknown = value.tags_unknown(actual, budget)?;
        if unknown {
            self.publish(local, true, Some(value), budget)?;
        } else if !had_entry {
            // A static Copy fact is not a dynamic tag/route proof.
            self.publish(local, true, None, budget)?;
        }
        Ok(unknown)
    }
    fn publish(
        &mut self,
        local: usize,
        copy: bool,
        unknown: Option<&Arc<Content>>,
        budget: &mut Budget,
    ) -> Check<()> {
        if self.slots.is_none() {
            // Pay actual allocation width, initialized slots and the final
            // slot destruction walk. No table is published before all fees.
            let words = std::mem::size_of::<Option<Entry>>().div_ceil(std::mem::size_of::<usize>());
            budget.spend(
                self.function
                    .locals
                    .len()
                    .saturating_mul(words.saturating_add(1))
                    .saturating_add(2),
            )?;
        }
        if let Some(value) = unknown {
            // Strong identities can own the final deep Content destruction.
            // Reserve it now; replacing or dropping the cache spends no hidden
            // recursive work, and ordinary writes retain their real COW fees.
            value.cost(budget)?;
            budget.spend(2)?; // Arc clone and previous strong-reference release
        }
        budget.spend(4)?; // table presence, slot lookup, bool/root field writes
        let entry = Entry {
            copy,
            unknown: unknown.cloned(),
        };
        let slots = self.slots.get_or_insert_with(|| {
            std::iter::repeat_with(|| None)
                .take(self.function.locals.len())
                .collect()
        });
        slots[local] = Some(entry);
        Ok(())
    }
}
fn qualified_unknown(value: &Content, actual: &Type, budget: &mut Budget) -> Check<bool> {
    let copy = if indexed_invalidation(budget) {
        budget.addressed_copy_sum(actual)?
    } else {
        resources::charge_payload(actual, budget)?;
        actual.is_copy_value()
    };
    Ok(copy && value.tags_unknown(actual, budget)?)
}
#[cfg(test)]
mod qualification_reference;
#[cfg(test)]
mod tests;
