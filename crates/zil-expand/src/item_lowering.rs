//! Private scalar item receipts. Actual current Core leaves and provenance are
//! retained only under the original lowering inputs and current map validation.
use super::*;
use std::sync::Arc;
use zil_base::SourceMapEntry;
use zil_syntax::ParsedSource;

#[derive(Clone, Copy, Debug)]
struct ItemState {
    node: u32,
    temporary: u32,
    map: usize,
    work: usize,
}
#[derive(Clone, Debug)]
pub(super) struct ItemLowerReceipt {
    span: Span,
    function: usize,
    before: ItemState,
    after: ItemState,
    binding: FunctionLowerBinding,
}
#[derive(Clone)]
pub(super) struct ItemLowerPlan {
    pub(super) old: Arc<ExpandedModule>,
    pub(super) source: ParsedSource,
}
impl Expander<'_> {
    fn item_state(&self) -> ItemState {
        ItemState {
            node: self.next,
            temporary: self.next_temporary,
            map: self.source_map.entries.len(),
            work: self.callable_work,
        }
    }
    fn item_function_index(&self) -> usize {
        self.function_cursor.unwrap_or(self.functions.len())
    }
    pub(super) fn push_function(&mut self, function: CoreFunction) {
        if let Some(index) = &mut self.function_cursor {
            self.functions.set_owner(*index, Arc::new(function));
            *index += 1;
        } else {
            self.functions.push(function);
        }
    }
    pub(super) fn lower_item(
        &mut self,
        source: &SourceUnit,
        ordinal: usize,
        item: &s::Item,
        plan: Option<&ItemLowerPlan>,
    ) {
        let before = self.item_state();
        let function_index = self.item_function_index();
        if let Some(plan) = plan
            && self.replay_item(source, ordinal, item, plan)
        {
            self.reused_lower_items += 1;
            return;
        }
        self.item(item, None, None, None);
        self.executed_lower_items += 1;
        if !self.capture_lower_bindings
            || !self.item_is_closed()
            || self.item_function_index() != function_index + 1
        {
            return;
        }
        let after = self.item_state();
        let Some(function) = self.functions.get(function_index) else {
            return;
        };
        let Some(binding) = self.function_lower_bindings.get(&function.id).copied() else {
            return;
        };
        let Some(entries) = self.source_map.entries.get(before.map..after.map) else {
            return;
        };
        if !entries
            .iter()
            .all(|entry| closed_map_entry(entry, before, after, item.span))
        {
            return;
        }
        self.item_lower_receipts.insert(
            ordinal,
            ItemLowerReceipt {
                span: item.span,
                function: function_index,
                before,
                after,
                binding,
            },
        );
    }
    fn item_is_closed(&self) -> bool {
        self.diagnostics.is_empty()
            && self.unsupported.is_empty()
            && self.aggregates.is_empty()
            && self.enums.is_empty()
            && self.generated_imports.is_empty()
            && self.generated_exports.is_empty()
            && self.imported_public_traits.is_empty()
            && self.trait_method_names.is_empty()
            && self.dyn_method_emissions.is_empty()
            && self.closure_context_emissions.is_empty()
            && self.callable_context_emissions.is_empty()
            && self.project_trait_work == 0
            && self.project_trait_nodes == 0
            && self.member_owner_work == 0
            && self.parent_map.is_none()
            && self.current_function.is_none()
    }
    fn replay_item(
        &mut self,
        source: &SourceUnit,
        ordinal: usize,
        item: &s::Item,
        plan: &ItemLowerPlan,
    ) -> bool {
        if !self.item_is_closed() {
            return false;
        }
        let Some(receipt) = plan.old.item_lower_receipts.get(&ordinal) else {
            return false;
        };
        let current = self.item_state();
        if receipt.span != item.span
            || current.node != receipt.before.node
            || current.temporary != receipt.before.temporary
            || current.map != receipt.before.map
            || self.item_function_index() != receipt.function
            || plan.source.source().file_id != source.file_id
        {
            return false;
        }
        let old_source = plan.source.source();
        let old = old_source.text.get(item.span.start..item.span.end);
        let current_text = source.text.get(item.span.start..item.span.end);
        if old.is_none() || old != current_text {
            return false;
        }
        let Some(work) = receipt.before.work.checked_sub(receipt.after.work) else {
            return false;
        };
        let Some(left) = self.callable_work.checked_sub(work) else {
            return false;
        };
        let Some(entries) = plan
            .old
            .source_map
            .entries
            .get(receipt.before.map..receipt.after.map)
        else {
            return false;
        };
        let Some(function) = plan.old.core.functions.owner(receipt.function) else {
            return false;
        };
        // This real output tree was installed from this exact old module before
        // entering the ordered item loop; untouched leaves remain its leaves.
        if !matches!(self.functions.owner(receipt.function), Some(current) if Arc::ptr_eq(current,function))
        {
            return false;
        }
        for entry in entries {
            match entry.target {
                SourceTarget::Node(node) => {
                    self.node_maps.insert(node, entry.id);
                }
                SourceTarget::Span(span) => {
                    self.markers.insert(span, entry.id);
                }
                SourceTarget::Parameter { .. } => {}
            }
        }
        self.source_map.entries.extend(entries.iter().cloned());
        self.copied_lower_map_entries += entries.len();
        self.callable_work = left;
        self.next = receipt.after.node;
        self.next_temporary = receipt.after.temporary;
        self.function_lower_bindings
            .insert(function.id, receipt.binding);
        if receipt.binding.node_named {
            self.node_named_functions.insert(function.id);
        }
        *self
            .function_cursor
            .as_mut()
            .expect("item replay has an installed output owner") += 1;
        let mut next = receipt.clone();
        next.before.work = current.work;
        next.after.work = left;
        self.item_lower_receipts.insert(ordinal, next);
        true
    }
}
fn closed_map_entry(
    entry: &SourceMapEntry,
    before: ItemState,
    after: ItemState,
    span: Span,
) -> bool {
    let within = |other: Span| {
        other.file == span.file
            && span.start <= other.start
            && other.start <= other.end
            && other.end <= span.end
    };
    if !within(entry.origin)
        || !entry.related.iter().copied().all(within)
        || entry.parent.is_some_and(|parent| {
            (parent.0 as usize) < before.map || parent.0 as usize >= after.map
        })
    {
        return false;
    }
    match entry.target {
        SourceTarget::Node(node) => node.0 >= before.node && node.0 < after.node,
        SourceTarget::Parameter { function, .. } => {
            function.0 >= before.node && function.0 < after.node
        }
        SourceTarget::Span(other) => within(other),
    }
}

