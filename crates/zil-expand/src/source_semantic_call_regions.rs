//! Independent original Call occurrences. These are syntax-domain identities,
//! not inferred lifetime images or permission to select a callable premise.
use super::*;
use zil_base::NodeId;

pub(super) struct Observation {
    pub owner: SemanticDefinitionId,
    pub call: NodeId,
    pub selector: Vec<u32>,
}
fn selector(
    rows: &mut Vec<Observation>, owner: SemanticDefinitionId, call: NodeId,
    suffix: &[u32], budget: &mut Budget<'_>, span: Span,
) -> R<()> {
    let recent = rows.iter().rev().take_while(|row| row.owner == owner && row.call == call).count();
    budget.identity_work(recent.saturating_mul(suffix.len() + 1), span)?;
    if rows.iter().rev().take(recent).any(|row| row.selector == suffix) {
        return Ok(());
    }
    budget.lineage_bytes(std::mem::size_of_val(suffix), span)?;
    budget.lineage_push(rows, Observation { owner, call, selector: suffix.to_vec() }, span)
}
pub(super) fn signature(
    rows: &mut Vec<Observation>, owner: SemanticDefinitionId, call: NodeId,
    definition: Option<&SemanticDefinition>, signature: &SemanticSignatureRecipe,
    outer_frames: &[u32], budget: &mut Budget<'_>, span: Span,
) -> R<()> {
    if matches!(&signature.result, SemanticTypeRecipe::Address {
        mutable: false,
        lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence {
            operand: SemanticRecipeOperand::Receiver, path, mode: StoredBorrowMode::SharedAddress,
        }), ..
    } if path.is_empty()) || definition.is_some_and(|row|
        row.kind == SemanticDefinitionKind::Closure && matches!(&signature.result,
            SemanticTypeRecipe::OriginalType { scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Result })) {
        // The original Call itself is the receiver-borrow occurrence. This
        // is only a directory identity; its actual callee and source window
        // must be independently reconstructed by each consumer.
        selector(rows, owner, call, &[], budget, span)?;
    }
    inputs(rows, owner, call, definition.map(|row| row.parameters.as_slice()), signature.parameters.iter().map(|row| &row.ty), budget, span)?;
    for index in 0..signature.invocation_lifetimes {
        budget.node(span, 0)?;
        selector(rows, owner, call, &[u32::MAX, 3, index], budget, span)?;
    }
    let mut depth = 0u32;
    // Only real nonempty quantifier frames occupy de Bruijn depth. The
    // signature's own frame is nearest, followed by its enclosing ForAlls.
    for count in std::iter::once(signature.bound_lifetimes).chain(outer_frames.iter().rev().copied()) {
        budget.node(span, 0)?;
        if count == 0 { continue; }
        for index in 0..count {
            budget.node(span, 0)?;
            selector(rows, owner, call, &[u32::MAX, 4, depth, index], budget, span)?;
        }
        depth = depth.checked_add(1).ok_or_else(|| error(span, "Call quantifier depth overflows"))?;
    }
    Ok(())
}
fn inputs<'a>(
    rows: &mut Vec<Observation>, owner: SemanticDefinitionId, call: NodeId,
    formals: Option<&[SemanticFormal]>, parameters: impl Iterator<Item = &'a SemanticTypeRecipe>,
    budget: &mut Budget<'_>, span: Span,
) -> R<()> {
    // Mixed formal positions retain unused Type/Const entries between Lives.
    if let Some(formals) = formals {
        for (ordinal, formal) in formals.iter().enumerate() {
            budget.node(span, 0)?;
            if matches!(formal, SemanticFormal::Lifetime) {
                let ordinal = u32::try_from(ordinal).map_err(|_| error(span, "Call mixed formal ordinal overflows"))?;
                selector(rows, owner, call, &[u32::MAX, 0, ordinal], budget, span)?;
            }
        }
    }
    for (ordinal, parameter) in parameters.enumerate() {
        budget.node(span, 0)?;
        match parameter {
            SemanticTypeRecipe::Address { lifetime, .. } | SemanticTypeRecipe::Desc { lifetime, .. } => {
                if let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit { scope: SemanticScopeRef::Ancestor(0), ordinal }) = lifetime {
                    selector(rows, owner, call, &[u32::MAX, 1, *ordinal], budget, span)?;
                }
                if matches!(parameter, SemanticTypeRecipe::Desc { .. }) {
                    let ordinal = u32::try_from(ordinal).map_err(|_| error(span, "Call value parameter ordinal overflows"))?;
                    selector(rows, owner, call, &[u32::MAX, 2, ordinal], budget, span)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
pub(super) fn variant(
    rows: &mut Vec<Observation>, owner: SemanticDefinitionId, call: NodeId,
    definition: &SemanticDefinition, fields: &[SemanticTypeRecipe], budget: &mut Budget<'_>, span: Span,
) -> R<()> {
    inputs(rows, owner, call, Some(&definition.parameters), fields.iter(), budget, span)
}
pub(super) fn constructor(
    rows: &mut Vec<Observation>,
    owner: SemanticDefinitionId,
    node: NodeId,
    definition: &SemanticDefinition,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<()> {
    if !matches!(
        definition.body,
        SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { .. },
            ..
        }
    ) {
        return Err(error(
            span,
            "Construct occurrence has no original product declaration",
        ));
    }
    for (ordinal, formal) in definition.parameters.iter().enumerate() {
        budget.node(span, 0)?;
        if matches!(formal, SemanticFormal::Lifetime) {
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| error(span, "Construct mixed formal ordinal overflows"))?;
            selector(rows, owner, node, &[u32::MAX - 1, 0, ordinal], budget, span)?;
        }
    }
    Ok(())
}
pub(super) fn prepare(modules: &mut [(ModuleId, SemanticModule)], budget: &mut Budget<'_>) -> R<()> {
    let mut observations = effects::observe_call_prototypes(modules, budget)?;
    for observation in &mut observations {
        let span = Span::default();
        budget.identity_work(modules.len().saturating_add(modules.iter().find(|(id, _)| *id == observation.owner.module).map_or(0, |(_, module)| module.definitions.len())), span)?;
        let definition = modules.iter_mut().find(|(id, _)| *id == observation.owner.module)
            .and_then(|(_, module)| module.definitions.iter_mut().find(|row| row.id == observation.owner))
            .ok_or_else(|| error(span, "Call occurrence lexical owner is absent"))?;
        let SemanticDefinitionBody::FunctionWithBody { sites, site_nodes, .. } = &definition.body else {
            return Err(error(span, "Call occurrence has no complete original body"));
        };
        budget.identity_work(site_nodes.len().saturating_add(sites.len()), span)?;
        let mut selected = None;
        for node in site_nodes {
            if node.node != SemanticOriginalNode::Expression(observation.call) { continue; }
            for site in sites {
                let matching_kind = if observation.selector.first() == Some(&(u32::MAX - 1)) {
                    matches!(site.kind, SemanticSiteRecipeKind::Construct { .. })
                } else {
                    matches!(site.kind, SemanticSiteRecipeKind::OriginalCall)
                };
                if site.id == node.site && matching_kind {
                    if selected.replace(site).is_some() { return Err(error(span, "Call occurrence has duplicate original sites")); }
                }
            }
        }
        // Structural operations without an OriginalCall site (such as a
        // pipeline's synthetic invocation) are not real Call occurrences.
        let Some(site) = selected else { continue; };
        budget.lineage_bytes((site.occurrence_path.len() + observation.selector.len()).saturating_mul(std::mem::size_of::<u32>()), span)?;
        let mut path = site.occurrence_path.clone();
        path.extend_from_slice(&observation.selector);
        budget.identity_work(definition.implicit_lifetimes.len().saturating_mul(path.len() + 1), span)?;
        if !definition.implicit_lifetimes.iter().any(|row| row.occurrence_path == path) {
            let ordinal = u32::try_from(definition.implicit_lifetimes.len()).map_err(|_| error(span, "Call occurrence namespace overflows"))?;
            budget.lineage_push(&mut definition.implicit_lifetimes, SemanticImplicitLifetime { ordinal, occurrence_path: path }, span)?;
        } else { budget.drop_lineage_vec(path, span)?; }
        budget.drop_lineage_vec(std::mem::take(&mut observation.selector), span)?;
    }
    budget.drop_lineage_vec(observations, Span::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_context::{self, DeclarationPlacement, SourceIdentityContext};
    use std::collections::BTreeMap;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn original_calls_mint_independent_mixed_roles_in_unused_branches() {
        let source = SourceUnit::new(FileId(0), ModuleId(0), "call-regions.zil",
            "fn join<T,life a,life b>(x:&'a T,y:&'b T)->&'a T{return x}fn unused<T,life a,life b>(x:&'a T,y:&'b T){if true{join(x,y);}else{join(y,x);}}");
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source), &[&parsed], SourceIdentityContext::ClosedSnapshot {
                root: source.module_id, aliases: &BTreeMap::new(), declarations: DeclarationPlacement::Original,
            }, &limits).unwrap();
        let mut remaining = limits;
        let (_, program) = program::prepare(&[&parsed], &origins, &[Vec::new()], &mut remaining).unwrap();
        let unused = &program.modules[0].1.definitions[1];
        let SemanticDefinitionBody::FunctionWithBody { sites, .. } = &unused.body else { panic!() };
        let calls = sites.iter().filter(|site| matches!(site.kind, SemanticSiteRecipeKind::OriginalCall)).collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        for call in calls {
            for full_ordinal in [1, 2] {
                let mut expected = call.occurrence_path.clone();
                expected.extend_from_slice(&[u32::MAX, 0, full_ordinal]);
                assert_eq!(unused.implicit_lifetimes.iter().filter(|row| row.occurrence_path == expected).count(), 1);
            }
        }
    }
}
