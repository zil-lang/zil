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
    pub result_sources: Vec<TaskReturnSources>,
}

impl Proof {
    /// Resolve the declaration whose full mixed actuals an inferred holder
    /// consumes. A Task operation has a real separately checked worker; it is
    /// never entered into the ordinary direct-call target inventory.
    pub(in crate::semantics) fn inferred_call_target(
        &self,
        owner: SemanticDefinitionId,
        call: SemanticSiteId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<SemanticDefinitionId>> {
        *left = left
            .checked_sub(
                self.call_targets.len().saturating_add(1).ilog2() as usize
                    + self.intrinsics.len().saturating_add(1).ilog2() as usize
                    + 2,
            )
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original inferred target lookup exceeded",
                    None,
                )
            })?;
        let ordinary = self.call_targets.get(&(owner, call)).copied();
        let Some(intrinsic) = self.intrinsics.get(&(owner, call)) else {
            return Ok(ordinary);
        };
        let IntrinsicKind::TaskCreate { worker, .. } = intrinsic.kind else {
            return Ok(ordinary);
        };
        if ordinary.is_some() || !self.complete.contains(&owner) || !self.complete.contains(&worker)
        {
            return Err(bad(
                "inferred Task holder changes its complete original call kind or worker",
            ));
        }
        let (signature, actuals) = intrinsic
            .worker_scheme
            .as_ref()
            .ok_or_else(|| bad("inferred Task holder lacks its complete solved worker scheme"))?;
        let recorded = self
            .calls
            .get(&(owner, call))
            .ok_or_else(|| bad("inferred Task holder lacks its original full mixed solution"))?;
        if actuals.len() > limits.max_nodes || signature.parameters.len() > limits.max_nodes {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original inferred worker scheme limit exceeded",
                None,
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(signature, limits, left, 1)?;
        *left = left
            .checked_sub(
                actuals
                    .len()
                    .saturating_add(recorded.len())
                    .saturating_add(intrinsic.parameters.len())
                    .saturating_add(1),
            )
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original inferred worker comparison exceeded",
                    None,
                )
            })?;
        for actual in actuals.iter().chain(recorded) {
            crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
        }
        for parameter in &intrinsic.parameters {
            crate::dyn_metadata::semantic_operand_snapshot(parameter, limits, left, 1)?;
        }
        if actuals != recorded
            || signature.parameters.len() != intrinsic.parameters.len()
            || signature
                .parameters
                .iter()
                .zip(&intrinsic.parameters)
                .any(|(a, b)| &a.ty != b)
        {
            return Err(bad(
                "inferred Task holder changes its complete mixed or runtime input scheme",
            ));
        }
        Ok(Some(worker))
    }
}
impl super::super::emission::Ledger {
    fn original_task_call(
        &self,
        function: FunctionId,
        expression: NodeId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<
        Option<(
            &super::super::original_context::OriginalContext,
            &SemanticInstance,
            SemanticSiteId,
            &IntrinsicCall,
        )>,
    > {
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
            if !matches!(
                intrinsic.kind,
                IntrinsicKind::TaskCreate { .. } | IntrinsicKind::TaskOperation(_)
            ) {
                continue;
            }
            if selected.replace((site.recipe, intrinsic)).is_some() {
                return Err(bad(
                    "Task producer source operation has ambiguous original worker schemes",
                ));
            }
        }
        let Some((call, intrinsic)) = selected else {
            return Ok(None);
        };
        Ok(Some((context.as_ref(), instance, call, intrinsic)))
    }
    pub(crate) fn original_task_payload(
        &self,
        function: FunctionId,
        expression: NodeId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<(SemanticInstanceId, TaskOp, SemanticTypeRecipe)>> {
        let Some((context, instance, call, intrinsic)) =
            self.original_task_call(function, expression, limits, left)?
        else {
            return Ok(None);
        };
        let IntrinsicKind::TaskOperation(op) = intrinsic.kind else {
            return Ok(None);
        };
        let actual = context
            .original
            .call_actual(instance.definition, call, 0, limits, left)?;
        let SemanticActualRecipe::Type(payload) = actual else {
            return Err(bad(
                "original Task operation payload changes its full Type slot",
            ));
        };
        Ok(Some((instance.id, op, payload)))
    }
    pub(crate) fn original_task_worker(
        &self,
        function: FunctionId,
        expression: NodeId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<TaskWorkerScheme>> {
        let Some((context, instance, call, intrinsic)) =
            self.original_task_call(function, expression, limits, left)?
        else {
            return Ok(None);
        };
        let IntrinsicKind::TaskCreate { worker, .. } = intrinsic.kind else {
            return Ok(None);
        };
        if !context.original.complete.contains(&worker) {
            return Err(bad("Task producer worker has no complete original Gamma"));
        }
        if context
            .original
            .inferred_call_target(instance.definition, call, limits, left)?
            != Some(worker)
        {
            return Err(bad(
                "Task producer changes its checked original inferred worker",
            ));
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
            result_sources: context.original.task_result_sources(
                &context.semantic,
                worker,
                limits,
                left,
            )?,
        }))
    }
}
