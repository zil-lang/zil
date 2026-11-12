//! Runtime specialization remains lifetime-polymorphic. Its Type/Const actuals
//! are checked against the real original call, and every life slot points back
//! to the callee's own unchanged original definition binder.
use super::source_types::{scalar_domain, step};
use super::*;
use crate::ct_artifact::compiler::{runtime_context::RuntimeEntrySource, source_sites::SourceNode};
use zil_expand::runtime_ct::RuntimeArgument;

#[derive(Clone, Copy)]
struct RuntimeFrame<'a> {
    definition: &'a CtSymbolId,
    caller: &'a CtSymbolId,
    call_site: crate::ct_artifact::compiler::source_sites::SourceSite,
    arguments: &'a [RuntimeArgument],
}

impl FrozenInvocationContext {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::ct_artifact) fn freeze_runtime_at_source(
        entry: &Entry<'_>,
        parsed: &[Arc<ParseResult>],
        index: &SourceIndex,
        origins: Option<&Arc<Vec<ModuleDeclarationOrigins>>>,
        catalog: Option<&zil_expand::source_context::GeneratedSourceCatalog>,
        source: &RuntimeEntrySource,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<Arc<Self>>> {
        Self::freeze_at_source_force(
            entry,
            parsed,
            index,
            origins,
            catalog,
            Some(source.expression_site),
            true,
            budget,
        )
    }

    // A raw row is only a locator. Every field, full ordinal, and actual is
    // independently checked against the original call in the admitted caller.
    #[allow(clippy::too_many_arguments)]
    fn validate_runtime_frame(
        &self,
        frame: RuntimeFrame<'_>,
        rows: &crate::ct_artifact::compiler::SourceRuntimeFrame,
        caller_scope: &CtAmbientLifeScope,
        caller_id: CtAmbientScopeId,
        root: bool,
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(96)?;
        if &caller_scope.declaration != frame.caller {
            return Err(unsupported(
                "CT runtime parent chain does not select the actual original caller",
            ));
        }
        let (_, target, _) = self.source_function_source(frame.definition, budget)?;
        if target.lexical_parent.is_some() {
            return Err(unsupported(
                "CT runtime member requires its separately authenticated enclosing instance",
            ));
        }
        let (_, caller, _) = self.source_function_source(frame.caller, budget)?;
        budget.reserve_analysis_work(caller.function.generics.len() as u64 + 1)?;
        if caller.lexical_parent.is_some() || (root && !caller.function.generics.is_empty()) {
            return Err(unsupported(
                "CT runtime root requires an actual free nongeneric caller",
            ));
        }
        if !root {
            self.validate_scope(caller_scope, budget)?;
        }
        let base = raw
            .ambient_life_scopes
            .get(rows.base.0 as usize)
            .ok_or_else(|| unsupported("CT runtime original Definition row is absent"))?;
        let instance = raw
            .ambient_life_scopes
            .get(rows.instance.0 as usize)
            .ok_or_else(|| unsupported("CT runtime complete instance row is absent"))?;
        budget.reserve_analysis_work(64)?;
        if &base.declaration != frame.definition
            || &instance.declaration != frame.definition
            || base.enclosing.is_some()
            || instance.enclosing.is_some()
            || !base.binder_site.is_empty()
            || !instance.binder_site.is_empty()
            || base.instance != CtScopeInstance::Definition
        {
            return Err(unsupported(
                "CT runtime frame rows do not select its actual original free declaration",
            ));
        }
        let CtScopeInstance::Instantiated(actuals) = &instance.instance else {
            return Err(unsupported(
                "CT runtime frame lacks its complete actual instance",
            ));
        };
        let call_site = self.source_location(frame.call_site, budget)?;
        if !call_site
            .function
            .is_some_and(|f| std::ptr::eq(f, caller.function))
        {
            return Err(unsupported(
                "CT runtime call site is outside its actual original caller",
            ));
        }
        let SourceNode::Expr(s::Expr {
            kind:
                s::ExprKind::Call {
                    callee,
                    arguments: value_arguments,
                },
            ..
        }) = call_site.node
        else {
            return Err(unsupported(
                "CT runtime entry site is not an actual original call",
            ));
        };
        let mut callee = callee.as_ref();
        while let s::ExprKind::Group(inner) = &callee.kind {
            step(budget, 0)?;
            callee = inner;
        }
        let (callee, arguments) = match &callee.kind {
            s::ExprKind::TypeApply { callee, arguments } => {
                (callee.as_ref(), Some(arguments.as_slice()))
            }
            _ => (callee, None),
        };
        if arguments.is_none() {
            budget.reserve_analysis_work(
                value_arguments.len() as u64 + target.function.params.len() as u64,
            )?;
            if value_arguments.len() != target.function.params.len()
                || value_arguments.iter().any(|argument| argument.spread)
            {
                return Err(unsupported(
                    "CT runtime inferred source arguments have missing or spread values",
                ));
            }
        }
        let before = budget.live_temporary_bytes();
        let mut path = Vec::new();
        let checked = (|| {
            self.source_path(callee, &mut path, budget, 0)?;
            let (actual_target, _) =
                self.source_symbol(frame.caller, frame.call_site.module, &path, index, budget)?;
            budget.reserve_analysis_work(32)?;
            if &actual_target != frame.definition {
                return Err(unsupported(
                    "CT runtime call does not select the actual original generic definition",
                ));
            }
            budget.reserve_analysis_work(
                (target.function.generics.len()
                    + frame.arguments.len()
                    + actuals.len()
                    + arguments.map_or(0, <[s::GenericArg]>::len)) as u64,
            )?;
            if target.function.generics.len() != actuals.len()
                || actuals.len() != frame.arguments.len()
            {
                return Err(unsupported(
                    "CT runtime actual vector loses an original complete generic ordinal",
                ));
            }
            let env =
                self.source_environment(caller_scope, caller_id, &raw.ambient_life_scopes, budget)?;
            let mut explicit = arguments.unwrap_or(&[]).iter();
            for (ordinal, ((parameter, recipe), actual)) in target
                .function
                .generics
                .iter()
                .zip(frame.arguments)
                .zip(actuals)
                .enumerate()
            {
                step(budget, 0)?;
                match (&parameter.kind, recipe, actual) {
                    (
                        s::GenericParamKind::Lifetime,
                        RuntimeArgument::LifeDefinition { ordinal: original },
                        CtScopeActual::Lifetime(reference),
                    ) => {
                        if *original != ordinal as u32
                            || reference.scope != rows.base
                            || reference.parameter != ordinal as u32
                        {
                            return Err(unsupported(
                                "CT runtime life actual does not preserve its own original definition binder",
                            ));
                        }
                    }
                    (
                        s::GenericParamKind::Type(_),
                        RuntimeArgument::Type(_),
                        CtScopeActual::Type(graph),
                    ) => {
                        if arguments.is_some() {
                            let Some(s::GenericArg::Type(ty)) = explicit.next() else {
                                return Err(unsupported(
                                    "CT runtime Type actual is absent at its original full ordinal",
                                ));
                            };
                            self.source_type(ty, graph, graph.root, &env, raw, index, budget, 0)?;
                        } else {
                            self.validate_runtime_inferred_type(
                                parameter,
                                target.function,
                                value_arguments,
                                graph,
                                budget,
                            )?;
                        }
                    }
                    (
                        s::GenericParamKind::Const(domain),
                        RuntimeArgument::Const(_),
                        CtScopeActual::Scalar(value),
                    ) => {
                        if arguments.is_none() {
                            return Err(unsupported(
                                "CT runtime inferred const needs an independent original source rule",
                            ));
                        }
                        let actual = explicit.next().ok_or_else(|| {
                            unsupported(
                                "CT runtime scalar actual is absent at its original full ordinal",
                            )
                        })?;
                        let expected = self.source_scalar(
                            actual,
                            scalar_domain(domain, budget)?,
                            &env,
                            budget,
                            0,
                        )?;
                        budget.reserve_analysis_work(32)?;
                        if &expected != value {
                            return Err(unsupported(
                                "CT runtime scalar actual differs from the actual original call",
                            ));
                        }
                    }
                    _ => {
                        return Err(unsupported(
                            "CT runtime actual kind differs from the original full generic ordinal",
                        ));
                    }
                }
            }
            if explicit.next().is_some() {
                return Err(unsupported(
                    "CT runtime source has additional actuals without an authenticated definition-phase rule",
                ));
            }
            Ok(())
        })();
        budget.release_temporary_bytes(budget.live_temporary_bytes().saturating_sub(before));
        checked
    }

    // Literal constraints are reconstructed separately from the producer's
    // inference. Recipes select a kind, never a Type graph or an inferred value.
    fn validate_runtime_inferred_type(
        &self,
        generic: &s::GenericParam,
        target: &s::Function,
        arguments: &[s::Argument],
        graph: &CtTypeRef,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(target.params.len() as u64)?;
        let mut constraints = 0usize;
        for (formal, actual) in target.params.iter().zip(arguments) {
            let mut ty = &formal.ty;
            let mut depth = 0;
            while let s::TypeKind::Group(inner) = &ty.kind {
                step(budget, depth)?;
                depth += 1;
                ty = inner;
            }
            step(budget, depth)?;
            let s::TypeKind::Named { path, args } = &ty.kind else {
                continue;
            };
            budget.reserve_analysis_work((path.len() + args.len()) as u64)?;
            let [name] = path.as_slice() else {
                continue;
            };
            budget.reserve_analysis_work(name.len().min(generic.name.len()) as u64 + 1)?;
            if !args.is_empty() || name != &generic.name {
                continue;
            }
            constraints += 1;
            let mut value = &actual.value;
            let mut depth = 0;
            while let s::ExprKind::Group(inner) = &value.kind {
                step(budget, depth)?;
                depth += 1;
                value = inner;
            }
            step(budget, depth)?;
            let expected = match &value.kind {
                s::ExprKind::Literal(s::Literal::Bool(_)) => CtScalarTy::Bool,
                s::ExprKind::Literal(s::Literal::Integer(text)) => {
                    budget.reserve_analysis_work(text.len() as u64)?;
                    match zil_types::parse_integer_literal(text, None, false, value.span)? {
                        zil_ir::Constant::U8(_) => CtScalarTy::U8,
                        zil_ir::Constant::U16(_) => CtScalarTy::U16,
                        zil_ir::Constant::U32(_) => CtScalarTy::U32,
                        zil_ir::Constant::U64(_) => CtScalarTy::U64,
                        zil_ir::Constant::Usize(_) => CtScalarTy::Usize,
                        zil_ir::Constant::I32(_) => CtScalarTy::I32,
                        zil_ir::Constant::I8(_) => CtScalarTy::I8,
                        zil_ir::Constant::I16(_) => CtScalarTy::I16,
                        zil_ir::Constant::Isize(_) => CtScalarTy::Isize,
                        zil_ir::Constant::I64(_) => CtScalarTy::I64,
                        _ => {
                            return Err(unsupported(
                                "CT runtime inferred integer has no original scalar domain",
                            ));
                        }
                    }
                }
                _ => {
                    return Err(unsupported(
                        "CT runtime inference requires an independently typed original value argument",
                    ));
                }
            };
            budget.reserve_analysis_work(4)?;
            if graph.root != CtTypeNodeId(0)
                || !graph.ambient_classes.is_empty()
                || graph.nodes.as_slice() != [CtTypeNode::Primitive(expected)]
            {
                return Err(unsupported(
                    "CT runtime inferred Type differs from an original source value constraint",
                ));
            }
        }
        if constraints == 0 {
            return Err(unsupported(
                "CT runtime Type binder has no independently inferred original source constraint",
            ));
        }
        Ok(())
    }

    fn runtime_existing_scope(
        &self,
        scope: &CtAmbientLifeScope,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<Option<usize>> {
        budget.reserve_analysis_work(self.active.len() as u64 * 32)?;
        self.charge_instance_lookup(scope, budget)?;
        Ok(self.active.iter().position(|active| &active.raw == scope))
    }

    // Definition identities and parameter kinds come from the real immutable
    // declaration. Parent recipes and raw scope rows cannot supply either.
    fn admit_runtime_definition(
        &mut self,
        scope: &CtAmbientLifeScope,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<usize> {
        if let Some(existing) = self.runtime_existing_scope(scope, budget)? {
            return Ok(existing);
        }
        let (_, original, tree) = self.source_function_source(&scope.declaration, budget)?;
        let origins = self
            .origins
            .as_ref()
            .ok_or_else(|| unsupported("CT runtime declaration has no original lineage"))?;
        budget.reserve_analysis_work(origins.len() as u64)?;
        let identity_before = budget.live_temporary_bytes();
        let generated_identity;
        let identity = if let Some(tree) = tree {
            let catalog = self.generated_catalog.as_ref().ok_or_else(|| {
                unsupported("CT runtime generated declaration has no frozen catalog")
            })?;
            budget.reserve_analysis_work(catalog.trees.len() as u64)?;
            let row = catalog
                .trees
                .iter()
                .position(|candidate| std::ptr::eq(&**candidate, tree))
                .ok_or_else(|| unsupported("CT runtime generated declaration tree is absent"))?;
            generated_identity =
                super::generated::root_identity(catalog, row, original.top, origins, budget)?;
            &generated_identity
        } else {
            &origins
                .iter()
                .find(|row| row.module == scope.declaration.module)
                .and_then(|row| row.items.get(original.top))
                .ok_or_else(|| unsupported("CT runtime declaration original namespace is absent"))?
                .identity
        };
        let temporary_identity = budget
            .live_temporary_bytes()
            .saturating_sub(identity_before);
        let bytes = super::super::execute::value_context::ambient_cost(
            std::slice::from_ref(scope),
            budget,
        )?
        .saturating_add(identity_cost(identity, budget)?)
        .saturating_add(std::mem::size_of::<ActiveScope>() as u64)
        .saturating_add(
            original.function.generics.len() as u64
                * std::mem::size_of::<CtScopeParameterKind>() as u64,
        );
        budget.reserve_temporary_bytes(bytes)?;
        let mut parameters = Vec::with_capacity(original.function.generics.len());
        for parameter in &original.function.generics {
            step(budget, 0)?;
            parameters.push(match &parameter.kind {
                s::GenericParamKind::Type(_) => CtScopeParameterKind::Type,
                s::GenericParamKind::Lifetime => CtScopeParameterKind::Lifetime,
                s::GenericParamKind::Const(domain) => {
                    CtScopeParameterKind::Scalar(scalar_domain(domain, budget)?)
                }
            });
        }
        let active = ActiveScope {
            raw: scope.clone(),
            identity: identity.clone(),
            parameters,
        };
        budget.release_temporary_bytes(temporary_identity);
        self.active.reserve_exact(1);
        let index = self.active.len();
        self.active.push(active);
        self.bytes = self.bytes.saturating_add(bytes);
        Ok(index)
    }
    fn admit_runtime_instance(
        &mut self,
        scope: &CtAmbientLifeScope,
        base: usize,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<usize> {
        if let Some(existing) = self.runtime_existing_scope(scope, budget)? {
            return Ok(existing);
        }
        let base = self
            .active
            .get(base)
            .ok_or_else(|| unsupported("CT runtime original definition admission is absent"))?;
        let bytes = super::super::execute::value_context::ambient_cost(
            std::slice::from_ref(scope),
            budget,
        )?
        .saturating_add(identity_cost(&base.identity, budget)?)
        .saturating_add(std::mem::size_of::<ActiveScope>() as u64)
        .saturating_add(
            base.parameters.len() as u64 * std::mem::size_of::<CtScopeParameterKind>() as u64,
        );
        budget.reserve_temporary_bytes(bytes)?;
        let active = ActiveScope {
            raw: scope.clone(),
            identity: base.identity.clone(),
            parameters: base.parameters.clone(),
        };
        self.active.reserve_exact(1);
        let index = self.active.len();
        self.active.push(active);
        self.bytes = self.bytes.saturating_add(bytes);
        Ok(index)
    }

    pub(in crate::ct_artifact) fn extend_runtime_entry(
        &mut self,
        source: &RuntimeEntrySource,
        rows: &[crate::ct_artifact::compiler::SourceRuntimeFrame],
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(64)?;
        if source.definition != self.entry_definition
            || self.entry_site != Some(source.expression_site)
        {
            return Err(unsupported(
                "CT runtime entry has no authentic frozen source coordinates",
            ));
        }
        if self.runtime_scope_id.is_some() || !self.runtime_root_active.is_empty() {
            return Err(unsupported(
                "CT runtime invocation already admitted its complete source chain",
            ));
        }
        let count = source
            .parents
            .len()
            .checked_add(1)
            .ok_or_else(|| unsupported("CT runtime source parent chain length overflow"))?;
        budget.reserve_analysis_work(count as u64 + rows.len() as u64)?;
        if rows.len() != count {
            return Err(unsupported(
                "CT runtime source chain loses a complete original frame",
            ));
        }
        if count > budget.remaining().max_call_depth as usize {
            return Err(vec![Diagnostic::limit(
                Phase::Ct,
                "CT runtime source parent chain exceeds depth budget",
                None,
            )]);
        }
        let root_caller = source
            .parents
            .first()
            .map_or(&source.caller, |frame| &frame.caller);
        let root_scope = CtAmbientLifeScope {
            declaration: root_caller.clone(),
            enclosing: None,
            binder_site: Vec::new(),
            instance: CtScopeInstance::Definition,
        };
        let frames = source
            .parents
            .iter()
            .map(|frame| RuntimeFrame {
                definition: &frame.definition,
                caller: &frame.caller,
                call_site: frame.call_site,
                arguments: &frame.arguments,
            })
            .chain(std::iter::once(RuntimeFrame {
                definition: &source.definition,
                caller: &source.caller,
                call_site: source.call_site,
                arguments: &source.arguments,
            }));
        let mut previous = None;
        let mut roots = Vec::new();
        let mut root_bytes = 0u64;
        let admitted = (|| {
            for (position, (frame, row)) in frames.zip(rows).enumerate() {
                let caller_id = previous.unwrap_or(CtAmbientScopeId(u32::MAX));
                let caller_scope = if let Some(previous) = previous {
                    raw.ambient_life_scopes
                        .get(previous.0 as usize)
                        .ok_or_else(|| {
                            unsupported("CT runtime authenticated parent scope placement is absent")
                        })?
                } else {
                    &root_scope
                };
                self.validate_runtime_frame(
                    frame,
                    row,
                    caller_scope,
                    caller_id,
                    position == 0,
                    raw,
                    index,
                    budget,
                )?;
                let base = &raw.ambient_life_scopes[row.base.0 as usize];
                let instance = &raw.ambient_life_scopes[row.instance.0 as usize];
                let base = self.admit_runtime_definition(base, budget)?;
                let instance = self.admit_runtime_instance(instance, base, budget)?;
                for (id, active) in [(row.base, base), (row.instance, instance)] {
                    budget.reserve_analysis_work(roots.len() as u64 * 2)?;
                    if let Some((_, admitted)) = roots.iter().find(|(row, _)| *row == id) {
                        if *admitted != active {
                            return Err(unsupported(
                                "CT runtime reused row changed its independently admitted scope",
                            ));
                        }
                        continue;
                    }
                    // The producer's interner is independently reconstructed:
                    // fresh frame rows form one exact prefix; repeated complete
                    // scopes reuse that prefix row instead of creating aliases.
                    if id.0 as usize != roots.len()
                        || roots.iter().any(|(_, admitted)| *admitted == active)
                    {
                        return Err(unsupported(
                            "CT runtime rows do not follow the canonical original-call frame prefix",
                        ));
                    }
                    let bytes = std::mem::size_of::<(CtAmbientScopeId, usize)>() as u64;
                    budget.reserve_temporary_bytes(bytes)?;
                    root_bytes += bytes;
                    roots.reserve_exact(1);
                    roots.push((id, active));
                }
                previous = Some(row.instance);
            }
            Ok(())
        })();
        if let Err(error) = admitted {
            budget.release_temporary_bytes(root_bytes);
            return Err(error);
        }
        self.runtime_root_active = roots;
        self.runtime_scope_id = previous;
        self.bytes = self.bytes.saturating_add(root_bytes);
        Ok(())
    }
    pub(in crate::ct_artifact) fn authenticated_runtime_scope_root_count(&self) -> usize {
        self.runtime_root_active.len()
    }
    pub(in crate::ct_artifact) fn authorized_runtime_scope_root(
        &self,
        index: usize,
        scope: &CtAmbientLifeScope,
    ) -> bool {
        self.runtime_root_active
            .get(index)
            .filter(|(id, _)| id.0 as usize == index)
            .and_then(|(_, active)| self.active.get(*active))
            .is_some_and(|active| &active.raw == scope)
    }
}

#[cfg(test)]
#[path = "invocation_context_runtime_tests.rs"]
mod tests;
