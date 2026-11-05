//! Synchronous clone correspondence. Addresses locate only the currently
//! visited mutable item; actual authority is its frozen structural SourceSite.
use super::*;
use compiler::source_sites::{SiteEvent, SourceNode, SourceSite, SourceTreeId};

#[derive(Default)]
pub(super) struct BoundSites {
    nodes: BTreeMap<(bool, usize), SourceSite>,
    bytes: u64,
}
fn key(node: SourceNode<'_>) -> (bool, usize) {
    (matches!(node, SourceNode::Type(_)), node.address())
}
struct Clear<'a> {
    nodes: &'a mut BTreeMap<(bool, usize), SourceSite>,
    budget: &'a mut zil_ct::CtExecutionBudget,
    max_depth: usize,
}
impl zil_expand::ct_selection::Visitor for Clear<'_> {
    fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
        if depth > self.max_depth {
            return Err(category("clone source invalidation depth exhausted", span));
        }
        self.budget
            .reserve_analysis_work(1 + self.nodes.len().checked_ilog2().unwrap_or(0) as u64)
    }
    fn text(&mut self, _: &str, _: Span) -> CompileResult<()> {
        Ok(())
    }
    fn expression(&mut self, expression: &s::Expr, depth: usize) -> CompileResult<()> {
        self.nodes.remove(&key(SourceNode::Expr(expression)));
        zil_expand::ct_selection::walk_expression(self, expression, depth)
    }
    fn ty(&mut self, ty: &s::TypeRef, depth: usize) -> CompileResult<()> {
        self.nodes.remove(&key(SourceNode::Type(ty)));
        zil_expand::ct_selection::walk_type(self, ty, depth)
    }
}
impl Materializer<'_, '_> {
    /// Called only at the trusted first clone boundary, before any rewrite.
    pub(super) fn bind_source_items(
        &mut self,
        source: &s::Module,
        items: &[s::Item],
        tree: SourceTreeId,
    ) -> CompileResult<()> {
        let budget = &mut self.environment.budget;
        let mut count = 0usize;
        compiler::source_sites::observe(source, tree, self.max_depth, |event| {
            match event {
                SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                SiteEvent::Node(_) => count += 1,
            }
            Ok(())
        })?;
        let bytes = count as u64 * std::mem::size_of::<(bool, SourceSite)>() as u64;
        budget.reserve_temporary_bytes(bytes)?;
        let mut canonical = Vec::with_capacity(count);
        compiler::source_sites::observe(source, tree, self.max_depth, |event| {
            match event {
                SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                SiteEvent::Node(node) => canonical.push((key(node.node).0, node.site)),
            }
            Ok(())
        })?;
        let mut ordinal = 0;
        let bound = &mut self.bound_source_sites;
        for item in items {
            compiler::source_sites::observe_item(item, source.id, tree, self.max_depth, |event| {
                match event {
                    SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                    SiteEvent::Node(node) => {
                        let Some((tag, site)) = canonical.get(ordinal) else {
                            return Err(category("clone has extra source nodes", item.span));
                        };
                        if *tag != key(node.node).0 {
                            return Err(category(
                                "clone source node kind/order differs",
                                item.span,
                            ));
                        }
                        budget.reserve_analysis_work(
                            1 + bound.nodes.len().checked_ilog2().unwrap_or(0) as u64,
                        )?;
                        budget.reserve_temporary_bytes(96)?;
                        bound.bytes += 96;
                        if bound.nodes.insert(key(node.node), *site).is_some() {
                            return Err(category("clone source node repeats", item.span));
                        }
                        ordinal += 1;
                    }
                }
                Ok(())
            })?;
        }
        let complete = ordinal == canonical.len();
        drop(canonical);
        budget.release_temporary_bytes(bytes);
        if !complete {
            return Err(category("clone omits frozen source nodes", source.span));
        }
        Ok(())
    }
    /// Moving a source-shaped item cannot mint provenance. Serialize the
    /// current mapping, including holes left by actual substitutions, then
    /// restore precisely those sites at the receiving addresses.
    pub(super) fn take_item_sites(
        &mut self,
        item: &s::Item,
    ) -> CompileResult<Vec<Option<SourceSite>>> {
        let mut count = 0usize;
        let budget = &mut self.environment.budget;
        compiler::source_sites::observe_item(
            item,
            ModuleId(0),
            SourceTreeId::Original,
            self.max_depth,
            |event| {
                match event {
                    SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                    SiteEvent::Node(_) => count += 1,
                }
                Ok(())
            },
        )?;
        budget.reserve_temporary_bytes(
            count as u64 * std::mem::size_of::<Option<SourceSite>>() as u64,
        )?;
        let mut sites = Vec::with_capacity(count);
        let bound = &mut self.bound_source_sites;
        compiler::source_sites::observe_item(
            item,
            ModuleId(0),
            SourceTreeId::Original,
            self.max_depth,
            |event| {
                match event {
                    SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                    SiteEvent::Node(node) => {
                        budget.reserve_analysis_work(
                            1 + bound.nodes.len().checked_ilog2().unwrap_or(0) as u64,
                        )?;
                        let site = bound.nodes.remove(&key(node.node));
                        if site.is_some() {
                            bound.bytes -= 96;
                            budget.release_temporary_bytes(96);
                        }
                        sites.push(site);
                    }
                }
                Ok(())
            },
        )?;
        Ok(sites)
    }
    pub(super) fn restore_item_sites(
        &mut self,
        item: &s::Item,
        sites: Vec<Option<SourceSite>>,
    ) -> CompileResult<()> {
        let budget = &mut self.environment.budget;
        let bound = &mut self.bound_source_sites;
        let mut ordinal = 0;
        compiler::source_sites::observe_item(
            item,
            ModuleId(0),
            SourceTreeId::Original,
            self.max_depth,
            |event| {
                match event {
                    SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                    SiteEvent::Node(node) => {
                        let site = sites
                            .get(ordinal)
                            .ok_or_else(|| category("moved source shape differs", item.span))?;
                        if let Some(site) = site {
                            budget.reserve_analysis_work(
                                1 + bound.nodes.len().checked_ilog2().unwrap_or(0) as u64,
                            )?;
                            budget.reserve_temporary_bytes(96)?;
                            bound.bytes += 96;
                            if bound.nodes.insert(key(node.node), *site).is_some() {
                                return Err(category("moved source address repeats", item.span));
                            }
                        }
                        ordinal += 1;
                    }
                }
                Ok(())
            },
        )?;
        if ordinal != sites.len() {
            return Err(category("moved source omits nodes", item.span));
        }
        let bytes = sites.capacity() as u64 * std::mem::size_of::<Option<SourceSite>>() as u64;
        drop(sites);
        budget.release_temporary_bytes(bytes);
        Ok(())
    }
    pub(super) fn discard_item_sites(&mut self, item: &s::Item) -> CompileResult<()> {
        use zil_expand::ct_selection::Visitor;
        Clear {
            nodes: &mut self.bound_source_sites.nodes,
            budget: &mut self.environment.budget,
            max_depth: self.max_depth,
        }
        .item(item, 0)
    }
    pub(super) fn expression_source_site(
        &mut self,
        expression: &s::Expr,
    ) -> CompileResult<Option<SourceSite>> {
        let site = self
            .bound_source_sites
            .nodes
            .get(&key(SourceNode::Expr(expression)))
            .copied();
        self.clear_expression_source_sites(expression)?;
        Ok(site)
    }
    pub(super) fn release_bound_source_sites(&mut self) {
        let bound = std::mem::take(&mut self.bound_source_sites);
        let bytes = bound.bytes;
        drop(bound);
        self.environment.budget.release_temporary_bytes(bytes);
    }
    pub(super) fn clear_expression_source_sites(
        &mut self,
        expression: &s::Expr,
    ) -> CompileResult<()> {
        use zil_expand::ct_selection::Visitor;
        Clear {
            nodes: &mut self.bound_source_sites.nodes,
            budget: &mut self.environment.budget,
            max_depth: self.max_depth,
        }
        .expression(expression, 0)
    }
    pub(super) fn clear_statement_source_sites(
        &mut self,
        statement: &s::Stmt,
    ) -> CompileResult<()> {
        use zil_expand::ct_selection::Visitor;
        Clear {
            nodes: &mut self.bound_source_sites.nodes,
            budget: &mut self.environment.budget,
            max_depth: self.max_depth,
        }
        .statement(statement, 0)
    }
}
