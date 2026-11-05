//! Generated bindings route only through the retained parse+hygiene catalog.
//! A catalog row or its digest never establishes source or scope authority.
use super::*;

impl<'a> Compiler<'a> {
    pub(super) fn index_generated(&mut self) -> R<()> {
        let Some(catalog) = self.generated_catalog else {
            return Ok(());
        };
        for tree in &catalog.trees {
            self.budget.work(1, tree.module.span)?;
            let module = self.module_index(tree.module.id, tree.module.span)?;
            if tree.identity.module() != tree.module.id || tree.generation_fingerprint().is_none() {
                return Err(error(
                    tree.module.span,
                    "generated compiler tree domain differs",
                ));
            }
            for item in &tree.module.items {
                self.budget.work(1, item.span)?;
                if let s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } =
                    &item.kind
                {
                    self.budget.node(item.span)?;
                    Budget::take(
                        &mut self.budget.bytes,
                        std::mem::size_of::<Declaration>(),
                        item.span,
                        "generated function inventory bytes exhausted",
                    )?;
                    self.declarations.push(Declaration {
                        module,
                        item,
                        function,
                    });
                }
                self.index_members(module, item)?;
            }
        }
        Ok(())
    }

    pub(super) fn generated_item_symbol(
        &mut self,
        module: usize,
        item: &s::Item,
    ) -> R<Option<CtSymbolId>> {
        let Some(catalog) = self.generated_catalog else {
            return Ok(None);
        };
        for tree in &catalog.trees {
            self.budget.work(1, item.span)?;
            if tree.module.id != self.modules[module].syntax.id {
                continue;
            }
            self.budget.work(tree.module.items.len(), item.span)?;
            let local = if let Some(top) = tree
                .module
                .items
                .iter()
                .position(|node| std::ptr::eq(node, item))
            {
                zil_base::NodeId(top as u32)
            } else if let Some(found) = source_scope::original_item(
                &tree.module,
                item,
                self.budget.limits.max_call_depth,
                |work| self.budget.work(work, item.span),
            )? {
                found.declaration
            } else {
                continue;
            };
            let declaration = *tree.declarations.get(local.0 as usize).ok_or_else(|| {
                error(item.span, "generated compiler declaration inventory absent")
            })?;
            self.budget.work(tree.identity_path_len(), item.span)?;
            let generation_path = tree
                .generation_fingerprint()
                .ok_or_else(|| error(item.span, "generated compiler tree identity absent"))?;
            return Ok(Some(CtSymbolId {
                module: tree.module.id,
                declaration,
                generation_path: Some(generation_path),
            }));
        }
        Ok(None)
    }

    pub(super) fn generated_declaration(
        &mut self,
        symbol: &CtSymbolId,
        span: Span,
    ) -> R<Option<(usize, &'a s::Item)>> {
        let Some(fingerprint) = symbol.generation_path else {
            return Ok(None);
        };
        let Some(catalog) = self.generated_catalog else {
            return Ok(None);
        };
        for tree in &catalog.trees {
            let path_len = match &tree.identity {
                zil_expand::source_context::SourceTreeIdentity::Generated {
                    event_path, ..
                } => event_path.len(),
                _ => 0,
            };
            self.budget.work(
                path_len
                    .saturating_add(tree.declarations.len())
                    .saturating_add(1),
                span,
            )?;
            if tree.module.id != symbol.module || tree.generation_fingerprint() != Some(fingerprint)
            {
                continue;
            }
            let Some(local) = tree
                .declarations
                .iter()
                .position(|id| *id == symbol.declaration)
            else {
                continue;
            };
            let original = source_scope::original_declaration(
                &tree.module,
                zil_base::NodeId(local as u32),
                self.budget.limits.max_call_depth,
                |work| self.budget.work(work, span),
            )?
            .ok_or_else(|| error(span, "generated source symbol is not an actual declaration"))?;
            let module = self.module_index(symbol.module, span)?;
            return Ok(Some((module, original.item)));
        }
        Ok(None)
    }

    pub(super) fn generated_named_item(
        &mut self,
        module: usize,
        name: &str,
        span: Span,
    ) -> R<Option<&'a s::Item>> {
        let Some(catalog) = self.generated_catalog else {
            return Ok(None);
        };
        let mut found = None;
        for tree in &catalog.trees {
            self.budget.work(1, span)?;
            if tree.module.id != self.modules[module].syntax.id {
                continue;
            }
            for item in &tree.module.items {
                self.budget.work(1, span)?;
                let actual = match &item.kind {
                    s::ItemKind::Aggregate(a) => Some(&a.name),
                    s::ItemKind::Enum(e) => Some(&e.name),
                    s::ItemKind::TypeAlias(a) => Some(&a.name),
                    _ => None,
                };
                if let Some(actual) = actual {
                    self.budget
                        .work(actual.len().min(name.len()).saturating_add(1), span)?;
                    if actual == name && found.replace(item).is_some() {
                        return Err(error(span, "duplicate generated CT nominal binding"));
                    }
                }
            }
        }
        Ok(found)
    }
}
