//! Catalog lookup is structural addressing only. These immutable trees enter
//! from actual verified ItemList emissions and the separate expansion ledger.
use super::*;
use zil_expand::source_context::{GeneratedSourceCatalog, SourceTreeIdentity};

pub(super) fn root_identity(
    catalog: &GeneratedSourceCatalog,
    row: usize,
    top: usize,
    origins: &[ModuleDeclarationOrigins],
    budget: &mut CtExecutionBudget,
) -> CompileResult<LifeDeclarationIdentity> {
    budget.reserve_analysis_work(origins.len() as u64 + 1)?;
    let tree = catalog
        .trees
        .get(row)
        .ok_or_else(|| unsupported("CT generated source identity has no frozen tree"))?;
    let SourceTreeIdentity::Generated { module, event_path } = &tree.identity else {
        return Err(unsupported(
            "CT generated source identity is in the original domain",
        ));
    };
    let namespace = &origins
        .iter()
        .find(|m| m.module == *module)
        .and_then(|m| m.items.first())
        .ok_or_else(|| unsupported("CT generated source has no original module namespace"))?
        .identity;
    let namespace_cost = identity_cost(namespace, budget)?;
    budget.reserve_analysis_work(event_path.len() as u64 * 4 + 32)?;
    budget.reserve_temporary_bytes(
        namespace_cost
            .saturating_add(event_path.len() as u64 * 4)
            .saturating_add(2 * std::mem::size_of::<String>() as u64 + 32),
    )?;
    tree.root_identity(top as u32, namespace)
        .ok_or_else(|| unsupported("CT generated source has no actual root identity"))
}

