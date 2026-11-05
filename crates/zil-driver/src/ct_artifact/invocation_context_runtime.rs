//! Runtime specialization remains lifetime-polymorphic. Its Type/Const actuals
//! are checked against the real original call, and every life slot points back
//! to the callee's own unchanged original definition binder.
use super::source_types::{scalar_domain, step};
use super::*;
use crate::ct_artifact::compiler::{
    runtime_context::RuntimeEntrySource,
    source_sites::{SourceNode, SourceTreeId},
};
use zil_expand::runtime_ct::RuntimeArgument;

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
    pub(in crate::ct_artifact) fn extend_runtime_entry(
        &mut self,
        source: &RuntimeEntrySource,
        raw: &CtArtifactProgram,
        index: &SourceIndex,
        budget: &mut CtExecutionBudget,
    ) -> CompileResult<()> {
        budget.reserve_analysis_work(96)?;
        if source.definition != self.entry_definition
            || source.definition.generation_path.is_some()
            || source.caller.generation_path.is_some()
            || source.call_site.tree != SourceTreeId::Original
            || source.expression_site.tree != SourceTreeId::Original
            || self.entry_site != Some(source.expression_site)
        {
            return Err(unsupported(
                "CT runtime entry has no authentic original source coordinates",
            ));
        }
        let target_scope = self
            .active
            .first()
            .ok_or_else(|| unsupported("CT runtime entry original definition scope absent"))?;
        if target_scope.raw.declaration != source.definition
            || target_scope.raw.enclosing.is_some()
            || target_scope.raw.instance != CtScopeInstance::Definition
        {
            return Err(unsupported(
                "CT runtime entry base is not its original lifetime-polymorphic definition",
            ));
        }
        let (_, target, tree) = self.source_function_source(&source.definition, budget)?;
        if tree.is_some() || target.lexical_parent.is_some() {
            return Err(unsupported(
                "CT runtime entry currently requires an original free generic definition",
            ));
        }
        let caller_scope = CtAmbientLifeScope {
            declaration: source.caller.clone(),
            enclosing: None,
            binder_site: Vec::new(),
            instance: CtScopeInstance::Definition,
        };
        let (_, caller, caller_tree) = self.source_function_source(&source.caller, budget)?;
        budget.reserve_analysis_work(caller.function.generics.len() as u64 + 1)?;
        if caller_tree.is_some()
            || caller.lexical_parent.is_some()
            || !caller.function.generics.is_empty()
        {
            return Err(unsupported(
                "CT runtime call requires an original free nongeneric caller until parent instances are authenticated",
            ));
        }
        let call_site = self.source_location(source.call_site, budget)?;
        if !call_site
            .function
            .is_some_and(|f| std::ptr::eq(f, caller.function))
        {
            return Err(unsupported(
                "CT runtime call site is outside its actual original caller",
            ));
        }
        let SourceNode::Expr(s::Expr {
            kind: s::ExprKind::Call { callee, .. },
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
        let s::ExprKind::TypeApply { callee, arguments } = &callee.kind else {
            return Err(unsupported(
                "CT runtime entry requires original explicit Type/Const actuals",
            ));
        };
        let before = budget.live_temporary_bytes();
        let mut path = Vec::new();
        let checked = (|| {
            self.source_path(callee, &mut path, budget, 0)?;
            let (actual_target, _) = self.source_symbol(
                &source.caller,
                source.call_site.module,
                &path,
                index,
                budget,
            )?;
            budget.reserve_analysis_work(32)?;
            if actual_target != source.definition {
                return Err(unsupported(
                    "CT runtime call does not select the actual original generic definition",
                ));
            }
            let mut base_id = None;
            let mut instance_id = None;
            for (position, scope) in raw.ambient_life_scopes.iter().enumerate() {
                budget.reserve_analysis_work(32)?;
                if scope.declaration != source.definition {
                    continue;
                }
                if scope.enclosing.is_some() || !scope.binder_site.is_empty() {
                    return Err(unsupported(
                        "CT runtime original free scope has unexpected lexical metadata",
                    ));
                }
                match &scope.instance {
                    CtScopeInstance::Definition => {
                        if base_id.replace(CtAmbientScopeId(position as u32)).is_some() {
                            return Err(unsupported(
                                "CT runtime original Definition scope repeats",
                            ));
                        }
                    }
                    CtScopeInstance::Instantiated(_) => {
                        if instance_id
                            .replace(CtAmbientScopeId(position as u32))
                            .is_some()
                        {
                            return Err(unsupported(
                                "CT runtime entry has ambiguous full instance scopes",
                            ));
                        }
                    }
                }
            }
            let base_id = base_id.ok_or_else(|| {
                unsupported("CT runtime original lifetime-polymorphic Definition row absent")
            })?;
            let instance_id = instance_id
                .ok_or_else(|| unsupported("CT runtime complete Type/Const instance row absent"))?;
            let instance = &raw.ambient_life_scopes[instance_id.0 as usize];
            let CtScopeInstance::Instantiated(actuals) = &instance.instance else {
                unreachable!()
            };
            budget.reserve_analysis_work(
                (target.function.generics.len()
                    + source.arguments.len()
                    + actuals.len()
                    + arguments.len()) as u64,
            )?;
            if target.function.generics.len() != actuals.len()
                || actuals.len() != source.arguments.len()
            {
                return Err(unsupported(
                    "CT runtime actual vector loses an original complete generic ordinal",
                ));
            }
            let env = self.source_environment(
                &caller_scope,
                CtAmbientScopeId(u32::MAX),
                &raw.ambient_life_scopes,
                budget,
            )?;
            let mut explicit = arguments.iter();
            for (ordinal, ((parameter, recipe), actual)) in target
                .function
                .generics
                .iter()
                .zip(&source.arguments)
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
                            || reference.scope != base_id
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
                        let Some(s::GenericArg::Type(ty)) = explicit.next() else {
                            return Err(unsupported(
                                "CT runtime Type actual is absent at its original full ordinal",
                            ));
                        };
                        self.source_type(ty, graph, graph.root, &env, raw, index, budget, 0)?;
                    }
                    (
                        s::GenericParamKind::Const(domain),
                        RuntimeArgument::Const(_),
                        CtScopeActual::Scalar(value),
                    ) => {
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
            Ok(instance_id)
        })();
        budget.release_temporary_bytes(budget.live_temporary_bytes().saturating_sub(before));
        let instance_id = checked?;
        let instance = &raw.ambient_life_scopes[instance_id.0 as usize];
        let base = self.active.first().unwrap();
        let cost = super::super::execute::value_context::ambient_cost(
            std::slice::from_ref(instance),
            budget,
        )?
        .saturating_add(identity_cost(&base.identity, budget)?)
        .saturating_add(
            base.parameters.len() as u64 * std::mem::size_of::<CtScopeParameterKind>() as u64,
        )
        .saturating_add(std::mem::size_of::<ActiveScope>() as u64)
        .saturating_add(2 * std::mem::size_of::<usize>() as u64);
        budget.reserve_temporary_bytes(cost)?;
        let active = ActiveScope {
            raw: instance.clone(),
            identity: base.identity.clone(),
            parameters: base.parameters.clone(),
        };
        self.runtime_root_active.reserve_exact(2);
        self.runtime_root_active.push(0);
        self.runtime_root_active.push(self.active.len());
        self.active.reserve_exact(1);
        self.active.push(active);
        self.runtime_scope_id = Some(instance_id);
        self.bytes = self.bytes.saturating_add(cost);
        Ok(())
    }
    pub(in crate::ct_artifact) fn authenticated_runtime_scope_root_count(&self) -> usize {
        self.runtime_root_active.len()
    }
    pub(in crate::ct_artifact) fn authorized_runtime_scope_root(
        &self,
        scope: &CtAmbientLifeScope,
    ) -> bool {
        self.runtime_root_active
            .iter()
            .filter_map(|index| self.active.get(*index))
            .any(|active| &active.raw == scope)
    }
}
