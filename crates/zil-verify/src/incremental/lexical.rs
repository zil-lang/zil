//! Complete lexical-region and lexical-cleanup obligations, in original order.
//!
//! Unlike ordinary diagnostic-span-independent flow proofs, region admission
//! reads lexical spans. Bind every raw field, including spans. The caller has
//! just independently established the current CFG: successors and reachability
//! are deterministic from this same complete function and candidate, with no
//! callee/module-table inputs. Capture views are not read by either stage.
//! Region checks only spend work; lexical cleanup additionally reads `sums`.
//! Deferred-action/callee checks remain fresh after these two stages.
use super::*;

pub(super) struct Receipt {
    region_work: usize,
    cleanup_work: usize,
    sums: bool,
    deferred: bool,
}
impl Attempt<'_> {
    pub(crate) fn discard_lexical(&mut self, function: FunctionId) {
        let index = function.0 as usize;
        if self.next_lexical.get(index).is_some_and(Option::is_some)
            && let Some(row) = self.next_lexical.get_mut(index)
        {
            *row = None;
        }
    }

    pub(crate) fn check_lexical(
        &mut self,
        function: &TypedFunction,
        plan: &FunctionPlan,
        target: &TargetProfile,
        graph: &FunctionGraph,
        budget: &mut Budget,
        limits: &Limits,
    ) -> Check<bool> {
        let index = function.id.0 as usize;
        let bound = self.previous_inputs.is_some_and(|previous| {
            previous.limits() == limits
                && previous.module().target == *target
                && previous
                    .module()
                    .functions
                    .get(index)
                    .is_some_and(|old| std::ptr::eq(old, function) || old == function)
                && previous.plan().functions.get(index) == Some(plan)
        });
        let previous = bound
            .then(|| self.previous_lexical.get(index).and_then(Option::as_ref))
            .flatten();
        let before_region = budget.remaining;
        if let Some(proof) = previous
            && proof.region_work <= budget.remaining
        {
            budget.spend(proof.region_work)?;
            self.stats.region_functions_reused += 1;
        } else {
            self.stats.region_functions_checked += 1;
            foreign::check_regions(function, graph, budget)?;
        }
        let region_work = before_region - budget.remaining;
        let before_cleanup = budget.remaining;
        let sums = budget.sums;
        let deferred = if let Some(proof) = previous
            && proof.sums == sums
            && proof.cleanup_work <= budget.remaining
        {
            budget.spend(proof.cleanup_work)?;
            self.stats.lexical_cleanup_functions_reused += 1;
            proof.deferred
        } else {
            self.stats.lexical_cleanup_functions_checked += 1;
            cleanup::check_lexical(function, graph, budget)?
        };
        if let Some(previous) = previous
            && previous.sums == sums
        {
            // Both successful stages have their original deterministic cost;
            // keep the existing immutable row without a persistent tree write.
            if !self
                .next_lexical
                .get(index)
                .and_then(Option::as_ref)
                .is_some_and(|row| Arc::ptr_eq(row, previous))
                && let Some(row) = self.next_lexical.get_mut(index)
            {
                *row = Some(previous.clone());
            }
        } else if let Some(row) = self.next_lexical.get_mut(index) {
            *row = Some(Arc::new(Receipt {
                region_work,
                cleanup_work: before_cleanup - budget.remaining,
                sums,
                deferred,
            }));
        }
        Ok(deferred)
    }
}
