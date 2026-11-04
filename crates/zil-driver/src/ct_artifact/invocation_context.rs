//! Immutable source witnesses for one actual CT invocation. Raw table indices,
//! source names and spans never select a lexical owner.
use super::{compiler::Entry, source_index::SourceIndex};
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
fn identity_cost(
    identity: &LifeDeclarationIdentity,
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    let count = identity
        .module_path
        .len()
        .saturating_add(identity.declaration_path.len())
        .saturating_add(identity.generation_path.len());
    budget.reserve_analysis_work(count as u64)?;
    let mut bytes = std::mem::size_of::<LifeDeclarationIdentity>() as u64;
    for text in std::iter::once(&identity.package)
        .chain(&identity.module_path)
        .chain(&identity.declaration_path)
    {
        budget.reserve_analysis_work(text.len() as u64)?;
        bytes = bytes
            .saturating_add(text.len() as u64)
            .saturating_add(std::mem::size_of::<String>() as u64);
    }
    Ok(bytes.saturating_add(identity.generation_path.len() as u64 * 4))
}
pub(super) fn origins_cost(
    origins: &[ModuleDeclarationOrigins],
    budget: &mut CtExecutionBudget,
) -> CompileResult<u64> {
    budget.reserve_analysis_work(origins.len() as u64)?;
    let mut bytes = origins.len() as u64 * std::mem::size_of::<ModuleDeclarationOrigins>() as u64;
    for module in origins {
        budget.reserve_analysis_work(module.items.len() as u64)?;
        for item in &module.items {
            bytes = bytes.saturating_add(identity_cost(&item.identity, budget)?);
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
}
impl FrozenInvocationContext {
    pub(super) fn freeze(
        entry: &Entry<'_>,
        parsed: &[Arc<ParseResult>],
        index: &SourceIndex,
        origins: Option<&Arc<Vec<ModuleDeclarationOrigins>>>,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Arc<Self>> {
        let definition = match entry {
            Entry::Expression { definition, .. } | Entry::Function(definition) => definition,
        };
        budget.reserve_analysis_work(parsed.len() as u64 + index.symbol_lookup_work())?;
        let module = parsed
            .iter()
            .find(|module| module.module.id == definition.module)
            .ok_or_else(|| unsupported("CT invocation has no actual original module"))?;
        if definition.generation_path.is_some() || !index.contains(definition) {
            return Err(unsupported(
                "generated CT invocation lacks authentic scope lineage",
            ));
        }
        // SourceIndex reserves the original top-level item ordinals before all
        // member rows. A member is not reinterpreted as a top-level declaration.
        let item = module.module.items.get(definition.declaration.0 as usize);
        let function = item.and_then(|item| match &item.kind {
            s::ItemKind::Function(function) | s::ItemKind::Extern { function, .. } => {
                Some(function)
            }
            _ => None,
        });
        let Some(function) = function else {
            return Ok(Arc::new(Self {
                active: Vec::new(),
                bytes: 0,
                origins: None,
                modules: Vec::new(),
            }));
        };
        budget.reserve_analysis_work(function.generics.len() as u64)?;
        if !function
            .generics
            .iter()
            .any(|parameter| matches!(parameter.kind, s::GenericParamKind::Lifetime))
        {
            return Ok(Arc::new(Self {
                active: Vec::new(),
                bytes: 0,
                origins: None,
                modules: Vec::new(),
            }));
        }
        let origins = origins
            .ok_or_else(|| unsupported("CT ambient scope requires original declaration lineage"))?;
        budget.reserve_analysis_work(origins.len() as u64)?;
        let origin = origins
            .iter()
            .find(|row| row.module == definition.module)
            .and_then(|row| row.items.get(definition.declaration.0 as usize))
            .ok_or_else(|| unsupported("CT ambient owner is absent from original lineage"))?;
        if origin.declaration != definition.declaration {
            return Err(unsupported(
                "CT ambient original declaration ordinal differs",
            ));
        }
        let bytes = (parsed.len() as u64)
            .saturating_mul(std::mem::size_of::<Arc<ParseResult>>() as u64)
            .saturating_add(identity_cost(&origin.identity, budget)?)
            .saturating_add(std::mem::size_of::<ActiveScope>() as u64)
            .saturating_add(
                function.generics.len() as u64 * std::mem::size_of::<CtScopeParameterKind>() as u64,
            );
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
        Ok(Arc::new(Self {
            active: vec![ActiveScope {
                raw: CtAmbientLifeScope {
                    declaration: definition.clone(),
                    binder_site: Vec::new(),
                    instance: CtScopeInstance::Definition,
                },
                identity: origin.identity.clone(),
                parameters,
            }],
            bytes,
            origins: Some(origins.clone()),
            modules: parsed.to_vec(),
        }))
    }
    pub(super) fn symbol_key(
        &self,
        symbol: &CtSymbolId,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Vec<u8>> {
        if symbol.generation_path.is_some() {
            return Err(unsupported(
                "generated CT semantic symbol lacks authentic lineage",
            ));
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
        budget.reserve_analysis_work(out.len() as u64)?;
        debug_assert!(out.capacity() <= size);
        Ok(out)
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
        if self.parameter_kind(scope, reference.parameter as usize)
            != Some(CtScopeParameterKind::Lifetime)
        {
            return Err(unsupported(
                "CT ambient reference is not an actual lifetime ordinal",
            ));
        }
        Ok(())
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
        budget.reserve_temporary_bytes((tail.len() as u64 + 2) * 8)?;
        let mut path = vec![kind, ordinal];
        path.extend(tail);
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