pub(super) fn debit_expanded(
    expanded: &ExpandedModule,
    remaining: &mut Limits,
    source_count: usize,
    span: Span,
) -> CompileResult<usize> {
    // New member constraints share the project's actual remaining allowance.
    remaining.max_analysis_steps = remaining
        .max_analysis_steps
        .checked_sub(expanded.member_owner_work)
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "project member owner work budget exceeded",
                Some(span),
            )]
        })?;
    remaining.max_nodes = remaining
        .max_nodes
        .checked_sub(expanded.member_owner_nodes)
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "project member owner node budget exceeded",
                Some(span),
            )]
        })?;
    // New project trait work shares the project's remaining allowance; a
    // later implementation module does not receive those steps again.
    remaining.max_analysis_steps = remaining
        .max_analysis_steps
        .checked_sub(expanded.project_trait_work)
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "project trait work budget exceeded",
                Some(span),
            )]
        })?;
    remaining.max_nodes = remaining
        .max_nodes
        .checked_sub(expanded.project_trait_nodes)
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "project trait instance node budget exceeded",
                Some(span),
            )]
        })?;
    remaining.max_nodes = remaining
        .max_nodes
        .checked_sub(expanded.source_map.entries.len())
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "project source-map node budget exceeded",
                Some(span),
            )]
        })?;
    // Raw/fresh paths retain the original checked map walk at this exact
    // position. A sealed hit carries the same already validated sum.
    let map_work = expanded.source_map_work.or_else(|| {
        expanded
            .source_map
            .entries
            .iter()
            .try_fold(source_count, |work, entry| {
                work.checked_add(entry.related.len().saturating_add(1))
            })
    });
    remaining.max_analysis_steps = map_work
        .and_then(|work| remaining.max_analysis_steps.checked_sub(work))
        .ok_or_else(|| {
            vec![Diagnostic::limit(
                Phase::Expand,
                "project source-map work budget exceeded",
                Some(span),
            )]
        })?;
    Ok(map_work.expect("successful checked source-map debit"))
}

