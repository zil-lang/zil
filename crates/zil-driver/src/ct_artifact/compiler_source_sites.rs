//! Bind producer events to retained original nodes before emitting authority.
use super::*;
use source_sites::{Selection, SourceSite};
impl Compiler<'_> {
    fn select_source_site(
        &mut self,
        module: usize,
        selection: Selection,
        span: Span,
    ) -> R<Option<SourceSite>> {
        Ok(source_sites::select(
            self.modules[module].syntax,
            selection,
            self.budget.limits.max_call_depth,
            |work| self.budget.work(work, span),
        )?
        .map(|n| n.site))
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
