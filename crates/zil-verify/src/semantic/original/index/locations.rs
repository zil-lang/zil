//! Coordinates into the independently traversed, retained original tree.
//! Each edge records one child in the same grammar used by the complete walk.
use super::*;

fn search(size: usize) -> usize {
    size.checked_ilog2().map_or(1, |n| n as usize + 2)
}

#[derive(Default)]
pub(super) struct Locations {
    parents: Vec<Option<(usize, usize)>>,
    nodes: BTreeMap<NodeId, usize>,
    pub(super) returns: Vec<NodeId>,
    pub(super) updates: Vec<NodeId>,
}

impl Locations {
    pub(super) fn push(&mut self, parent: Option<(usize, usize)>, b: &mut Budget) -> Check<usize> {
        let index = self.parents.len();
        b.spend(
            1 + if index == self.parents.capacity() {
                index
            } else {
                0
            },
        )?;
        self.parents.push(parent);
        Ok(index)
    }

    pub(super) fn record(&mut self, node: NodeId, index: usize, b: &mut Budget) -> Check<()> {
        b.spend(search(self.nodes.len()))?;
        if self.nodes.insert(node, index).is_some() {
            return Err(bad("duplicate original body node identity"));
        }
        Ok(())
    }

    pub(super) fn finish(&mut self, b: &mut Budget) -> Check<()> {
        for rows in [&mut self.returns, &mut self.updates] {
            b.spend(rows.len().saturating_mul(search(rows.len())))?;
            rows.sort_unstable();
        }
        Ok(())
    }

    fn get<'a>(
        &self,
        body: &'a SemanticOriginalFunctionBody,
        node: NodeId,
        b: &mut Budget,
    ) -> Check<Option<Work<'a>>> {
        if let Some(p) = &mut b.diagnostic_progress {
            p.original_cursor_queries += 1;
        }
        b.spend(search(self.nodes.len()))?;
        let Some(&index) = self.nodes.get(&node) else {
            return Ok(None);
        };
        let SemanticOriginalFunctionBody::Defined(body) = body else {
            return Err(bad("checked original coordinate has no body"));
        };
        let mut path = Vec::new();
        let mut cursor = index;
        while let Some(&(parent, slot)) = self.parents.get(cursor).and_then(Option::as_ref) {
            b.spend(
                2 + if path.len() == path.capacity() {
                    path.len()
                } else {
                    0
                },
            )?;
            path.push(slot);
            cursor = parent;
        }
        if cursor != 0 {
            return Err(bad("checked original coordinate has no body root"));
        }
        let mut work = Work::Block(body);
        let mut children = Vec::new();
        for slot in path.into_iter().rev() {
            b.spend(2)?;
            if let Some(p) = &mut b.diagnostic_progress {
                p.original_cursor_edges += 1;
            }
            work = if let Work::Block(block) = work {
                let index = block
                    .statements
                    .len()
                    .checked_sub(slot.saturating_add(1))
                    .ok_or_else(|| bad("checked original statement leaves its block"))?;
                Work::Statement(&block.statements[index])
            } else {
                children.clear();
                work.children(&mut children, b)?;
                if let Some(p) = &mut b.diagnostic_progress {
                    p.original_cursor_children += children.len();
                }
                *children
                    .get(slot)
                    .ok_or_else(|| bad("checked original child leaves its parent"))?
            };
        }
        Ok(Some(work))
    }
}

pub(in crate::semantic::original) struct Selected<'a> {
    pub(in crate::semantic::original) definitions:
        BTreeMap<SemanticDefinitionId, SelectedFunction<'a>>,
}

pub(in crate::semantic::original) struct SelectedFunction<'a> {
    pub(in crate::semantic::original) definition: &'a SemanticDefinition,
    pub(in crate::semantic::original) sites: &'a [SemanticSiteRecipe],
    pub(in crate::semantic::original) closures: &'a BTreeMap<NodeId, SemanticDefinitionId>,
    cursor: &'a DefinitionCursor,
    body: &'a SemanticOriginalFunctionBody,
}

impl<'a> SelectedFunction<'a> {
    /// Block children have reverse traversal ordinals. A larger sibling
    /// ordinal denotes an earlier declaration in the same enclosing block.
    pub(in crate::semantic::original) fn local_covers_call_binding(
        &self, local: NodeId, call: NodeId, b: &mut Budget,
    ) -> Check<bool> {
        let syntax = &self.cursor.places;
        let Some(binding) = syntax.binding(local, b)? else { return Ok(false); };
        if binding.value.is_none() { return Ok(false); }
        let locations = &self.cursor.locations;
        b.spend(search(locations.nodes.len()))?;
        let local = locations.nodes[&local];
        let Some((block, ordinal)) = locations.parents[local] else { return Ok(false); };
        for (holder, binding) in &syntax.bindings {
            b.spend(1)?;
            let Some(mut value) = binding.value else { continue; };
            while let PlaceOperation::Group(inner) | PlaceOperation::Ascribed(inner) = syntax.expression(value, b)?.operation {
                value = inner;
            }
            if value != call { continue; }
            b.spend(search(locations.nodes.len()))?;
            let mut cursor = locations.nodes[holder];
            while let Some((parent, slot)) = locations.parents[cursor] {
                b.spend(1)?;
                if parent == block { return Ok(ordinal > slot); }
                cursor = parent;
            }
        }
        Ok(false)
    }

