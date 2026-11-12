//! Reuse only a successful stable solve in this check, never its final checks.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Mode([bool; 10]);
impl Mode {
    fn read(b: &mut Budget) -> Check<Self> {
        b.spend(10)?;
        Ok(Self([
            b.pod_stores,
            b.store_option_results,
            b.ordinary_error_places,
            b.task_borrowed_results,
            b.stored_enum_addresses,
            b.sums,
            b.tasks,
            b.registrations,
            b.pure_nominal,
            b.stored_exclusive_shapes,
        ]))
    }
    fn matches(self, b: &mut Budget) -> Check<bool> {
        let current = Self::read(b)?;
        b.spend(10)?;
        Ok(self == current)
    }
}
fn search(n: usize, b: &mut Budget) -> Check<()> {
    b.spend(
        n.checked_ilog2()
            .map_or(1, |n| n as usize + 2)
            .saturating_add(1),
    )
}
// Only immutable actual-function metadata. Neither old Context/summaries nor
// the per-Flow positive-Arc tag cache can cross the solve/replay boundary.
struct Plan {
    definitions: Vec<Option<(usize, usize)>>,
    observers: Vec<Option<(LocalId, Prefix)>>,
    partial: bool,
    place_meta: Option<partial_places::Meta>,
    tagged_locals: Option<Vec<usize>>,
    address_replacement_tags: bool,
    owned_creation: bool,
}
impl Plan {
    fn cleanup(flow: &Flow<'_>, b: &mut Budget) -> Check<()> {
        b.spend(
            flow.definitions
                .len()
                .saturating_add(flow.observers.len())
                .saturating_add(10),
        )?;
        for (_, path) in flow.observers.iter().flatten() {
            b.spend(path.len().saturating_add(1))?;
        }
        if let Some(locals) = &flow.tagged_locals {
            b.spend(locals.len().saturating_add(2))?;
        }
        if let Some(meta) = &flow.place_meta {
            meta.retained_cleanup(b)?;
        }
        Ok(())
    }
    fn take(flow: Flow<'_>) -> Self {
        Self {
            definitions: flow.definitions,
            observers: flow.observers,
            partial: flow.partial,
            place_meta: flow.place_meta,
            tagged_locals: flow.tagged_locals,
            address_replacement_tags: flow.address_replacement_tags,
            owned_creation: flow.owned_creation,
        }
    }
    fn flow<'a>(
        self,
        module: &'a TypedModule,
        f: &'a TypedFunction,
        summaries: &'a [Content],
        replacements: Option<&'a Context<'a>>,
        b: &Budget,
    ) -> Flow<'a> {
        Flow {
            module,
            f,
            summaries,
            replacements,
            definitions: self.definitions,
            observers: self.observers,
            partial: self.partial,
            place_meta: self.place_meta,
            tagged_locals: self.tagged_locals,
            address_replacement_tags: self.address_replacement_tags,
            owned_creation: self.owned_creation,
            tag_cache: (self.partial && indexed_invalidation(b))
                .then(|| RefCell::new(tag_cache::Cache::new(f))),
        }
    }
}
fn copy_rows(rows: &Rows, b: &mut Budget) -> Check<Rows> {
    b.spend(rows.len().saturating_mul(2).saturating_add(2))?;
    for (target, payload) in rows {
        b.spend(target.steps.len().saturating_mul(2).saturating_add(4))?;
        payload.cost(b)?;
        payload.cost(b)?; // owned deep copy and its cleanup
    }
    Ok(rows.clone())
}
fn retain_inputs(inputs: &[Option<State>], b: &mut Budget) -> Check<Vec<Option<State>>> {
    let words = std::mem::size_of::<Option<State>>().div_ceil(std::mem::size_of::<usize>());
    b.spend(
        inputs
            .len()
            .saturating_mul(words.saturating_mul(2).saturating_add(1))
            .saturating_add(2),
    )?;
    for state in inputs.iter().flatten() {
        state.cost(b)?; // actual duplicate
        state.cost(b)?; // held references, facts/paths/partial final cleanup
        b.spend(state.locals.len().saturating_add(state.captures.len()))?;
        for root in &state.locals {
            root.cost(b)?;
        }
        for root in state.captures.iter().flatten() {
            root.cost(b)?;
        }
        state.values.retained_cleanup(b)?;
    }
    // All budget failures above precede every added strong reference.
    Ok(inputs.to_vec())
}
pub(in crate::sums) struct Draft<'m> {
    module: &'m TypedModule,
    function: &'m TypedFunction,
    graph: &'m FunctionGraph,
    mode: Mode,
    inputs: Vec<Option<State>>,
    written: Rows,
    excluded: ExcludedEdges,
}
pub(in crate::sums) struct Entry<'m> {
    draft: Draft<'m>,
    plan: Plan,
    reads: Vec<(usize, usize)>,
    sum_marked: bool,
}
pub(in crate::sums) struct Start {
    mode: Mode,
}
impl Start {
    pub(in crate::sums) fn new(b: &mut Budget) -> Check<Self> {
        Ok(Self {
            mode: Mode::read(b)?,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(in crate::sums) fn save<'m>(
        self,
        module: &'m TypedModule,
        f: &'m TypedFunction,
        graph: &'m FunctionGraph,
        flow: &Flow<'_>,
        inputs: &[Option<State>],
        excluded: &ExcludedEdges,
        context: Option<&Context<'_>>,
        b: &mut Budget,
    ) -> Check<Option<Draft<'m>>> {
        if !self.mode.matches(b)? {
            return Ok(None);
        }
        let Some(context) = context else {
            return Ok(None);
        };
        let Some(reads) = &context.observed else {
            return Ok(None);
        };
        let reads = reads.borrow();
        search(reads.len(), b)?;
        if reads.contains(&(f.id.0 as usize)) {
            return Ok(None);
        }
        drop(reads);
        Plan::cleanup(flow, b)?;
        b.spend(excluded.len().saturating_mul(2).saturating_add(2))?;
        let written = copy_rows(&context.written.borrow(), b)?;
        let inputs = retain_inputs(inputs, b)?;
        Ok(Some(Draft {
            module,
            function: f,
            graph,
            mode: self.mode,
            inputs,
            written,
            excluded: excluded.clone(),
        }))
    }
}
impl<'m> Draft<'m> {
    pub(in crate::sums) fn finish(
        self,
        flow: Flow<'_>,
        b: &mut Budget,
    ) -> Check<Option<Entry<'m>>> {
        if !self.mode.matches(b)? {
            return Ok(None);
        }
        b.spend(2)?; // actual sum marker and owned plan/checkpoint record
        let sum_marked = b.sum_function(self.function.id);
        Ok(Some(Entry {
            draft: self,
            plan: Plan::take(flow),
            reads: Vec::new(),
            sum_marked,
        }))
    }
}
impl Entry<'_> {
    pub(super) fn written(&mut self, b: &mut Budget) -> Check<Rows> {
        b.spend(1)?;
        Ok(std::mem::take(&mut self.draft.written))
    }
    pub(in crate::sums) fn replay(
        self,
        summaries: &[Content],
        context: Option<&Context<'_>>,
        b: &mut Budget,
    ) -> Check<Solved> {
        let Draft {
            module,
            function,
            inputs,
            excluded,
            ..
        } = self.draft;
        // This path allocates only the final row container, not the old
        // initial State/queue containers paid by the full fresh solver.
        b.spend(function.blocks.len().saturating_add(2))?;
        let flow = self.plan.flow(module, function, summaries, context, b);
        let (result, rows) = replay_inputs(&flow, inputs, true, b)?;
        Ok((result, rows, excluded))
    }
}
pub(super) struct Cache<'m> {
    versions: Vec<usize>,
    entries: Option<Vec<Option<Entry<'m>>>>,
    enabled: bool,
}
impl<'m> Cache<'m> {
    pub(super) fn new(count: usize, b: &mut Budget) -> Check<Self> {
        b.spend(count.saturating_add(2))?;
        Ok(Self {
            versions: vec![0; count],
            entries: None,
            enabled: true,
        })
    }
    pub(super) fn begin(&mut self, index: usize, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        if let Some(entries) = &mut self.entries {
            b.spend(1)?;
            entries[index] = None;
        }
        Ok(self.enabled)
    }
    pub(super) fn publish(
        &mut self,
        index: usize,
        entry: Option<Entry<'m>>,
        reads: &BTreeSet<usize>,
        b: &mut Budget,
    ) -> Check<()> {
        let Some(mut entry) = entry else {
            return Ok(());
        };
        search(reads.len(), b)?;
        if !self.enabled || reads.contains(&index) {
            return Ok(());
        }
        // Every complete actual read, not a digest or summary key count.
        b.spend(reads.len().saturating_mul(4).saturating_add(2))?;
        let mut versions = Vec::with_capacity(reads.len());
        for &callee in reads {
            let Some(&version) = self.versions.get(callee) else {
                return Ok(());
            };
            versions.push((callee, version));
        }
        if self.entries.is_none() {
            let words =
                std::mem::size_of::<Option<Entry<'m>>>().div_ceil(std::mem::size_of::<usize>());
            b.spend(
                self.versions
                    .len()
                    .saturating_mul(words.saturating_add(1))
                    .saturating_add(2),
            )?;
        }
        b.spend(2)?;
        entry.reads = versions;
        let count = self.versions.len();
        self.entries
            .get_or_insert_with(|| (0..count).map(|_| None).collect())[index] = Some(entry);
        Ok(())
    }
    pub(super) fn published(&mut self, index: usize, b: &mut Budget) -> Check<()> {
        b.spend(2)?;
        let Some(next) = self.versions[index].checked_add(1) else {
            // No wrap/ABA and no changed inference behavior on metadata overflow.
            self.enabled = false;
            self.entries = None;
            return Ok(());
        };
        self.versions[index] = next;
        Ok(())
    }
    pub(super) fn take(
        &mut self,
        index: usize,
        module: &TypedModule,
        f: &TypedFunction,
        graph: &FunctionGraph,
        b: &mut Budget,
    ) -> Check<Option<Entry<'m>>> {
        b.spend(1)?;
        let Some(entries) = &mut self.entries else {
            return Ok(None);
        };
        b.spend(2)?;
        let Some(entry) = entries[index].take() else {
            return Ok(None);
        };
        b.spend(4)?;
        if !self.enabled
            || !std::ptr::eq(entry.draft.module, module)
            || !std::ptr::eq(entry.draft.function, f)
            || !std::ptr::eq(entry.draft.graph, graph)
        {
            return Ok(None);
        }
        if !entry.draft.mode.matches(b)? {
            return Ok(None);
        }
        b.spend(2)?;
        if entry.sum_marked && !b.sum_function(f.id) {
            return Ok(None);
        }
        b.spend(entry.reads.len().saturating_mul(3))?;
        for &(callee, version) in &entry.reads {
            if self.versions.get(callee) != Some(&version) {
                return Ok(None);
            }
        }
        #[cfg(test)]
        tests::hit(f.id.0 as usize, &entry.draft.inputs);
        Ok(Some(entry))
    }
}
#[cfg(test)]
pub(super) fn drain_hits() -> Vec<usize> {
    tests::drain_hits()
}
#[cfg(test)]
mod tests;
