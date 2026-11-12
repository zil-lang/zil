//! Ordered raw-shape obligation tree. An unchanged raw subtree may reuse its
//! checker-produced aggregate only with the same entry state and enough work.
//! The aggregate domain is defined by actual effects: no module-dependent life
//! header, pointer inventory or per-function sum marks. Other domains descend
//! to the original per-function stage, including all rich type checks.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Context {
    flags: shapes::Flags,
    function_count: usize,
}
impl Context {
    fn read(b: &Budget) -> Option<Self> {
        if b.pod_stores
            || b.pod_carriers.is_some()
            || !b.sum_functions.is_empty()
            || !b.array_snapshot_roots.is_empty()
        {
            return None;
        }
        Some(Self {
            flags: shapes::Flags::read(b),
            function_count: b.function_count,
        })
    }
    fn apply(self, b: &mut Budget) {
        self.flags.apply(b);
    }
}
struct Receipt {
    before: Context,
    after: Context,
    work: usize,
}
pub(super) struct Node {
    len: usize,
    source_free: bool,
    receipt: Option<Receipt>,
    children: Option<(Arc<Node>, Arc<Node>)>,
}

impl Attempt<'_> {
    pub(crate) fn check_shape_tree(
        &mut self,
        module: &TypedModule,
        b: &mut Budget,
        limits: &Limits,
    ) -> Check<()> {
        self.next_shapes = resized_rows(self.previous_shapes, module.functions.len());
        self.next_inventory = resized_rows(self.previous_inventory, module.functions.len());
        let previous = self.previous_shape_tree.cloned();
        let root = self.check_shape_range(
            module,
            0,
            module.functions.len(),
            previous.as_ref(),
            b,
            limits,
        )?;
        self.next_shape_tree = Some(root);
        Ok(())
    }
    fn check_shape_range(
        &mut self,
        module: &TypedModule,
        start: usize,
        len: usize,
        previous: Option<&Arc<Node>>,
        b: &mut Budget,
        limits: &Limits,
    ) -> Check<Arc<Node>> {
        let previous = previous.filter(|p| p.len == len);
        let before = Context::read(b);
        if let Some(previous) = previous
            && let Some(proof) = &previous.receipt
            && Some(proof.before) == before
            && proof.work <= b.remaining
            && self
                .delta
                .as_ref()
                .is_some_and(|delta| delta.unchanged_range(module, start, len))
        {
            b.spend(proof.work)?;
            proof.after.apply(b);
            b.current_function = Some(start + len - 1);
            self.stats.raw_shape_functions_aggregated += len;
            self.stats.raw_shape_ranges_reused += 1;
            return Ok(previous.clone());
        }
        // Insufficient total credit is never a new rejection point. Descending
        // preserves the earliest original error and its residual budget.
        let remaining = b.remaining;
        let (source_free, children) = if len == 1 {
            let function = &module.functions[start];
            check_function_raw_shapes(module, start, function, b, limits, Some(self))?;
            (function.storage_life.is_none(), None)
        } else {
            let split = if len.is_power_of_two() {
                len / 2
            } else {
                1usize << len.ilog2()
            };
            let old_children = previous.and_then(|p| p.children.as_ref());
            let left = self.check_shape_range(
                module,
                start,
                split,
                old_children.map(|p| &p.0),
                b,
                limits,
            )?;
            let right = self.check_shape_range(
                module,
                start + split,
                len - split,
                old_children.map(|p| &p.1),
                b,
                limits,
            )?;
            (left.source_free && right.source_free, Some((left, right)))
        };
        let receipt = if source_free {
            before.zip(Context::read(b)).map(|(before, after)| Receipt {
                before,
                after,
                work: remaining - b.remaining,
            })
        } else {
            None
        };
        Ok(Arc::new(Node {
            len,
            source_free,
            receipt,
            children,
        }))
    }
}
