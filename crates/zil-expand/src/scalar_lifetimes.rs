//! Lifetime demand collected by the current complete Probe traversal. This
//! private observation lives only until the same request's Lifetime phase.
use super::*;

pub(crate) struct Probe {
    item: *const s::Item,
    closed: bool,
    headers: usize,
    depth: usize,
}
#[derive(Clone, Copy)]
pub(crate) struct Demand {
    item: *const s::Item,
    nodes: usize,
    work: usize,
    depth: usize,
}
impl Probe {
    pub(crate) fn new(item: &s::Item) -> Option<Self> {
        if !incremental::scalar_admission::item_shape(item) {
            return None;
        }
        let headers = match &item.kind {
            s::ItemKind::Function(f) => f
                .params
                .len()
                .checked_add(usize::from(f.return_type.is_some()))?,
            s::ItemKind::Extern { .. } => 0,
            _ => return None,
        };
        Some(Self {
            item: std::ptr::from_ref(item),
            closed: true,
            headers,
            depth: 0,
        })
    }
    pub(crate) fn node(&mut self, depth: usize) {
        self.depth = self.depth.max(depth);
    }
    pub(crate) fn ty(&mut self, ty: &s::TypeRef) {
        self.closed &= matches!(&ty.kind, s::TypeKind::Named { path, args }
            if path.len() == 1 && args.is_empty()
                && (integer_name(&path[0]) || matches!(path[0].as_str(), "bool" | "void")))
            || matches!(ty.kind, s::TypeKind::Group(_));
    }
    pub(crate) fn expression(&mut self, expression: &s::Expr) {
        self.closed &= incremental::scalar_admission::expression_shape(expression);
    }
    pub(crate) fn statement(&mut self, statement: &s::Stmt) {
        self.closed &= incremental::scalar_admission::statement_shape(statement);
    }
    pub(crate) fn finish(self, before: &Limits, after: &Limits) -> Option<Demand> {
        if !self.closed {
            return None;
        }
        // In this checked domain Lifetime uses the identical ct_selection
        // node/text callbacks as Probe, plus one node per ordinary function
        // signature type at depth zero. There are no life binders, nominal
        // types, task workers or feature effects. Externs omit those headers.
        Some(Demand {
            item: self.item,
            nodes: before
                .max_nodes
                .checked_sub(after.max_nodes)?
                .checked_add(self.headers)?,
            work: before
                .max_analysis_steps
                .checked_sub(after.max_analysis_steps)?
                .checked_add(self.headers)?,
            depth: self.depth,
        })
    }
}
impl Demand {
    pub(crate) fn debit(
        self,
        item: &s::Item,
        budget: &mut generics::Budget<'_>,
    ) -> Option<(usize, usize)> {
        // Probe and Lifetime borrow the same fixed normalized product slice.
        // ensure() materializes before observation; neither phase moves or
        // mutates an observed Item. The scalar Lifetime adapter clears every
        // observation before that slice can be redistributed into Restore.
        // The pointer is only compared, never dereferenced or persisted.
        if !std::ptr::eq(self.item, item) {
            return None;
        }
        let mut remaining = budget.remaining_limits();
        if self.depth > remaining.max_call_depth {
            return None;
        }
        remaining.max_nodes = remaining.max_nodes.checked_sub(self.nodes)?;
        remaining.max_analysis_steps = remaining.max_analysis_steps.checked_sub(self.work)?;
        // Every original debit is nonnegative. Only a complete fitting demand
        // can bypass the walk; otherwise its exact ordered error runs below.
        budget.adopt_remaining(&remaining);
        Some((self.nodes, self.work))
    }
}
