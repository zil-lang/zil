//! Closure-instance layout obligations. The complete expanded layout table is
//! freshly reconstructed by this checker before this stage. Its exact values
//! and cardinality bind every ClosureId lookup and its expansion cost; no
//! producer layout, cache hash or signature summary is authoritative here.
//! Raw type projections and all target/limit inputs bind the other reads.
//! Reverse function/type order is retained for exhausted budgets and failures.
use super::*;

struct Node {
    len: usize,
    work: usize,
    children: Option<(Arc<Node>, Arc<Node>)>,
}
pub(super) struct Receipt {
    expanded: Arc<Vec<usize>>,
    root: Arc<Node>,
}
impl Attempt<'_> {
    pub(crate) fn check_closure_instances(
        &mut self,
        module: &TypedModule,
        expanded: &[usize],
        budget: &mut Budget,
    ) -> Check<()> {
        let previous = self
            .previous_closure_instances
            .filter(|proof| proof.expanded.as_slice() == expanded)
            .cloned();
        let root = self.check_instance_range(
            module,
            expanded,
            0,
            module.functions.len(),
            previous.as_ref().map(|proof| &proof.root),
            budget,
        )?;
        let expanded = previous.map_or_else(
            || Arc::new(expanded.to_vec()),
            |proof| proof.expanded.clone(),
        );
        self.next_closure_instances = Some(Arc::new(Receipt { expanded, root }));
        Ok(())
    }
    fn check_instance_range(
        &mut self,
        module: &TypedModule,
        expanded: &[usize],
        start: usize,
        len: usize,
        previous: Option<&Arc<Node>>,
        budget: &mut Budget,
    ) -> Check<Arc<Node>> {
        let previous = previous.filter(|p| p.len == len);
        if let Some(previous) = previous
            && previous.work <= budget.remaining
            && self.delta.as_ref().is_some_and(|delta| {
                delta.unchanged_range(module, start, len)
                    || len == 1
                        && delta.functions.ptr_eq(&module.functions)
                        && delta
                            .previous_functions
                            .get(start)
                            .is_some_and(|old| shapes::same_types(old, &module.functions[start]))
            })
        {
            budget.spend(previous.work)?;
            self.stats.closure_instance_functions_reused += len;
            self.stats.closure_instance_ranges_reused += 1;
            return Ok(previous.clone());
        }
        let remaining = budget.remaining;
        let children = if len == 1 {
            self.stats.closure_instance_functions_checked += 1;
            closures::check_function_instances(module, &module.functions[start], expanded, budget)?;
            None
        } else {
            let split = if len.is_power_of_two() {
                len / 2
            } else {
                1usize << len.ilog2()
            };
            let old_children = previous.and_then(|p| p.children.as_ref());
            // The old module-wide type stack visited the last function first.
            let right = self.check_instance_range(
                module,
                expanded,
                start + split,
                len - split,
                old_children.map(|p| &p.1),
                budget,
            )?;
            let left = self.check_instance_range(
                module,
                expanded,
                start,
                split,
                old_children.map(|p| &p.0),
                budget,
            )?;
            Some((left, right))
        };
        Ok(Arc::new(Node {
            len,
            work: remaining - budget.remaining,
            children,
        }))
    }
}
