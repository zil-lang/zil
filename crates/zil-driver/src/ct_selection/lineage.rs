//! Track the declarations emitted by the actual selection pass.
use super::*;
use std::sync::Arc;
use zil_expand::source_context::{
    ClosureSiteLineage, DeclarationOrigin, GeneratedDeclarationOrigin, GeneratedSourceTree,
    ModuleDeclarationOrigins, SourceSelectionStep, source_selection_site,
};

pub(super) enum ItemList<'a> {
    Module,
    If(&'a Occurrence, bool),
    For(&'a Occurrence, u32),
    Members,
}

pub(super) enum Occurrence {
    Original(u32),
    Selection {
        root: u32,
        steps: Vec<SourceSelectionStep>,
    },
    Member,
}

pub(super) struct Placement {
    pub occurrence: Occurrence,
    pub body_changed: bool,
}

fn limit(span: Span) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "selection declaration identity budget exhausted",
        Some(span),
    )]
}

impl ItemList<'_> {
    pub(super) fn child(
        &self,
        item: usize,
        budget: &mut Budget,
        span: Span,
    ) -> CompileResult<Occurrence> {
        let item = u32::try_from(item).map_err(|_| limit(span))?;
        let (parent, step) = match *self {
            Self::Module => return Ok(Occurrence::Original(item)),
            Self::Members => return Ok(Occurrence::Member),
            Self::If(parent, then_arm) => (parent, SourceSelectionStep::If { then_arm, item }),
            Self::For(parent, iteration) => (parent, SourceSelectionStep::For { iteration, item }),
        };
        let (root, prior) = match parent {
            Occurrence::Original(root) => (*root, &[][..]),
            Occurrence::Selection { root, steps } => (*root, steps.as_slice()),
            Occurrence::Member => return Ok(Occurrence::Member),
        };
        let len = prior.len().saturating_add(1);
        budget.spend(0, len, span)?;
        budget.bytes(
            len.saturating_mul(std::mem::size_of::<SourceSelectionStep>()),
            span,
        )?;
        let mut steps = Vec::with_capacity(len);
        steps.extend_from_slice(prior);
        steps.push(step);
        Ok(Occurrence::Selection { root, steps })
    }
}

pub(super) fn bind(
    module: &s::Module,
    origins: &mut ModuleDeclarationOrigins,
    placements: Vec<Placement>,
    budget: &mut Budget,
) -> CompileResult<()> {
    if module.id != origins.module || module.items.len() != placements.len() {
        return Err(vec![Diagnostic::error(
            Phase::Expand,
            "E-SOURCE-IDENTITY",
            Some(module.span),
            "selected declarations and actual output positions disagree",
        )]);
    }
    let original = std::mem::take(&mut origins.items);
    let mut next = None;
    for (item, placement) in module.items.iter().zip(placements) {
        let span = item.span;
        budget.spend(1, 1, span)?;
        budget.bytes(std::mem::size_of::<DeclarationOrigin>(), span)?;
        let row = match placement.occurrence {
            Occurrence::Original(root) => {
                let row = &original[root as usize];
                pay_identity(&row.identity, budget, span)?;
                let mut row = row.clone();
                if placement.body_changed {
                    row.closure_sites = ClosureSiteLineage::Unavailable;
                }
                row
            }
            Occurrence::Selection { root, steps } => {
                let end = match next {
                    Some(end) => end,
                    None => zil_expand::source_declarations::source_declaration_count(
                        &origins.original,
                        &mut budget.remaining,
                    )?,
                };
                let cost = syntax::item_cost(item, &budget.remaining)?;
                budget.copy(cost, span)?;
                budget.bytes(
                    cost.nodes
                        .saturating_mul(
                            std::mem::size_of::<s::Item>()
                                + std::mem::size_of::<s::Stmt>()
                                + std::mem::size_of::<s::Expr>()
                                + std::mem::size_of::<s::TypeRef>()
                                + std::mem::size_of::<s::Pattern>(),
                        )
                        .saturating_add(cost.work),
                    span,
                )?;
                let frozen = s::Module {
                    id: module.id,
                    span: module.span,
                    items: vec![item.clone()],
                };
                let count = zil_expand::source_declarations::source_declaration_count(
                    &frozen,
                    &mut budget.remaining,
                )?;
                let after = end.checked_add(count).ok_or_else(|| limit(span))?;
                next = Some(after);
                budget.spend(
                    count as usize,
                    steps.len().saturating_add(count as usize),
                    span,
                )?;
                budget.bytes(
                    (count as usize)
                        .saturating_mul(std::mem::size_of::<NodeId>())
                        .saturating_add(steps.len().saturating_mul(32))
                        .saturating_add(std::mem::size_of::<GeneratedSourceTree>()),
                    span,
                )?;
                let (identity, emitter) = source_selection_site(module.id, root, &steps);
                let tree = Arc::new(GeneratedSourceTree {
                    identity,
                    emitter,
                    module: Arc::new(frozen),
                    declarations: (end..after).map(NodeId).collect(),
                });
                let source = &original[root as usize];
                pay_identity(&source.identity, budget, span)?;
                let identity = tree
                    .root_identity(0, &source.identity)
                    .ok_or_else(|| limit(span))?;
                origins.generated.trees.push(tree.clone());
                DeclarationOrigin {
                    declaration: NodeId(end),
                    identity,
                    closure_sites: ClosureSiteLineage::Original,
                    retained_members: None,
                    generated: Some(GeneratedDeclarationOrigin { tree, root: 0 }),
                }
            }
            Occurrence::Member => unreachable!("module occurrence is not a member"),
        };
        origins.items.push(row);
    }
    Ok(())
}

fn pay_identity(
    identity: &LifeDeclarationIdentity,
    budget: &mut Budget,
    span: Span,
) -> CompileResult<()> {
    let mut bytes = identity.package.len();
    for part in identity
        .module_path
        .iter()
        .chain(&identity.declaration_path)
    {
        bytes = bytes.saturating_add(part.len().saturating_add(std::mem::size_of::<String>()));
    }
    budget.spend(0, bytes, span)?;
    budget.bytes(bytes, span)
}
