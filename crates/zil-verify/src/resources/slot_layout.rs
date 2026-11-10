//! Complete immutable slot layouts, owned by one resource-check request.
use super::*;

fn words<T>() -> usize {
    std::mem::size_of::<T>().div_ceil(std::mem::size_of::<usize>())
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Mode {
    sums: bool,
    sum_function: bool,
    head_descriptions: bool,
    inline_store_addresses: bool,
    pod_stores: bool,
    max_call_depth: usize,
}
impl Mode {
    fn read(f: &TypedFunction, budget: &mut Budget) -> Check<Self> {
        // Six values, the function-indexed bit's bounds/index work, and the
        // actual Mode record's construction/return are paid before reading.
        budget.spend(8usize.saturating_add(words::<Self>().saturating_mul(2)))?;
        Ok(Self {
            sums: budget.sums,
            sum_function: budget.sum_function(f.id),
            head_descriptions: budget.head_descriptions,
            inline_store_addresses: budget.inline_store_addresses,
            pod_stores: budget.pod_stores,
            max_call_depth: budget.limits.max_call_depth,
        })
    }
    fn matches(&self, current: &Self, budget: &mut Budget) -> Check<bool> {
        budget.spend(18)?; // Six scalar comparisons: both reads and comparison.
        Ok(self == current)
    }
}

struct Layout {
    slots: Slots,
    mode: Mode,
}
struct Function<'a> {
    binding: desc_liveness::Binding<'a>,
    slots: Option<Layout>,
}

// The actual module, functions, selected graphs/CaptureViews and replacement
// Plan stay borrowed for this entire request. The exclusive Budget borrow is
// never exposed by the production API. Resource transfers can mark Sum modes,
// which are checked at each use; the other layout inputs and the complete
// actual-Type inline_store_roots inventory have no resource-phase write path.
// Only complete Slots are retained. Effects, Bridges, States, written sets and
// the Plan's dynamic calls/pairs still belong to each real transfer.
pub(super) struct Request<'a, 'b> {
    module: &'a TypedModule,
    graphs: &'a [FunctionGraph],
    replacements: Option<&'a replacements::Plan>,
    budget: &'b mut Budget,
    functions: Vec<Function<'a>>,
}
impl<'a, 'b> Request<'a, 'b> {
    pub(super) fn new(
        module: &'a TypedModule,
        graphs: &'a [FunctionGraph],
        refined: Option<&'a BTreeMap<usize, FunctionGraph>>,
        replacements: Option<&'a replacements::Plan>,
        budget: &'b mut Budget,
    ) -> Check<Self> {
        // This is the actual function table, replacing the old Binding table.
        // Each slot funds both eventual optional-record dispatches. Request's
        // full record includes its outer Vec; no second Vec header is added.
        budget.spend(
            module
                .functions
                .len()
                .saturating_mul(words::<Function<'a>>().saturating_add(2))
                .saturating_add(words::<Self>().saturating_mul(2))
                .saturating_add(2), // table allocation and eventual release
        )?;
        let mut functions = Vec::with_capacity(module.functions.len());
        for (index, f) in module.functions.iter().enumerate() {
            budget.spend(4)?; // Actual function/graph reads, binding and empty slot writes.
            let graph = if let Some(rows) = refined {
                budget.spend(rows.len().saturating_add(1))?;
                rows.get(&index).unwrap_or(&graphs[index])
            } else {
                &graphs[index]
            };
            functions.push(Function {
                binding: desc_liveness::Binding::new(f, graph),
                slots: None,
            });
        }
        Ok(Self {
            module,
            graphs,
            replacements,
            budget,
            functions,
        })
    }

    pub(super) fn run(
        &mut self,
        index: usize,
        effects: &[BTreeSet<Root>],
        validate: bool,
        sum_rows: Option<&[Vec<sums::Bridge>]>,
    ) -> Check<BTreeSet<Root>> {
        self.budget.spend(3)?; // Indexed binding, actual function, foreign discriminant.
        let function = &mut self.functions[index];
        let f = function.binding.function();
        if f.foreign.is_some() {
            return Ok(BTreeSet::new());
        }
        let mode = Mode::read(f, self.budget)?;
        self.budget.spend(1)?; // Complete layout presence.
        let matches = if let Some(layout) = &function.slots {
            layout.mode.matches(&mode, self.budget)?
        } else {
            false
        };
        if !matches {
            let mut slots = Slots::new(
                self.module,
                f,
                &function.binding.graph().package_views,
                self.budget,
            )?;
            if let Some(plan) = self.replacements {
                plan.extend_slots(f, &mut slots, self.budget)?;
            }
            // Pack the actual Slots and Mode records, then move the complete
            // Layout into its optional slot. Pay the replaced-slot dispatch
            // before publication. Deep contents move uniquely: no new clone,
            // Arc owner, or extra deep release is introduced by retention.
            self.budget.spend(
                words::<Slots>()
                    .saturating_mul(2)
                    .saturating_add(words::<Mode>().saturating_mul(2))
                    .saturating_add(words::<Layout>())
                    .saturating_add(words::<Option<Layout>>())
                    .saturating_add(2),
            )?;
            function.slots = Some(Layout { slots, mode });
        }
        self.budget.spend(2)?; // Optional-record selection and complete Slots borrow.
        let Function { binding, slots } = function;
        run_with_slots(
            self.module,
            binding,
            &slots.as_ref().expect("complete resource slot layout").slots,
            effects,
            validate,
            sum_rows,
            self.budget,
            self.replacements,
        )
    }

    pub(super) fn infer_effects(&mut self) -> Check<Vec<BTreeSet<Root>>> {
        self.budget.spend(self.module.functions.len())?;
        let mut effects = vec![BTreeSet::new(); self.module.functions.len()];
        let dependents = effect_dependents(self.module, self.graphs, self.budget)?;
        // Keep the original batch boundaries and consumer notification order.
        let mut dirty: Vec<usize> = (0..self.module.functions.len()).rev().collect();
        let mut next = Vec::new();
        let mut scheduled = vec![true; self.module.functions.len()];
        while !dirty.is_empty() {
            while let Some(index) = dirty.pop() {
                self.budget.spend(1)?;
                scheduled[index] = false;
                #[cfg(test)]
                TEST_EFFECT_SOLVES.with(|value| value.set(value.get() + 1));
                let found = self.run(index, &effects, false, None)?;
                self.budget
                    .spend(found.len().saturating_add(effects[index].len()))?;
                let before = effects[index].len();
                effects[index].extend(found);
                if effects[index].len() != before {
                    self.budget.spend(dependents[index].len())?;
                    for &user in &dependents[index] {
                        if !scheduled[user] {
                            self.budget.spend(1)?;
                            scheduled[user] = true;
                            next.push(user);
                        }
                    }
                }
            }
            std::mem::swap(&mut dirty, &mut next);
        }
        Ok(effects)
    }

    pub(super) fn close_pairs(&mut self) -> Check<()> {
        if let Some(plan) = self.replacements {
            plan.close_pairs(self.module, self.budget)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn slots(&self, index: usize) -> Option<&Slots> {
        self.functions[index]
            .slots
            .as_ref()
            .map(|layout| &layout.slots)
    }
    #[cfg(test)]
    fn binding(&self, index: usize) -> &desc_liveness::Binding<'a> {
        &self.functions[index].binding
    }
}

#[cfg(test)]
mod tests;
