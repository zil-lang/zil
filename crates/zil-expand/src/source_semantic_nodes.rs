//! Index only the actual minted nodes requested by the consumed emission log.
//! This is a bounded immutable Core inventory, not a permission cache.
use super::*;
use zil_base::NodeId;
#[derive(Clone, Copy)]
pub(crate) enum Node<'a> {
    Expression(&'a CoreExpr),
    Statement(&'a CoreStatement),
}
pub(crate) type Key = (ModuleId, NodeId, NodeId, bool);
pub(crate) struct Inventory<'a> {
    nodes: Vec<(Key, Option<Node<'a>>)>,
}
impl<'a> Inventory<'a> {
    pub(crate) fn new(
        wanted: impl ExactSizeIterator<Item = Key>,
        budget: &mut Budget<'_>,
    ) -> R<Self> {
        let span = Span::default();
        let count = wanted.len();
        let bytes = count
            .checked_mul(std::mem::size_of::<(Key, Option<Node<'a>>)>())
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Expand,
                    "actual node directory allocation overflow",
                    Some(span),
                )
            })?;
        budget.lineage_bytes(bytes, span)?;
        let mut nodes = Vec::new();
        nodes.try_reserve_exact(count).map_err(|_| {
            Diagnostic::limit(
                Phase::Expand,
                "actual node directory allocation failed",
                Some(span),
            )
        })?;
        for key in wanted {
            budget.node(span, 0)?;
            nodes.push((key, None));
        }
        budget.identity_work(
            count
                .saturating_mul(count.checked_ilog2().map_or(1, |n| n as usize + 1))
                .saturating_mul(12),
            span,
        )?;
        nodes.sort_unstable_by_key(|(key, _)| *key);
        for pair in nodes.windows(2) {
            budget.identity_work(1, span)?;
            if pair[0].0 == pair[1].0 {
                return Err(error(
                    span,
                    "actual semantic emission identity was recorded twice",
                ));
            }
        }
        Ok(Self { nodes })
    }
    pub(crate) fn get(&self, key: &Key, budget: &mut Budget<'_>) -> R<Option<Node<'a>>> {
        budget.identity_work(
            self.nodes
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
            Span::default(),
        )?;
        Ok(self
            .nodes
            .binary_search_by_key(key, |(key, _)| *key)
            .ok()
            .and_then(|index| self.nodes[index].1))
    }
    fn enter(
        &mut self,
        key: Key,
        node: Node<'a>,
        budget: &mut Budget<'_>,
        span: Span,
        depth: usize,
    ) -> R<()> {
        budget.node(span, depth)?;
        budget.identity_work(
            self.nodes
                .len()
                .checked_ilog2()
                .map_or(1, |n| n as usize + 2),
            span,
        )?;
        if let Ok(index) = self.nodes.binary_search_by_key(&key, |(key, _)| *key) {
            if self.nodes[index].1.replace(node).is_some() {
                return Err(error(span, "actual Core node identity is reused"));
            }
        }
        Ok(())
    }
    pub(crate) fn expression(
        &mut self,
        module: ModuleId,
        creator: NodeId,
        e: &'a CoreExpr,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        self.enter(
            (module, creator, e.id, false),
            Node::Expression(e),
            budget,
            e.span,
            depth,
        )?;
        match &e.kind {
            CoreExprKind::MakeDynBorrow { source, .. }
            | CoreExprKind::MakeDynOwned { source, .. } => {
                self.expression(module, creator, source, budget, depth + 1)?
            }
            CoreExprKind::Ascribed { value, .. }
            | CoreExprKind::NumericConvert { value, .. }
            | CoreExprKind::IntegerConvert { value, .. }
            | CoreExprKind::CompileTime(value)
            | CoreExprKind::Try(value) => {
                self.expression(module, creator, value, budget, depth + 1)?
            }
            CoreExprKind::Unary { operand, .. } => {
                self.expression(module, creator, operand, budget, depth + 1)?
            }
            CoreExprKind::Binary { left, right, .. } => {
                self.expression(module, creator, left, budget, depth + 1)?;
                self.expression(module, creator, right, budget, depth + 1)?;
            }
            CoreExprKind::TypeApply { callee, .. } => {
                self.expression(module, creator, callee, budget, depth + 1)?
            }
            CoreExprKind::Call { callee, arguments }
            | CoreExprKind::CallDynSlot {
                callee, arguments, ..
            } => {
                self.expression(module, creator, callee, budget, depth + 1)?;
                for arg in arguments {
                    self.expression(module, creator, arg, budget, depth + 1)?;
                }
            }
            CoreExprKind::Tuple(values) | CoreExprKind::Array(values) => {
                for value in values {
                    self.expression(module, creator, value, budget, depth + 1)?;
                }
            }
            CoreExprKind::Index { object, index } => {
                self.expression(module, creator, object, budget, depth + 1)?;
                self.expression(module, creator, index, budget, depth + 1)?;
            }
            CoreExprKind::Slice { object, start, end } => {
                self.expression(module, creator, object, budget, depth + 1)?;
                for value in [start, end].into_iter().flatten() {
                    self.expression(module, creator, value, budget, depth + 1)?;
                }
            }
            CoreExprKind::Member { object, .. } => {
                self.expression(module, creator, object, budget, depth + 1)?
            }
            CoreExprKind::Record { fields, .. } => {
                for (_, value) in fields {
                    self.expression(module, creator, value, budget, depth + 1)?;
                }
            }
            CoreExprKind::RecordUpdate { base, fields, .. } => {
                self.expression(module, creator, base, budget, depth + 1)?;
                for (_, value) in fields {
                    self.expression(module, creator, value, budget, depth + 1)?;
                }
            }
            CoreExprKind::Sequence { bindings, value } => {
                for (_, value) in bindings {
                    self.expression(module, creator, value, budget, depth + 1)?;
                }
                self.expression(module, creator, value, budget, depth + 1)?;
            }
            CoreExprKind::Closure { body, .. } => {
                self.block(module, creator, body, budget, depth + 1)?
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                self.expression(module, creator, condition, budget, depth + 1)?;
                self.expression(module, creator, then_value, budget, depth + 1)?;
                self.expression(module, creator, else_value, budget, depth + 1)?;
            }
            CoreExprKind::Spread(_)
            | CoreExprKind::Group(_)
            | CoreExprKind::Character(_)
            | CoreExprKind::Range { .. }
            | CoreExprKind::Pipeline { .. }
            | CoreExprKind::RecordConstructor { .. }
            | CoreExprKind::Splice(_)
            | CoreExprKind::CompileTimeBlock(_)
            | CoreExprKind::SyntaxQuote { .. }
            | CoreExprKind::SyntaxIsland { .. }
            | CoreExprKind::GenericApply { .. } => {
                return Err(unsupported(
                    e.span,
                    "original operation has not reached its actual Core elaboration",
                ));
            }
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::Bool(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Unit
            | CoreExprKind::Name(_)
            | CoreExprKind::ReceiverMember(_)
            | CoreExprKind::Unsupported(_) => {}
        }
        Ok(())
    }
    pub(crate) fn block(
        &mut self,
        module: ModuleId,
        creator: NodeId,
        body: &'a CoreBlock,
        budget: &mut Budget<'_>,
        depth: usize,
    ) -> R<()> {
        budget.node(body.span, depth)?;
        for s in &body.statements {
            self.enter(
                (module, creator, s.id, true),
                Node::Statement(s),
                budget,
                s.span,
                depth + 1,
            )?;
            match &s.kind {
                CoreStatementKind::Binding { initializer, .. } => {
                    if let Some(value) = initializer {
                        self.expression(module, creator, value, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::TupleBinding { initializer, .. } => {
                    self.expression(module, creator, initializer, budget, depth + 1)?
                }
                CoreStatementKind::Assign { target, value } => {
                    self.expression(module, creator, target, budget, depth + 1)?;
                    self.expression(module, creator, value, budget, depth + 1)?;
                }
                CoreStatementKind::Expression(value) => {
                    self.expression(module, creator, value, budget, depth + 1)?
                }
                CoreStatementKind::Return(value) => {
                    if let Some(value) = value {
                        self.expression(module, creator, value, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::If {
                    condition,
                    then_body,
                    else_body,
                } => {
                    self.expression(module, creator, condition, budget, depth + 1)?;
                    self.block(module, creator, then_body, budget, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(module, creator, body, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::IfLet {
                    value,
                    then_body,
                    else_body,
                    ..
                } => {
                    self.expression(module, creator, value, budget, depth + 1)?;
                    self.block(module, creator, then_body, budget, depth + 1)?;
                    if let Some(body) = else_body {
                        self.block(module, creator, body, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::While { condition, body } => {
                    self.expression(module, creator, condition, budget, depth + 1)?;
                    self.block(module, creator, body, budget, depth + 1)?;
                }
                CoreStatementKind::WhileLet { value, body, .. } => {
                    self.expression(module, creator, value, budget, depth + 1)?;
                    self.block(module, creator, body, budget, depth + 1)?;
                }
                CoreStatementKind::ForRange {
                    start, end, body, ..
                } => {
                    self.expression(module, creator, start, budget, depth + 1)?;
                    self.expression(module, creator, end, budget, depth + 1)?;
                    self.block(module, creator, body, budget, depth + 1)?;
                }
                CoreStatementKind::Match { value, arms } => {
                    self.expression(module, creator, value, budget, depth + 1)?;
                    for arm in arms {
                        self.block(module, creator, &arm.body, budget, depth + 1)?;
                    }
                }
                CoreStatementKind::Block(body)
                | CoreStatementKind::Defer(body)
                | CoreStatementKind::Unsafe(body) => {
                    self.block(module, creator, body, budget, depth + 1)?
                }
                CoreStatementKind::ForEach { .. }
                | CoreStatementKind::CompoundAssign { .. }
                | CoreStatementKind::Yield(_)
                | CoreStatementKind::Loop(_)
                | CoreStatementKind::With { .. }
                | CoreStatementKind::CompileTime(_) => {
                    return Err(unsupported(
                        s.span,
                        "original statement has not reached its actual Core elaboration",
                    ));
                }
                CoreStatementKind::Break
                | CoreStatementKind::Continue
                | CoreStatementKind::Unsupported(_) => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_directory_preserves_creator_kind_and_duplicate_rejection() {
        let limits = Limits::default();
        let mut budget = Budget::new(&limits);
        let a = (ModuleId(0), NodeId(1), NodeId(7), false);
        let b = (ModuleId(0), NodeId(2), NodeId(7), false);
        let expression = CoreExpr {
            id: NodeId(7),
            span: Span::default(),
            kind: CoreExprKind::Unit,
        };
        let mut inventory = Inventory::new([b, a].into_iter(), &mut budget).unwrap();
        for creator in [NodeId(1), NodeId(2)] {
            inventory
                .expression(ModuleId(0), creator, &expression, &mut budget, 0)
                .unwrap();
        }
        assert!(
            matches!(inventory.get(&a, &mut budget).unwrap(), Some(Node::Expression(node)) if std::ptr::eq(node, &expression))
        );
        assert!(
            inventory
                .get(&(ModuleId(0), NodeId(1), NodeId(7), true), &mut budget)
                .unwrap()
                .is_none()
        );
        assert!(
            inventory
                .expression(ModuleId(0), NodeId(1), &expression, &mut budget, 0)
                .is_err()
        );
        assert!(Inventory::new([a, a].into_iter(), &mut budget).is_err());
    }
}