/// Consume complete shared syntax items directly. The current scalar producer
/// admitted these exact items; richer products keep the ordinary full pipeline.
pub(super) fn expand_shared_scalar<S: std::borrow::Borrow<SourceUnit>>(
    sources: &[S],
    modules: &[Arc<modules::SharedScalarModule>],
    limits: &Limits,
    products: &mut generics::products::Products,
    reuse: &mut incremental::Reuse<'_>,
) -> CompileResult<Option<SharedProgramModules>> {
    if products.metered()
        || products.has_publications()
        || sources.len() != modules.len()
        || modules.iter().any(|module| {
            module.items.iter().any(|item| {
                !matches!(
                    item.kind,
                    s::ItemKind::Function(_) | s::ItemKind::Extern { .. }
                )
            })
        })
    {
        return Ok(None);
    }
    let mut remaining = products.trait_limits(limits);
    let mut output = Vec::with_capacity(modules.len());
    for (source, module) in sources.iter().zip(modules) {
        let source: &SourceUnit = source.borrow();
        let input_limits = remaining.clone();
        let cached = reuse.lower(module.span.file, &remaining);
        let expanded = if let Some(cached) = cached {
            cached
        } else {
            reuse.lower_executed();
            let clock = reuse.clock();
            let result = expand_scalar_items(
                source,
                module,
                sources,
                &remaining,
                products,
                reuse.item_lower_plan(module.span.file),
            );
            reuse.production(clock);
            let mut expanded = result?;
            let work = debit_expanded(&expanded, &mut remaining, sources.len(), module.span)?;
            expanded.source_map_work = Some(work);
            expanded.lowering_demand.work = expanded.lowering_demand.work.max(work);
            reuse.save_lower(module.span.file, &input_limits, &expanded);
            // Debits must occur exactly once for both cached and actual work.
            output.push((
                ProgramModule {
                    exports: scalar_exports(module, &expanded.core),
                    core: expanded.core,
                    imports: module.imports.clone(),
                },
                expanded.source_map,
            ));
            continue;
        };
        debit_expanded(&expanded, &mut remaining, sources.len(), module.span)?;
        output.push((
            ProgramModule {
                exports: scalar_exports(module, &expanded.core),
                core: expanded.core,
                imports: module.imports.clone(),
            },
            expanded.source_map,
        ));
    }
    products.finish_closure_contexts(limits)?;
    products.finish_callable_contexts(limits)?;
    Ok(Some(output))
}
fn expand_scalar_items<S: std::borrow::Borrow<SourceUnit>>(
    source: &SourceUnit,
    module: &modules::SharedScalarModule,
    sources: &[S],
    limits: &Limits,
    products: &generics::products::Products,
    plan: Option<ItemLowerPlan>,
) -> CompileResult<ExpandedModule> {
    let declarations = BTreeMap::new();
    let mut e = Expander::new(module.id, true, limits, None, &declarations);
    e.capture_lower_bindings = true;
    e.member_patterns = Some(&products.member_patterns);
    e.dynamic_catalog = Some(&products.dynamic);
    e.storage_catalog = Some(&products.storage);
    e.method_providers = Some(&products.method_providers);
    if let Some(plan) = &plan {
        e.functions = plan.old.core.functions.clone();
        e.function_cursor = Some(0);
    }
    for (ordinal, item) in module.items.iter().enumerate() {
        e.source_item_ordinal = ordinal;
        e.lower_item(source, ordinal, item, plan.as_ref());
    }
    if !e.diagnostics.is_empty() {
        return Err(e.diagnostics);
    }
    if e.function_cursor
        .is_some_and(|cursor| cursor != e.functions.len())
    {
        return expand_scalar_items(source, module, sources, limits, products, None);
    }
    finish_lowering(source, module.id, sources, limits, limits, e)
}
fn scalar_exports(module: &modules::SharedScalarModule, core: &CoreModule) -> Vec<NodeId> {
    let names: BTreeMap<_, _> = core.functions.iter().map(|f| (&f.name, f.id)).collect();
    module
        .items
        .iter()
        .filter_map(|item| {
            if !item.public {
                return None;
            }
            let s::ItemKind::Function(function) = &item.kind else {
                return None;
            };
            names.get(&function.name).copied()
        })
        .collect()
}
