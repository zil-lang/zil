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
