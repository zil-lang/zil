use super::*;
use zil_base::SourceMapEntry;

impl Expander<'_> {
    pub(super) fn map_node(
        &mut self,
        node: NodeId,
        origin: Span,
        role: ExpansionRole,
        related: Vec<Span>,
    ) -> SourceMapId {
        let id = self.map_target(SourceTarget::Node(node), origin, role, related);
        self.node_maps.insert(node, id);
        id
    }

    pub(super) fn map_target(
        &mut self,
        target: SourceTarget,
        origin: Span,
        role: ExpansionRole,
        related: Vec<Span>,
    ) -> SourceMapId {
        let id = SourceMapId(self.source_map.entries.len() as u32);
        self.source_map.entries.push(SourceMapEntry {
            id,
            target,
            origin,
            role,
            related,
            parent: self.parent_map,
        });
        id
    }

    pub(super) fn marker(&mut self, origin: Span) -> SourceMapId {
        if let Some(&id) = self.markers.get(&origin) {
            return id;
        }
        let id = self.map_target(
            SourceTarget::Span(origin),
            origin,
            ExpansionRole::Source,
            Vec::new(),
        );
        self.markers.insert(origin, id);
        id
    }

    pub(super) fn role(&mut self, node: NodeId, role: ExpansionRole, related: Vec<Span>) {
        let id = self.node_maps[&node];
        let entry = &mut self.source_map.entries[id.0 as usize];
        entry.role = role;
        entry.related.extend(related);
    }

    pub(super) fn temporary(&mut self, purpose: &str) -> String {
        let index = self.next_temporary;
        self.next_temporary += 1;
        // `$` is deliberately outside the source identifier alphabet.
        format!("${purpose}_{index}")
    }

    pub(super) fn temporary_read(&mut self, name: &str, origin: Span) -> CoreExpr {
        let value = self.expr_node(CoreExprKind::Name(name.to_owned()), origin);
        self.role(value.id, ExpansionRole::TemporaryRead, Vec::new());
        value
    }

    pub(super) fn arrow_return(&mut self, value: &s::Expr, expected: Option<&Type>) -> CoreBlock {
        let expr = self.expr_expected(value, expected);
        let statement = self.statement(CoreStatementKind::Return(Some(expr)), value.span);
        self.role(statement.id, ExpansionRole::ArrowReturn, Vec::new());
        CoreBlock {
            statements: vec![statement],
            span: value.span,
        }
    }
}
