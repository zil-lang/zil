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
enum Frozen {
    Original(Arc<ParseResult>),
    Generated(Arc<zil_expand::source_context::GeneratedSourceTree>),
}
impl Frozen {
    fn module(&self) -> &s::Module {
        match self {
            Self::Original(parsed) => &parsed.module,
            Self::Generated(tree) => &tree.module,
        }
    }
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
    pub(super) fn bind_item_source_sites(
        &mut self,
        owner: &CtSymbolId,
        item: &s::Item,
    ) -> CompileResult<()> {
        let (frozen, tree_id, row, local) = if let Some(fingerprint) = owner.generation_path {
            self.environment.budget.reserve_analysis_work(
                self.environment
                    .generated_catalog
                    .trees
                    .iter()
                    .fold(1u64, |work, tree| {
                        work.saturating_add(tree.identity_path_len() as u64 + 1)
                    }),
            )?;
            let (row, tree) = self
                .environment
                .generated_catalog
                .trees
                .iter()
                .enumerate()
                .find(|(_, tree)| {
                    tree.module.id == owner.module
                        && tree.generation_fingerprint() == Some(fingerprint)
                })
                .ok_or_else(|| category("clone source generated tree absent", item.span))?;
            self.environment
                .budget
                .reserve_analysis_work(tree.declarations.len() as u64)?;
            let local = tree
                .declarations
                .iter()
                .position(|id| *id == owner.declaration)
                .ok_or_else(|| category("clone source generated declaration absent", item.span))?;
            (
                Frozen::Generated(tree.clone()),
                SourceTreeId::Generated(row as u32),
                Some(row as u32),
                zil_base::NodeId(local as u32),
            )
        } else {
            self.environment
                .budget
                .reserve_analysis_work(self.parsed.len() as u64 + 1)?;
            let parsed = self
                .parsed
                .iter()
                .find(|parsed| parsed.module.id == owner.module)
                .ok_or_else(|| category("clone source original module absent", item.span))?;
            (
                Frozen::Original(parsed.clone()),
                SourceTreeId::Original,
                None,
                owner.declaration,
            )
        };
        let module = frozen.module();
        self.environment.budget.reserve_analysis_work(
            1 + self.indexed_source_trees.len().checked_ilog2().unwrap_or(0) as u64,
        )?;
        if !self.indexed_source_trees.contains(&(module.id, row)) {
            let budget = &mut self.environment.budget;
            let inventory = &mut self.canonical_source_sites;
            compiler::source_sites::observe(module, tree_id, self.max_depth, |event| {
                match event {
                    SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                    SiteEvent::Node(node) => {
                        budget.reserve_analysis_work(
                            1 + inventory.len().checked_ilog2().unwrap_or(0) as u64,
                        )?;
                        budget.reserve_temporary_bytes(96)?;
                        if inventory.insert(key(node.node), node.site).is_some() {
                            return Err(category(
                                "frozen source node inventory repeats",
                                item.span,
                            ));
                        }
                    }
                }
                Ok(())
            })?;
            self.environment.budget.reserve_temporary_bytes(64)?;
            self.indexed_source_trees.insert((module.id, row));
        }
        let source =
            compiler::source_scope::original_declaration(module, local, self.max_depth, |work| {
                self.environment.budget.reserve_analysis_work(work as u64)
            })?
            .ok_or_else(|| category("clone source is not an actual declaration", item.span))?;
        let budget = &mut self.environment.budget;
        let mut source_count = 0usize;
        compiler::source_sites::observe_item(
            source.item,
            module.id,
            tree_id,
            self.max_depth,
            |event| {
                match event {
                    SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                    SiteEvent::Node(_) => source_count += 1,
                }
                Ok(())
            },
        )?;
        let temporary = source_count as u64 * std::mem::size_of::<(bool, SourceSite)>() as u64;
        budget.reserve_temporary_bytes(temporary)?;
        let mut canonical = Vec::with_capacity(source_count);
        let inventory = &self.canonical_source_sites;
        compiler::source_sites::observe_item(
            source.item,
            module.id,
            tree_id,
            self.max_depth,
            |event| {
                match event {
                    SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                    SiteEvent::Node(node) => {
                        budget.reserve_analysis_work(
                            1 + inventory.len().checked_ilog2().unwrap_or(0) as u64,
                        )?;
                        canonical.push((
                            matches!(node.node, SourceNode::Type(_)),
                            *inventory.get(&key(node.node)).ok_or_else(|| {
                                category("frozen source canonical site absent", item.span)
                            })?,
                        ));
                    }
                }
                Ok(())
            },
        )?;
        let mut ordinal = 0;
        let mut bound = BoundSites::default();
        compiler::source_sites::observe_item(item, module.id, tree_id, self.max_depth, |event| {
            match event {
                SiteEvent::Work(work) => budget.reserve_analysis_work(work as u64)?,
                SiteEvent::Node(node) => {
                    let Some((tag, site)) = canonical.get(ordinal) else {
                        return Err(category("clone has extra source nodes", item.span));
                    };
                    if *tag != matches!(node.node, SourceNode::Type(_)) {
                        return Err(category("clone source node kind/order differs", item.span));
                    }
                    budget.reserve_analysis_work(
                        1 + bound.nodes.len().checked_ilog2().unwrap_or(0) as u64,
                    )?;
                    budget.reserve_temporary_bytes(96)?;
                    bound.bytes += 96;
                    bound.nodes.insert(key(node.node), *site);
                    ordinal += 1;
                }
            }
            Ok(())
        })?;
        budget.release_temporary_bytes(temporary);
        if ordinal != canonical.len() {
            return Err(category("clone omits frozen source nodes", item.span));
        }
        self.bound_source_sites = bound;
        Ok(())
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
        self.environment.budget.release_temporary_bytes(bound.bytes);
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
