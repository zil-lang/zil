//! Bind producer events to retained original nodes before emitting authority.
use super::*;
use source_sites::{Selection, SourceSite};
impl<'a> Compiler<'a> {
    fn select_source_site(
        &mut self,
        module: usize,
        selection: Selection,
        span: Span,
    ) -> R<Option<SourceSite>> {
        if let (Some(root_site), Some(expression)) =
            (self.entry_source_site, self.entry_source_expression)
        {
            let selected = source_sites::relative_node(
                self.modules[module].syntax,
                source_sites::SourceNode::Expr(expression),
                selection,
                self.budget.limits.max_call_depth,
                |work| self.budget.work(work, span),
            )?;
            if let Some(selected) = selected {
                let (tree, module) = self.frozen_site_module(root_site, span)?;
                let root = source_sites::select_in_tree(
                    module,
                    tree,
                    Selection::Site(root_site),
                    self.budget.limits.max_call_depth,
                    |work| self.budget.work(work, span),
                )?
                .ok_or_else(|| error(span, "CT source query frozen root is absent"))?;
                let relative = source_sites::SourceSite {
                    tree: source_sites::SourceTreeId::Original,
                    module: module.id,
                    ordinal: selected.site.ordinal,
                };
                let original = source_sites::relative_node(
                    module,
                    root.node,
                    Selection::Site(relative),
                    self.budget.limits.max_call_depth,
                    |work| self.budget.work(work, span),
                )?
                .ok_or_else(|| {
                    error(
                        span,
                        "CT source query clone has no corresponding original structural node",
                    )
                })?;
                let same_kind = match (selected.node, original.node) {
                    (source_sites::SourceNode::Expr(a), source_sites::SourceNode::Expr(b)) => {
                        std::mem::discriminant(&a.kind) == std::mem::discriminant(&b.kind)
                    }
                    (source_sites::SourceNode::Type(a), source_sites::SourceNode::Type(b)) => {
                        std::mem::discriminant(&a.kind) == std::mem::discriminant(&b.kind)
                    }
                    _ => false,
                };
                if !same_kind {
                    return Err(error(
                        span,
                        "CT source query structural clone differs from its frozen original",
                    ));
                }
                let pointer = match original.node {
                    source_sites::SourceNode::Expr(node) => {
                        Selection::Expr(node as *const s::Expr as usize)
                    }
                    source_sites::SourceNode::Type(node) => {
                        Selection::Type(node as *const s::TypeRef as usize)
                    }
                };
                return Ok(source_sites::select_in_tree(
                    module,
                    tree,
                    pointer,
                    self.budget.limits.max_call_depth,
                    |work| self.budget.work(work, span),
                )?
                .map(|n| n.site));
            }
        }
        if let Some(original) = source_sites::select(
            self.modules[module].syntax,
            selection,
            self.budget.limits.max_call_depth,
            |work| self.budget.work(work, span),
        )? {
            return Ok(Some(original.site));
        }
        if let Some(catalog) = self.generated_catalog {
            self.budget.work(catalog.trees.len(), span)?;
            for (row, tree) in catalog.trees.iter().enumerate() {
                if let Some(node) = source_sites::select_in_tree(
                    &tree.module,
                    source_sites::SourceTreeId::Generated(row as u32),
                    selection,
                    self.budget.limits.max_call_depth,
                    |work| self.budget.work(work, span),
                )? {
                    return Ok(Some(node.site));
                }
            }
        }
        Ok(None)
    }
    fn frozen_site_module(
        &mut self,
        site: SourceSite,
        span: Span,
    ) -> R<(source_sites::SourceTreeId, &'a s::Module)> {
        match site.tree {
            source_sites::SourceTreeId::Original => {
                self.budget.work(self.modules.len(), span)?;
                let module = self
                    .modules
                    .iter()
                    .find(|m| m.source.module_id == site.module)
                    .ok_or_else(|| error(span, "CT frozen source module is absent"))?;
                Ok((site.tree, module.syntax))
            }
            source_sites::SourceTreeId::Generated(row) => {
                self.budget.work(1, span)?;
                let tree = self
                    .generated_catalog
                    .and_then(|c| c.get(row))
                    .ok_or_else(|| error(span, "CT generated source catalog row absent"))?;
                if tree.identity.module() != site.module {
                    return Err(error(span, "CT generated source catalog module differs"));
                }
                Ok((site.tree, &tree.module))
            }
        }
    }
    pub(super) fn current_expression_site(&mut self, span: Span) -> R<SourceSite> {
        let (module, address) = self
            .current_source_expr
            .ok_or_else(|| error(span, "CT event has no original source expression"))?;
        self.select_source_site(module, Selection::Expr(address), span)?
            .ok_or_else(|| error(span, "CT event expression is not retained original source"))
    }
    pub(super) fn source_site_for_arguments(
        &mut self,
        module: usize,
        args: &[s::GenericArg],
        span: Span,
    ) -> R<SourceSite> {
        if !args.is_empty() {
            if let Some(site) =
                self.select_source_site(module, Selection::Arguments(args.as_ptr() as usize), span)?
            {
                return Ok(site);
            }
        }
        self.current_expression_site(span)
    }
}
impl cfg::Builder<'_, '_> {
    pub(super) fn record_source_type_site(&mut self, value: CtValueId, span: Span) -> R<()> {
        let mut site = None;
        if let Some((module, address)) = self.compiler.current_source_type {
            site = self
                .compiler
                .select_source_site(module, Selection::Type(address), span)?;
        }
        let site = match site {
            Some(site) => site,
            None => self.compiler.current_expression_site(span)?,
        };
        self.compiler.budget.node(span)?;
        Budget::take(
            &mut self.compiler.budget.bytes,
            std::mem::size_of::<SourceTypeSite>(),
            span,
            "CT source Type site bytes exhausted",
        )?;
        self.compiler.source_type_sites.push(SourceTypeSite {
            caller: self.function_id(),
            value,
            site,
        });
        Ok(())
    }
}
