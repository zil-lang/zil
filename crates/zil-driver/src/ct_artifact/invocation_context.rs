//! Immutable source witnesses for one actual CT invocation. Raw table indices,
//! source names and spans never select a lexical owner.
use super::{compiler::Entry, source_index::SourceIndex};
#[path = "invocation_context_calls.rs"]
mod calls;
#[path = "invocation_context_generated.rs"]
mod generated;
#[path = "invocation_context_relocations.rs"]
mod relocations;
#[path = "invocation_context_runtime.rs"]
mod runtime;
#[path = "invocation_context_source_types.rs"]
mod source_types;
use std::sync::Arc;
use zil_base::{CompileResult, Diagnostic, Phase};
use zil_ct::CtExecutionBudget;
use zil_expand::source_context::ModuleDeclarationOrigins;
use zil_ir::{LifeDeclarationIdentity, ct_artifact::*};
use zil_syntax::{ParseResult, ast as s};
use zil_verify::CtScopeParameterKind;

fn unsupported(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Ct, message, None)]
}
pub(super) fn identity_cost(
    identity: &LifeDeclarationIdentity,
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    budget.reserve_analysis_work(
        identity
            .module_path
            .len()
            .saturating_add(identity.declaration_path.len())
            .saturating_add(identity.generation_path.len()) as u64,
    )?;
    let mut bytes = (identity
        .module_path
        .len()
        .saturating_add(identity.declaration_path.len()) as u64)
        .saturating_mul(std::mem::size_of::<String>() as u64)
        .saturating_add(identity.generation_path.len() as u64 * std::mem::size_of::<u32>() as u64);
    for text in std::iter::once(&identity.package)
        .chain(&identity.module_path)
        .chain(&identity.declaration_path)
    {
        budget.reserve_analysis_work(text.len() as u64)?;
        bytes = bytes.saturating_add(text.len() as u64);
    }
    Ok(bytes)
}
pub(super) fn origins_cost(
    origins: &[ModuleDeclarationOrigins],
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    budget.reserve_analysis_work(origins.len() as u64)?;
    let mut bytes = (std::mem::size_of::<Vec<ModuleDeclarationOrigins>>()
        + 2 * std::mem::size_of::<usize>()) as u64
        + (origins.len() as u64) * std::mem::size_of::<ModuleDeclarationOrigins>() as u64;
    for module in origins {
        budget.reserve_analysis_work(module.items.len() as u64)?;
        bytes = bytes.saturating_add(
            module.items.len() as u64
                * std::mem::size_of::<zil_expand::source_context::DeclarationOrigin>() as u64,
        );
        for item in &module.items {
            bytes = bytes.saturating_add(identity_cost(&item.identity, budget)?);
            if let Some(members) = &item.retained_members {
                budget.reserve_analysis_work(members.positions.len() as u64 + 1)?;
                bytes = bytes.saturating_add(
                    members.positions.len() as u64 * std::mem::size_of::<u32>() as u64,
                );
            }
        }
    }
    Ok(bytes)
}
#[derive(Clone)]
struct ActiveScope {
    raw: CtAmbientLifeScope,
    identity: LifeDeclarationIdentity,
    parameters: Vec<CtScopeParameterKind>,
}
pub(super) struct FrozenInvocationContext {
    active: Vec<ActiveScope>,
    pub bytes: u64,
    origins: Option<Arc<Vec<ModuleDeclarationOrigins>>>,
    modules: Vec<Arc<ParseResult>>,
    authenticated_types: Vec<source_types::AuthenticatedTypeSite>,
    authenticated_schemas: Vec<source_types::AuthenticatedSchemaSite>,
    runtime_scope_id: Option<CtAmbientScopeId>,
    runtime_root_active: Vec<usize>,
    entry_site: Option<super::compiler::source_sites::SourceSite>,
    entry_definition: CtSymbolId,
    generated_catalog: Option<zil_expand::source_context::GeneratedSourceCatalog>,
}
impl FrozenInvocationContext {
    pub(super) fn freeze(
        entry: &Entry<'_>,
        parsed: &[Arc<ParseResult>],
        index: &SourceIndex,
        origins: Option<&Arc<Vec<ModuleDeclarationOrigins>>>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<Arc<Self>>> {
        Self::freeze_at_source(entry, parsed, index, origins, None, None, budget)
    }
    pub(super) fn freeze_with_catalog(
        entry: &Entry<'_>,
        parsed: &[Arc<ParseResult>],
        index: &SourceIndex,
        origins: Option<&Arc<Vec<ModuleDeclarationOrigins>>>,
        catalog: Option<&zil_expand::source_context::GeneratedSourceCatalog>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<Arc<Self>>> {
        Self::freeze_at_source(entry, parsed, index, origins, catalog, None, budget)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn freeze_at_source(
        entry: &Entry<'_>,
        parsed: &[Arc<ParseResult>],
        index: &SourceIndex,
        origins: Option<&Arc<Vec<ModuleDeclarationOrigins>>>,
        catalog: Option<&zil_expand::source_context::GeneratedSourceCatalog>,
        source_site: Option<super::compiler::source_sites::SourceSite>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<Arc<Self>>> {
        Self::freeze_at_source_force(
            entry,
            parsed,
            index,
            origins,
            catalog,
            source_site,
            false,
            budget,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn freeze_at_source_force(
        entry: &Entry<'_>,
        parsed: &[Arc<ParseResult>],
        index: &SourceIndex,
        origins: Option<&Arc<Vec<ModuleDeclarationOrigins>>>,
        catalog: Option<&zil_expand::source_context::GeneratedSourceCatalog>,
        source_site: Option<super::compiler::source_sites::SourceSite>,
        force_complete_scope: bool,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<Arc<Self>>> {
        let before = budget.live_temporary_bytes();
        let result = Self::freeze_inner(
            entry,
            parsed,
            index,
            origins,
            catalog,
            source_site,
            force_complete_scope,
            budget,
        );
        let keep = result
            .as_ref()
            .ok()
            .and_then(|context| context.as_ref())
            .map_or(0, |context| context.bytes);
        let added = budget.live_temporary_bytes().saturating_sub(before);
        budget.release_temporary_bytes(added.saturating_sub(keep));
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn freeze_inner(
        entry: &Entry<'_>,
        parsed: &[Arc<ParseResult>],
        index: &SourceIndex,
        origins: Option<&Arc<Vec<ModuleDeclarationOrigins>>>,
        catalog: Option<&zil_expand::source_context::GeneratedSourceCatalog>,
        source_site: Option<super::compiler::source_sites::SourceSite>,
        force_complete_scope: bool,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<Arc<Self>>> {
        let definition = match entry {
            Entry::Expression {
                lexical_owner: Some(owner),
                ..
            }
            | Entry::Function(owner) => owner,
            Entry::Expression {
                lexical_owner: None,
                ..
            } => return Ok(None),
        };
        budget.reserve_analysis_work(parsed.len() as u64 + index.symbol_lookup_work())?;
        let module = parsed
            .iter()
            .find(|module| module.module.id == definition.module)
            .ok_or_else(|| unsupported("CT invocation has no actual original module"))?;
        let max_depth = budget.remaining().max_call_depth as usize;
        let (scope_module, original, source_tree, generated_row) =
            if definition.generation_path.is_some() {
                let catalog = catalog.ok_or_else(|| {
                    unsupported("generated CT invocation lacks an authentic frozen source catalog")
                })?;
                let (row, original) = super::compiler::source_scope::generated_original_function(
                    catalog,
                    definition,
                    max_depth,
                    |work| budget.reserve_analysis_work(work as u64),
                )?
                .ok_or_else(|| {
                    unsupported("generated CT invocation lacks its actual frozen function")
                })?;
                (
                    &*catalog.trees[row].module,
                    original,
                    super::compiler::source_sites::SourceTreeId::Generated(row as u32),
                    Some(row),
                )
            } else {
                if !index.contains(definition) {
                    return Err(unsupported(
                        "CT invocation lacks its actual original declaration",
                    ));
                }
                let Some(original) = super::compiler::source_scope::original_function(
                    &module.module,
                    definition.declaration,
                    max_depth,
                    |work| budget.reserve_analysis_work(work as u64),
                )?
                else {
                    return Ok(None);
                };
                (
                    &module.module,
                    original,
                    super::compiler::source_sites::SourceTreeId::Original,
                    None,
                )
            };
        if original.generic_host {
            return Err(unsupported(
                "CT member scope requires its actual enclosing generic context",
            ));
        }
        let function = original.function;
        budget.reserve_analysis_work(function.generics.len() as u64)?;
        if !force_complete_scope
            && !function
                .generics
                .iter()
                .any(|parameter| matches!(parameter.kind, s::GenericParamKind::Lifetime))
        {
            return Ok(None);
        }
        if let Some(catalog) = catalog {
            generated::validate_catalog(catalog, parsed, budget)?;
        }
        let origins = origins
            .ok_or_else(|| unsupported("CT ambient scope requires original declaration lineage"))?;
        budget.reserve_analysis_work(origins.len() as u64)?;
        let generated_identity;
        let identity = if let Some(row) = generated_row {
            generated_identity =
                generated::root_identity(catalog.unwrap(), row, original.top, origins, budget)?;
            &generated_identity
        } else {
            let origin = origins
                .iter()
                .find(|row| row.module == definition.module)
                .and_then(|row| row.items.get(original.top))
                .ok_or_else(|| unsupported("CT ambient owner is absent from original lineage"))?;
            if origin.declaration.0 as usize != original.top {
                return Err(unsupported(
                    "CT ambient original declaration ordinal differs",
                ));
            }
            &origin.identity
        };
        let catalog_bytes = if let Some(catalog) = catalog {
            budget.reserve_analysis_work(catalog.trees.len() as u64)?;
            catalog.trees.len() as u64
                * std::mem::size_of::<Arc<zil_expand::source_context::GeneratedSourceTree>>() as u64
        } else {
            0
        };
        let bytes = catalog_bytes
            + (std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>()) as u64
            + (parsed.len() as u64) * std::mem::size_of::<Arc<ParseResult>>() as u64
            + identity_cost(identity, budget)?
            + std::mem::size_of::<ActiveScope>() as u64
            + (function.generics.len() as u64) * std::mem::size_of::<CtScopeParameterKind>() as u64;
        budget.reserve_temporary_bytes(bytes)?;
        let mut parameters = Vec::with_capacity(function.generics.len());
        for parameter in &function.generics {
            parameters.push(match &parameter.kind {
                s::GenericParamKind::Type(_) => CtScopeParameterKind::Type,
                s::GenericParamKind::Lifetime => CtScopeParameterKind::Lifetime,
                s::GenericParamKind::Const(ty) => {
                    let s::TypeKind::Named { path, args } = &ty.kind else {
                        return Err(unsupported(
                            "CT ambient const domain requires resolved scalar lineage",
                        ));
                    };
                    budget.reserve_analysis_work(path.len() as u64 + args.len() as u64)?;
                    let [name] = path.as_slice() else {
                        return Err(unsupported(
                            "CT ambient const domain requires resolved scalar lineage",
                        ));
                    };
                    budget.reserve_analysis_work(name.len() as u64)?;
                    let scalar = super::compiler::scalar_type(name)
                        .ok_or_else(|| unsupported("CT ambient const domain is not a scalar"))?;
                    if !args.is_empty() {
                        return Err(unsupported("CT ambient scalar domain has arguments"));
                    }
                    CtScopeParameterKind::Scalar(scalar)
                }
            });
        }
        let entry_site = match entry {
            Entry::Expression { expression, .. } => Some(
                super::compiler::source_sites::select_in_tree(
                    scope_module,
                    source_tree,
                    match source_site {
                        Some(site) => super::compiler::source_sites::Selection::Site(site),
                        None => super::compiler::source_sites::Selection::Expr(
                            *expression as *const s::Expr as usize,
                        ),
                    },
                    max_depth,
                    |work| budget.reserve_analysis_work(work as u64),
                )?
                .filter(|node| node.function.is_some_and(|f| std::ptr::eq(f, function)))
                .ok_or_else(|| {
                    unsupported(
                        "CT invocation expression is not the actual frozen original source root",
                    )
                })?
                .site,
            ),
            Entry::Function(_) => None,
        };
        Ok(Some(Arc::new(Self {
            active: vec![ActiveScope {
                raw: CtAmbientLifeScope {
                    declaration: definition.clone(),
                    enclosing: None,
                    binder_site: Vec::new(),
                    instance: CtScopeInstance::Definition,
                },
                identity: identity.clone(),
                parameters,
            }],
            bytes,
            origins: Some(origins.clone()),
            modules: parsed.to_vec(),
            authenticated_types: Vec::new(),
            authenticated_schemas: Vec::new(),
            runtime_scope_id: None,
            runtime_root_active: Vec::new(),
            entry_site,
            entry_definition: definition.clone(),
            generated_catalog: catalog.cloned(),
        })))
    }
    pub(super) fn active_scope_count(&self) -> usize {
        self.active.len()
    }
    pub(super) fn has_scopes(&self) -> bool {
        !self.active.is_empty()
    }
    pub(super) fn symbol_key(
        &self,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Vec<u8>> {
        let before = budget.live_temporary_bytes();
        let result = self.symbol_key_inner(symbol, budget);
        let keep = result.as_ref().map_or(0, |key| key.capacity() as u64);
        let added = budget.live_temporary_bytes().saturating_sub(before);
        budget.release_temporary_bytes(added.saturating_sub(keep));
        result
    }
    fn symbol_key_inner(
        &self,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Vec<u8>> {
        if symbol.generation_path.is_some() {
            return self.generated_symbol_key(symbol, budget);
        }
        budget.reserve_analysis_work(self.modules.len() as u64)?;
        let module = self
            .modules
            .iter()
            .find(|module| module.module.id == symbol.module)
            .ok_or_else(|| unsupported("CT semantic symbol has no actual source module"))?;
        let origins = self
            .origins
            .as_ref()
            .ok_or_else(|| unsupported("CT semantic symbol has no original declaration ledger"))?;
        budget.reserve_analysis_work(origins.len() as u64)?;
        let rows = origins
            .iter()
            .find(|row| row.module == symbol.module)
            .ok_or_else(|| unsupported("CT semantic module has no original ledger"))?;
        let mut top = symbol.declaration.0 as usize;
        let mut member = Vec::new();
        if top >= module.module.items.len() {
            let mut next = u32::try_from(module.module.items.len())
                .map_err(|_| unsupported("CT source member index overflow"))?;
            let mut found = None;
            for (ordinal, item) in module.module.items.iter().enumerate() {
                if let Some(path) = member_path(item, &mut next, symbol.declaration.0, budget, 0)? {
                    found = Some((ordinal, path));
                    break;
                }
            }
            let row = found.ok_or_else(|| {
                unsupported("CT semantic member is not in the actual original traversal")
            })?;
            top = row.0;
            member = row.1;
        }
        let identity = &rows
            .items
            .get(top)
            .ok_or_else(|| unsupported("CT semantic declaration has no original lineage"))?
            .identity;
        identity_cost(identity, budget)?;
        let size = identity
            .package
            .len()
            .saturating_add(8)
            .saturating_add(32)
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
        let member_bytes = member.capacity() as u64 * std::mem::size_of::<u32>() as u64;
        for part in member {
            number(&mut out, u64::from(part));
        }
        budget.release_temporary_bytes(member_bytes);
        debug_assert!(out.capacity() <= size);
        Ok(out)
    }
    fn charge_instance_lookup(
        &self,
        scope: &CtAmbientLifeScope,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        if !matches!(scope.instance, CtScopeInstance::Instantiated(_)) {
            return Ok(());
        }
        let query =
            super::execute::value_context::ambient_cost(std::slice::from_ref(scope), budget)?;
        budget.reserve_analysis_work(self.active.len() as u64 * 32)?;
        for row in &self.active {
            if row.raw.declaration == scope.declaration
                && matches!(row.raw.instance, CtScopeInstance::Instantiated(_))
            {
                let prior = super::execute::value_context::ambient_cost(
                    std::slice::from_ref(&row.raw),
                    budget,
                )?;
                budget.reserve_analysis_work(query.saturating_add(prior))?;
            }
        }
        Ok(())
    }
    pub(super) fn parameter_count_paid(
        &self,
        scope: &CtAmbientLifeScope,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<usize>> {
        self.charge_instance_lookup(scope, budget)?;
        Ok(self.parameter_count(scope))
    }
    pub(super) fn parameter_kind_paid(
        &self,
        scope: &CtAmbientLifeScope,
        ordinal: usize,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<CtScopeParameterKind>> {
        self.charge_instance_lookup(scope, budget)?;
        Ok(self.parameter_kind(scope, ordinal))
    }
    pub(super) fn validate_scope(
        &self,
        scope: &CtAmbientLifeScope,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(
            (self.active.len() as u64)
                .saturating_mul(32)
                .saturating_add(scope.binder_site.len() as u64),
        )?;
        self.charge_instance_lookup(scope, budget)?;
        if self.identity(scope).is_none() {
            return Err(unsupported(
                "CT ambient scope is not this actual invocation",
            ));
        }
        Ok(())
    }
    pub(super) fn validate_reference(
        &self,
        scopes: &[CtAmbientLifeScope],
        reference: CtAmbientLifeReference,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        let scope = scopes
            .get(reference.scope.0 as usize)
            .ok_or_else(|| unsupported("CT ambient scope reference has no retained inventory"))?;
        self.validate_scope(scope, budget)?;
        if self.parameter_kind_paid(scope, reference.parameter as usize, budget)?
            != Some(CtScopeParameterKind::Lifetime)
        {
            return Err(unsupported(
                "CT ambient reference is not an actual lifetime ordinal",
            ));
        }
        Ok(())
    }
    pub(super) fn resolve_reference<'a>(
        &self,
        scopes: &'a [CtAmbientLifeScope],
        mut reference: CtAmbientLifeReference,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<(&'a CtSymbolId, u32)> {
        let mut visited = Vec::new();
        let mut bytes = 0u64;
        let result = (|| {
            loop {
                self.validate_reference(scopes, reference, budget)?;
                budget.reserve_analysis_work(visited.len() as u64 + 1)?;
                if visited.contains(&reference) {
                    return Err(unsupported("CT actual life reference cycle"));
                }
                if visited.len() >= budget.remaining().max_call_depth as usize {
                    return Err(vec![Diagnostic::limit(
                        Phase::Ct,
                        "CT actual life reference depth exceeded",
                        None,
                    )]);
                }
                let scope = &scopes[reference.scope.0 as usize];
                match &scope.instance {
                    CtScopeInstance::Definition => {
                        return Ok((&scope.declaration, reference.parameter));
                    }
                    CtScopeInstance::Instantiated(actuals) => {
                        let Some(CtScopeActual::Lifetime(next)) =
                            actuals.get(reference.parameter as usize)
                        else {
                            return Err(unsupported(
                                "CT actual life reference selects another parameter kind",
                            ));
                        };
                        budget.reserve_temporary_bytes(
                            std::mem::size_of::<CtAmbientLifeReference>() as u64,
                        )?;
                        bytes += std::mem::size_of::<CtAmbientLifeReference>() as u64;
                        visited.reserve_exact(1);
                        visited.push(reference);
                        reference = *next;
                    }
                }
            }
        })();
        budget.release_temporary_bytes(bytes);
        result
    }
    pub(super) fn parameter_count(&self, scope: &CtAmbientLifeScope) -> Option<usize> {
        self.active
            .iter()
            .find(|row| &row.raw == scope)
            .map(|row| row.parameters.len())
    }
    pub(super) fn parameter_kind(
        &self,
        scope: &CtAmbientLifeScope,
        ordinal: usize,
    ) -> Option<CtScopeParameterKind> {
        self.active
            .iter()
            .find(|row| &row.raw == scope)?
            .parameters
            .get(ordinal)
            .copied()
    }
    pub(super) fn identity(&self, scope: &CtAmbientLifeScope) -> Option<&LifeDeclarationIdentity> {
        self.active
            .iter()
            .find(|row| &row.raw == scope)
            .map(|row| &row.identity)
    }
}

fn member_path(
    item: &s::Item,
    next: &mut u32,
    target: u32,
    budget: &mut CtExecutionBudget,
    depth: usize,
) -> CompileResult<Option<Vec<u32>>> {
    budget.reserve_analysis_work(1)?;
    if depth > budget.remaining().max_call_depth as usize {
        return Err(vec![Diagnostic::limit(
            Phase::Ct,
            "CT semantic member depth exceeded",
            None,
        )]);
    }
    fn row(next: &mut u32, target: u32, budget: &mut CtExecutionBudget) -> CompileResult<bool> {
        budget.reserve_analysis_work(1)?;
        let matched = *next == target;
        *next = next
            .checked_add(1)
            .ok_or_else(|| unsupported("CT member identity overflow"))?;
        Ok(matched)
    }
    fn prefix(
        kind: u32,
        ordinal: usize,
        tail: Vec<u32>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Vec<u32>> {
        let ordinal =
            u32::try_from(ordinal).map_err(|_| unsupported("CT member ordinal overflow"))?;
        let count = tail.len().saturating_add(2);
        budget.reserve_analysis_work(tail.len() as u64)?;
        budget.reserve_temporary_bytes(count as u64 * std::mem::size_of::<u32>() as u64)?;
        let mut path = Vec::with_capacity(count);
        path.extend([kind, ordinal]);
        let tail_bytes = tail.capacity() as u64 * std::mem::size_of::<u32>() as u64;
        path.extend(tail);
        budget.release_temporary_bytes(tail_bytes);
        Ok(path)
    }
    match &item.kind {
        s::ItemKind::Aggregate(aggregate) => {
            for (ordinal, _) in aggregate.fields.iter().enumerate() {
                if row(next, target, budget)? {
                    return Ok(Some(prefix(0, ordinal, Vec::new(), budget)?));
                }
            }
            for (ordinal, item) in aggregate.methods.iter().enumerate() {
                if row(next, target, budget)? {
                    return Ok(Some(prefix(1, ordinal, Vec::new(), budget)?));
                }
                if let Some(path) = member_path(item, next, target, budget, depth + 1)? {
                    return Ok(Some(prefix(1, ordinal, path, budget)?));
                }
            }
        }
        s::ItemKind::Enum(enumeration) => {
            for (ordinal, variant) in enumeration.variants.iter().enumerate() {
                if row(next, target, budget)? {
                    return Ok(Some(prefix(2, ordinal, Vec::new(), budget)?));
                }
                let fields = match &variant.fields {
                    s::VariantFields::Unit => 0,
                    s::VariantFields::Tuple(v) => v.len(),
                    s::VariantFields::Record(v) => v.len(),
                };
                for field in 0..fields {
                    if row(next, target, budget)? {
                        return Ok(Some(prefix(
                            2,
                            ordinal,
                            prefix(3, field, Vec::new(), budget)?,
                            budget,
                        )?));
                    }
                }
            }
        }
        s::ItemKind::Trait(declaration) => {
            for (ordinal, item) in declaration.members.iter().enumerate() {
                if row(next, target, budget)? {
                    return Ok(Some(prefix(4, ordinal, Vec::new(), budget)?));
                }
                if let Some(path) = member_path(item, next, target, budget, depth + 1)? {
                    return Ok(Some(prefix(4, ordinal, path, budget)?));
                }
            }
        }
        s::ItemKind::Impl(declaration) => {
            for (ordinal, item) in declaration.members.iter().enumerate() {
                if row(next, target, budget)? {
                    return Ok(Some(prefix(5, ordinal, Vec::new(), budget)?));
                }
                if let Some(path) = member_path(item, next, target, budget, depth + 1)? {
                    return Ok(Some(prefix(5, ordinal, path, budget)?));
                }
            }
        }
        _ => {}
    }
    Ok(None)
}