    pub(in crate::semantic::original) fn expression(
        &self,
        id: NodeId,
        b: &mut Budget,
    ) -> Check<Option<&'a SemanticOriginalExpr>> {
        Ok(match self.cursor.locations.get(self.body, id, b)? {
            Some(Work::Expression(expression)) => {
                if expression.id != id {
                    return Err(bad("checked original coordinate changes its expression"));
                }
                Some(expression)
            }
            _ => None,
        })
    }
    pub(in crate::semantic::original) fn statement(
        &self,
        id: NodeId,
        b: &mut Budget,
    ) -> Check<Option<&'a SemanticOriginalStatement>> {
        Ok(match self.cursor.locations.get(self.body, id, b)? {
            Some(Work::Statement(statement)) => {
                if statement.id != id {
                    return Err(bad("checked original coordinate changes its statement"));
                }
                Some(statement)
            }
            _ => None,
        })
    }
    pub(in crate::semantic::original) fn node(
        &self,
        site: SemanticSiteId,
        b: &mut Budget,
    ) -> Check<Option<Node<'a>>> {
        b.spend(search(self.cursor.nodes.len()))?;
        match self.cursor.nodes.get(&site) {
            Some(SemanticOriginalNode::Expression(id)) => Ok(Some(Node::Expression(
                self.expression(*id, b)?.ok_or_else(|| {
                    bad("checked original cursor expression is absent from its exact body")
                })?,
            ))),
            Some(SemanticOriginalNode::Statement(id)) => {
                Ok(Some(Node::Statement(self.statement(*id, b)?.ok_or_else(
                    || bad("checked original cursor statement is absent from its exact body"),
                )?)))
            }
            None => Ok(None),
        }
    }
    pub(in crate::semantic::original) fn returns(&self) -> &[NodeId] {
        &self.cursor.locations.returns
    }
    pub(in crate::semantic::original) fn record_updates(&self) -> &[NodeId] {
        &self.cursor.locations.updates
    }
}

pub(in crate::semantic::original) fn select<'a>(
    source: &'a SourceIndex,
    requested: &[SemanticDefinitionId],
    b: &mut Budget,
) -> Check<Selected<'a>> {
    source.bounds(b)?;
    let program = source.program()?;
    let rows = &source.directory.definitions;
    let mut definitions = BTreeMap::new();
    for id in requested {
        b.spend(search(rows.len()).saturating_add(search(definitions.len())))?;
        if definitions.contains_key(id) {
            continue;
        }
        let cursor = rows.get(id).ok_or_else(|| {
            bad("original local cursor selects no fully checked function definition")
        })?;
        let definition = program
            .definitions
            .get(cursor.ordinal)
            .filter(|d| d.id == *id)
            .ok_or_else(|| {
                bad("original local cursor changes its immutable definition identity")
            })?;
        let SemanticDefinitionBody::FunctionWithBody { body, sites, .. } = &definition.body else {
            return Err(bad(
                "original local cursor changes its checked function kind",
            ));
        };
        definitions.insert(
            *id,
            SelectedFunction {
                definition,
                sites,
                closures: &cursor.closures,
                cursor,
                body,
            },
        );
    }
    Ok(Selected { definitions })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cursor_borrows_its_checked_original_node_without_rewalking_the_body() {
        let limits = Limits::default();
        for bytes in [
            &include_bytes!("../../fixtures/ordinary-identity-complete-call-sites.wire")[..],
            &include_bytes!("../../fixtures/phantom-closure-complete-creator.wire")[..],
        ] {
            let (module, _) = zil_ir::wire::decode_module_candidate(bytes, &limits).unwrap();
            let module = std::sync::Arc::new(module);
            let mut b = Budget::new(&limits);
            b.diagnostic_progress = Some(crate::DiagnosticProgress::default());
            b.diagnostic_body_definitions = Some(BTreeSet::new());
            b.original_input = Some(crate::OriginalInput::Module(module.clone()));
            let whole = build(&module, &mut b).unwrap();
            let grammar = snapshot(&module, &whole, &mut b).unwrap();
            let before = b.diagnostic_progress.unwrap();
            for (&id, function) in &whole.definitions {
                let selected = select(&grammar, &[id, id], &mut b).unwrap();
                let view = &selected.definitions[&id];
                for (&node, &expression) in &function.expressions {
                    assert!(std::ptr::eq(
                        expression,
                        view.expression(node, &mut b).unwrap().unwrap()
                    ));
                    assert!(view.statement(node, &mut b).unwrap().is_none());
                }
                for (&node, &statement) in &function.statements {
                    assert!(std::ptr::eq(
                        statement,
                        view.statement(node, &mut b).unwrap().unwrap()
                    ));
                    assert!(view.expression(node, &mut b).unwrap().is_none());
                }
                for (&site, node) in &function.nodes {
                    let selected = view.node(site, &mut b).unwrap().unwrap();
                    assert!(match (*node, selected) {
                        (Node::Statement(a), Node::Statement(z)) => std::ptr::eq(a, z),
                        (Node::Expression(a), Node::Expression(z)) => std::ptr::eq(a, z),
                        _ => false,
                    });
                }
            }
            let after = b.diagnostic_progress.unwrap();
            assert_eq!(after.original_body_walks, before.original_body_walks);
            assert_eq!(
                after.original_statement_and_expression_visits,
                before.original_statement_and_expression_visits
            );
            let id = *whole.definitions.keys().next().unwrap();
            assert!(
                select(
                    &grammar,
                    &[SemanticDefinitionId {
                        index: u32::MAX,
                        ..id
                    }],
                    &mut b
                )
                .is_err()
            );
            build(&module, &mut b).unwrap();
            assert!(b.diagnostic_progress.unwrap().original_body_revisits > 0);
            b.limits.max_nodes = 1;
            assert_eq!(
                select(&grammar, &[id], &mut b).err().unwrap().category,
                zil_base::ResultCategory::ResourceLimit
            );
        }
    }
}
