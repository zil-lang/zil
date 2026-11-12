//! The resource entry gate's exact raw-type predicate, with ordered fee replay.
//! This is not a resource safety receipt: an owning result still requires the
//! original complete resource/effect/cleanup checks. Current closure captures
//! and the global gate are evaluated separately, at their original positions.
use super::*;

pub(super) struct Node {
    len: usize,
    work: usize,
    owns: bool,
    children: Option<(Arc<Node>, Arc<Node>)>,
}
impl Attempt<'_> {
    pub(crate) fn check_resource_ownership(
        &mut self,
        module: &TypedModule,
        budget: &mut Budget,
    ) -> Check<bool> {
        let previous = self.previous_resource_inventory.cloned();
        let root = self.check_resource_range(
            module,
            0,
            module.functions.len(),
            previous.as_ref(),
            budget,
        )?;
        let owns = root.owns;
        self.next_resource_inventory = Some(root);
        Ok(owns)
    }
    fn check_resource_range(
        &mut self,
        module: &TypedModule,
        start: usize,
        len: usize,
        previous: Option<&Arc<Node>>,
        budget: &mut Budget,
    ) -> Check<Arc<Node>> {
        let previous = previous.filter(|p| p.len == len);
        if let Some(previous) = previous
            && previous.work <= budget.remaining
            && self
                .delta
                .as_ref()
                .is_some_and(|delta| delta.unchanged_range(module, start, len))
        {
            budget.spend(previous.work)?;
            self.stats.resource_inventory_functions_reused += len;
            self.stats.resource_inventory_ranges_reused += 1;
            return Ok(previous.clone());
        }
        let remaining = budget.remaining;
        // No function error can be hidden by an aggregate rejection. With low
        // credit, descend to the original per-function upfront fee and order.
        let (owns, children) = if len == 1 {
            self.stats.resource_inventory_functions_checked += 1;
            (
                resources::raw_function_owns(&module.functions[start], budget)?,
                None,
            )
        } else {
            let split = if len.is_power_of_two() {
                len / 2
            } else {
                1usize << len.ilog2()
            };
            let old_children = previous.and_then(|p| p.children.as_ref());
            let left = self.check_resource_range(
                module,
                start,
                split,
                old_children.map(|p| &p.0),
                budget,
            )?;
            let right = self.check_resource_range(
                module,
                start + split,
                len - split,
                old_children.map(|p| &p.1),
                budget,
            )?;
            (left.owns || right.owns, Some((left, right)))
        };
        Ok(Arc::new(Node {
            len,
            work: remaining - budget.remaining,
            owns,
            children,
        }))
    }
}