impl FrozenInvocationContext {
    pub(super) fn generated_symbol_key(
        &self,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Vec<u8>> {
        let catalog = self.generated_catalog.as_ref().ok_or_else(|| {
            unsupported("CT generated semantic symbol has no actual frozen catalog")
        })?;
        budget.reserve_analysis_work(catalog.trees.len() as u64)?;
        let mut selected = None;
        for (row, tree) in catalog.trees.iter().enumerate() {
            let work = match &tree.identity {
                SourceTreeIdentity::Generated { event_path, .. } => {
                    event_path.len() as u64 * 4 + 32
                }
                _ => 1,
            };
            budget.reserve_analysis_work(work + tree.declarations.len() as u64)?;
            if tree.identity.module() != symbol.module
                || tree.generation_fingerprint() != symbol.generation_path
            {
                continue;
            }
            for (local, output) in tree.declarations.iter().enumerate() {
                if *output == symbol.declaration {
                    if selected.replace((row, tree, local)).is_some() {
                        return Err(unsupported(
                            "CT generated semantic declaration identity is ambiguous",
                        ));
                    }
                }
            }
        }
        let (row, tree, local) = selected.ok_or_else(|| {
            unsupported("CT generated semantic declaration has no actual frozen placement")
        })?;
        let mut top = local;
        let mut member = Vec::new();
        if local >= tree.module.items.len() {
            let mut next = u32::try_from(tree.module.items.len())
                .map_err(|_| unsupported("CT generated declaration local index overflow"))?;
            let mut found = None;
            for (ordinal, item) in tree.module.items.iter().enumerate() {
                if let Some(path) = super::member_path(item, &mut next, local as u32, budget, 0)? {
                    found = Some((ordinal, path));
                    break;
                }
            }
            (top, member) = found.ok_or_else(|| {
                unsupported("CT generated semantic member is not in the actual frozen traversal")
            })?;
        }
        let origins = self.origins.as_ref().ok_or_else(|| {
            unsupported("CT generated semantic namespace has no original lineage")
        })?;
        let identity = root_identity(catalog, row, top, origins, budget)?;
        identity_cost(&identity, budget)?;
        let size = identity
            .package
            .len()
            .saturating_add(40)
            .saturating_add(
                identity
                    .module_path
                    .iter()
                    .chain(&identity.declaration_path)
                    .fold(0usize, |n, s| n.saturating_add(s.len()).saturating_add(8)),
            )
            .saturating_add(identity.generation_path.len().saturating_mul(8))
            .saturating_add(member.len().saturating_mul(8));
        budget.reserve_analysis_work(size as u64)?;
        budget.reserve_temporary_bytes(size as u64)?;
        let mut out = Vec::with_capacity(size);
        fn number(out: &mut Vec<u8>, n: u64) {
            out.extend_from_slice(&n.to_be_bytes());
        }
        fn text(out: &mut Vec<u8>, s: &str) {
            number(out, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        text(&mut out, &identity.package);
        number(&mut out, identity.module_path.len() as u64);
        for part in &identity.module_path {
            text(&mut out, part);
        }
        number(&mut out, identity.declaration_path.len() as u64);
        for part in &identity.declaration_path {
            text(&mut out, part);
        }
        number(&mut out, identity.generation_path.len() as u64);
        for part in &identity.generation_path {
            number(&mut out, u64::from(*part));
        }
        number(&mut out, member.len() as u64);
        for part in member {
            number(&mut out, u64::from(part));
        }
        Ok(out)
    }
}

/// Independently select every emitter by BOTH its declaration inventory and
/// its explicit structural directory path. A catalog identity alone is never
/// enough, and unsupported selection/unrolling paths cannot gain authority.
pub(super) fn validate_catalog(
    catalog: &GeneratedSourceCatalog,
    parsed: &[Arc<ParseResult>],
    budget: &mut CtExecutionBudget,
) -> CompileResult<()> {
    budget.reserve_analysis_work(catalog.trees.len() as u64)?;
    for (row, tree) in catalog.trees.iter().enumerate() {
        let SourceTreeIdentity::Generated { module, event_path } = &tree.identity else {
            return Err(unsupported(
                "CT generated catalog contains an original tree identity",
            ));
        };
        budget.reserve_analysis_work(
            event_path.len() as u64 + tree.emitter.event_path.len() as u64 + 2,
        )?;
        if tree.module.id != *module
            || tree.emitter.tree.module() != *module
            || tree.emitter.event_path.is_empty()
        {
            return Err(unsupported(
                "CT generated source emitter module or structural path differs",
            ));
        }
        let (emitter_module, emitter_declaration, parent_path) = match &tree.emitter.tree {
            SourceTreeIdentity::Original(module) => {
                budget.reserve_analysis_work(parsed.len() as u64)?;
                let source = parsed
                    .iter()
                    .find(|p| p.module.id == *module)
                    .ok_or_else(|| {
                        unsupported("CT generated emitter has no actual original module")
                    })?;
                (&source.module, tree.emitter.declaration, None)
            }
            SourceTreeIdentity::Generated {
                module,
                event_path: parent_identity,
            } => {
                let mut parent = None;
                for candidate in &catalog.trees[..row] {
                    budget.reserve_analysis_work(1)?;
                    let SourceTreeIdentity::Generated {
                        module: actual,
                        event_path: path,
                    } = &candidate.identity
                    else {
                        continue;
                    };
                    budget
                        .reserve_analysis_work(path.len().min(parent_identity.len()) as u64 + 1)?;
                    if actual == module && path == parent_identity {
                        if parent.replace(candidate).is_some() {
                            return Err(unsupported(
                                "CT generated emitter parent tree identity is ambiguous",
                            ));
                        }
                    }
                }
                let parent = parent.ok_or_else(|| {
                    unsupported(
                        "CT generated emitter parent is absent or not an earlier actual event",
                    )
                })?;
                budget.reserve_analysis_work(parent.declarations.len() as u64)?;
                let mut local = None;
                for (position, global) in parent.declarations.iter().enumerate() {
                    if *global == tree.emitter.declaration {
                        if local.replace(zil_base::NodeId(position as u32)).is_some() {
                            return Err(unsupported(
                                "CT generated emitter declaration placement is ambiguous",
                            ));
                        }
                    }
                }
                (
                    &*parent.module,
                    local.ok_or_else(|| {
                        unsupported(
                            "CT generated emitter has no actual original declaration placement",
                        )
                    })?,
                    Some(parent_identity.as_slice()),
                )
            }
        };
        let expected_len = parent_path
            .map_or(1, |p| p.len() + 1)
            .saturating_add(tree.emitter.event_path.len());
        if event_path.len() != expected_len {
            return Err(unsupported(
                "CT generated event path has a different structural domain",
            ));
        }
        let suffix = if let Some(parent) = parent_path {
            budget.reserve_analysis_work(parent.len() as u64)?;
            if &event_path[..parent.len()] != parent || event_path[parent.len()] != 1 {
                return Err(unsupported(
                    "CT generated event path does not extend its actual emitter tree",
                ));
            }
            &event_path[parent.len() + 1..]
        } else {
            if event_path[0] != 0 {
                return Err(unsupported(
                    "CT original emission event has a generated parent domain",
                ));
            }
            &event_path[1..]
        };
        budget.reserve_analysis_work(suffix.len() as u64)?;
        if suffix != tree.emitter.event_path.as_slice() {
            return Err(unsupported(
                "CT generated event differs from its actual emitter structural path",
            ));
        }
        let by_declaration = super::super::compiler::source_scope::original_declaration(
            emitter_module,
            emitter_declaration,
            budget.remaining().max_call_depth as usize,
            |work| budget.reserve_analysis_work(work as u64),
        )?
        .ok_or_else(|| {
            unsupported("CT generated emitter declaration is absent from its actual frozen tree")
        })?;
        let by_path = zil_expand::source_context::source_item_at_path(
            emitter_module,
            &tree.emitter.event_path,
            budget.remaining().max_call_depth as usize,
            |work| budget.reserve_analysis_work(work as u64),
        )?
        .ok_or_else(|| {
            unsupported(
                "CT generated emitter structural path is absent from its actual frozen tree",
            )
        })?;
        if !std::ptr::eq(by_declaration.item, by_path)
            || !matches!(by_path.kind, s::ItemKind::Meta(s::MetaItem::Emit(_)))
        {
            return Err(unsupported(
                "CT generated source authority does not come from its actual original MetaEmit",
            ));
        }
    }
    Ok(())
}
