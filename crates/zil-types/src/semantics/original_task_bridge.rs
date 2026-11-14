//! Read-only task producer access to one independently checked original call.
//! Actual Core expression IDs are resolved through the immutable site directory,
//! rather than reinterpreted as original expression IDs.
use super::*;

pub(crate) struct TaskWorkerScheme {
    pub caller: SemanticDefinitionId,
    pub instance: SemanticInstanceId,
    pub call: SemanticSiteId,
    pub worker: SemanticDefinitionId,
    pub signature: SemanticSignatureRecipe,
    pub result: SemanticTypeRecipe,
    pub actuals: Vec<SemanticActualRecipe>,
    pub caller_actuals: Vec<SemanticActual>,
}
impl super::super::emission::Ledger {
    pub(crate) fn original_task_worker(
        &self,
        function: FunctionId,
        expression: NodeId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<TaskWorkerScheme>> {
        let Some(context) = self.original.context() else {
            return Ok(None);
        };
        *left = left
            .checked_sub(self.frames.len().saturating_add(1).ilog2() as usize + 2)
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original Task producer lookup exceeded", None)
            })?;
        let frame = self
            .frames
            .get(&function)
            .ok_or_else(|| bad("Task producer has no real emitter frame"))?;
        let target = match frame.origin.closure {
            None => SemanticFunctionTarget::Core {
                module: frame.origin.module,
                function: frame.origin.creator,
            },
            Some(expression) => SemanticFunctionTarget::CoreClosure {
                module: frame.origin.module,
                creator: frame.origin.creator,
                expression,
                occurrence: 0,
            },
        };
        *left = left
            .checked_sub(
                context
                    .semantic
                    .instances
                    .len()
                    .saturating_add(context.semantic.sites.len())
                    .saturating_add(1),
            )
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original Task source directory work exceeded",
                    None,
                )
            })?;
        let mut instances = context.semantic.instances.iter().filter(|instance|
            matches!(&instance.target, SemanticInstanceTarget::Function(actual) if actual == &target));
        let Some(instance) = instances.next() else {
            return Ok(None);
        };
        if instances.next().is_some() {
            return Err(bad(
                "Task producer has several original caller instances for its exact Core origin",
            ));
        }
        if !context.original.complete.contains(&instance.definition) {
            return Err(bad("Task producer caller has no complete original Gamma"));
        }
        let mut selected = None;
        for site in &context.semantic.sites {
            if site.instance != instance.id
                || !matches!(site.at, SemanticSiteAddress::Core {
                expression: node, occurrence: 0, role: SemanticEmissionRole::Call
            } if node == expression)
            {
                continue;
            }
            let Some(intrinsic) = context
                .original
                .intrinsics
                .get(&(instance.definition, site.recipe))
            else {
                continue;
            };
            let IntrinsicKind::TaskCreate { worker, .. } = intrinsic.kind else {
                continue;
            };
            if selected.replace((site.recipe, worker, intrinsic)).is_some() {
                return Err(bad(
                    "Task producer source operation has ambiguous original worker schemes",
                ));
            }
        }
        let Some((call, worker, intrinsic)) = selected else {
            return Ok(None);
        };
        if !context.original.complete.contains(&worker) {
            return Err(bad("Task producer worker has no complete original Gamma"));
        }
        let (signature, actuals) = intrinsic.worker_scheme.as_ref().ok_or_else(|| {
            bad("Task producer original operation lacks its full solved worker scheme")
        })?;
        crate::dyn_metadata::semantic_operand_snapshot(signature, limits, left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&intrinsic.result, limits, left, 1)?;
        *left = left
            .checked_sub(
                actuals
                    .len()
                    .saturating_add(instance.actuals.len())
                    .saturating_add(1),
            )
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original Task actual vector work exceeded",
                    None,
                )
            })?;
        for actual in actuals {
            crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
        }
        for actual in &instance.actuals {
            crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
        }
        Ok(Some(TaskWorkerScheme {
            caller: instance.definition,
            instance: instance.id,
            call,
            worker,
            signature: signature.clone(),
            result: intrinsic.result.clone(),
            actuals: actuals.clone(),
            caller_actuals: instance.actuals.clone(),
        }))
    }
}
