//! A result annotation selects Task's result/owning family. Its actual input
//! Context is obtained from every checked return, never an actual header.
use super::*;

fn task_payload(ty: &SemanticTypeRecipe) -> bool {
    match ty {
        SemanticTypeRecipe::Managed(inner) | SemanticTypeRecipe::Desc { inner, .. } => {
            matches!(inner.as_ref(), SemanticTypeRecipe::Task { .. })
        }
        _ => false,
    }
}

impl Proof {
    /// Congruence of complete original whole Types, not equality of concrete
    /// layouts or of two caller instantiations. The capture consumer separately
    /// checks that its shared intrinsic ordinal exists in both complete domains.
    pub(in crate::semantics) fn equal_capture_types(
        &self,
        program: &SemanticModule,
        owner: SemanticDefinitionId,
        left_slot: u32,
        right_slot: u32,
        limits: &Limits,
        left: &mut usize,
    ) -> R<bool> {
        *left = left.checked_sub(self.complete.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "original capture equation lookup work exceeded", None))?;
        if !self.complete.contains(&owner) { return Ok(false); }
        let mut check = super::nominal_where::borrowed_check(program, self, owner, limits, left)?;
        let declaration = check.definition(owner)?;
        if declaration.kind != SemanticDefinitionKind::Closure { return Ok(false); }
        for slot in [left_slot, right_slot] {
            check.pay(self.environments.len().saturating_add(1).ilog2() as usize + 2, 0)?;
            if !matches!(declaration.environment.get(slot as usize).map(|slot| &slot.kind), Some(SemanticFormal::Type { .. }))
                || self.environments.get(&(owner, slot)).is_none_or(|(parent, _)| Some(*parent) != declaration.parent)
            { return Ok(false); }
        }
        let a = SemanticTypeRecipe::Environment { scope: SemanticScopeRef::Ancestor(0), ordinal: left_slot };
        let b = SemanticTypeRecipe::Environment { scope: SemanticScopeRef::Ancestor(0), ordinal: right_slot };
        match check.same(&a, &b) {
            Ok(()) => Ok(true),
            Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => Err(error),
            Err(_) => Ok(false),
        }
    }
    /// A Task Context part is empty only by the immutable original theorem.
    /// This grants no actual closure value, cleanup or callable permission.
    pub(in crate::semantics) fn task_type_is_zero(
        &self,
        program: &SemanticModule,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        limits: &Limits,
        left: &mut usize,
        depth: usize,
    ) -> R<bool> {
        if !self.complete.contains(&owner) {
            return Ok(false);
        }
        let mut check = super::nominal_where::borrowed_check(program, self, owner, limits, left)?;
        match check.original_zero_domain(owner, ty, depth + 1) {
            Ok(zero) => Ok(zero),
            Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => Err(error),
            // This optional query supplies no fact when its original lexical
            // mapping is delayed. The normal complete materializer still runs.
            Err(_) => Ok(false),
        }
    }
}
impl Check<'_, '_> {
    fn task_sequence(
        &mut self,
        owner: SemanticDefinitionId,
        sequence: &SemanticClassSequenceRecipe,
        depth: usize,
    ) -> R<SemanticClassSequenceRecipe> {
        crate::dyn_metadata::semantic_operand_snapshot(sequence, self.limits, self.left, 1)?;
        let mut sequence = sequence.clone();
        let mut result = Vec::new();
        for mut part in sequence.parts {
            if let SemanticClassSequencePart::AllClasses { ty, path } = &mut part {
                **ty = self.resolve(owner, ty, depth + 1)?;
                if path.is_empty() && self.original_zero_domain(owner, ty, depth + 1)? {
                    continue;
                }
            }
            result.push(part);
        }
        sequence.parts = result;
        Ok(sequence)
    }
    pub(super) fn normalized_task_shape(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticRetainedShapeRecipe,
        depth: usize,
    ) -> R<SemanticRetainedShapeRecipe> {
        crate::dyn_metadata::semantic_operand_snapshot(value, self.limits, self.left, 1)?;
        let mut value = value.clone();
        value.domain = self.task_sequence(owner, &value.domain, depth + 1)?;
        let mut ports = Vec::new();
        for mut part in value.ports {
            match &mut part {
                SemanticPortPart::FromType { ty, classes } => {
                    **ty = self.resolve(owner, ty, depth + 1)?;
                    *classes = self.task_sequence(owner, classes, depth + 1)?;
                    if classes.parts.is_empty()
                        && self.original_zero_domain(owner, ty, depth + 1)?
                    {
                        continue;
                    }
                }
                SemanticPortPart::One { leaf, .. } => {
                    **leaf = self.resolve(owner, leaf, depth + 1)?
                }
            }
            ports.push(part);
        }
        value.ports = ports;
        let mut retained = Vec::new();
        for mut part in value.retained {
            if let SemanticRetainedPart::All { path, parents } = &mut part {
                *parents = self.task_sequence(owner, parents, depth + 1)?;
                if parents.parts.is_empty()
                    && matches!(
                        path.as_slice(),
                        [LifeTypeProjection::Result] | [LifeTypeProjection::Parameter(_)]
                    )
                {
                    continue;
                }
            }
            retained.push(part);
        }
        value.retained = retained;
        let mut nominal = Vec::new();
        for mut part in value.nominal {
            if let SemanticNominalPart::FromType { path, ty, classes } = &mut part {
                **ty = self.resolve(owner, ty, depth + 1)?;
                *classes = self.task_sequence(owner, classes, depth + 1)?;
                if matches!(path.as_slice(), [LifeTypeProjection::Parameter(_)])
                    && classes.parts.is_empty()
                    && self.original_zero_domain(owner, ty, depth + 1)?
                {
                    continue;
                }
            }
            nominal.push(part);
        }
        value.nominal = nominal;
        Ok(value)
    }
    pub(super) fn same_task_shape(
        &mut self,
        owner: SemanticDefinitionId,
        left: &SemanticRetainedShapeRecipe,
        right: &SemanticRetainedShapeRecipe,
        depth: usize,
    ) -> R<()> {
        if self.contexts.last().copied() != Some(owner) {
            return Err(bad(
                "original Task comparison leaves its exact lexical Gamma",
            ));
        }
        let left = self.normalized_task_shape(owner, left, depth + 1)?;
        let right = self.normalized_task_shape(owner, right, depth + 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&left, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&right, self.limits, self.left, 1)?;
        if left != right {
            // Exact original types/lifetimes can have distinct sealed recipe
            // spellings. Reuse the complete table traversal in rigid mode:
            // no target-private variable is solved and no class is merged.
            let answer = self.compare_task_shape(
                &left,
                &right,
                &[],
                &mut [],
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                true,
                depth + 1,
            );
            let cause = match answer {
                Ok(()) => return Ok(()),
                Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => {
                    return Err(error);
                }
                Err(error) => error,
            };
            self.pay(384, depth)?;
            let mut error =
                bad("original Task changes its complete input Context identity or projection");
            error.notes.push(format!("original Task Context: domain {}/{}, ports {}/{}, retained {}/{}, nominal {}/{}; exact tables equal {}/{}/{}/{}",
                left.domain.parts.len(), right.domain.parts.len(), left.ports.len(), right.ports.len(),
                left.retained.len(), right.retained.len(), left.nominal.len(), right.nominal.len(),
                left.domain == right.domain, left.ports == right.ports, left.retained == right.retained, left.nominal == right.nominal));
            let detail = format!(
                "rigid Context comparison: {}; actual {left:?}; expected {right:?}",
                cause.message
            );
            self.pay(detail.len(), depth)?;
            error.notes.push(detail);
            error.notes.extend(cause.notes);
            return Err(error);
        }
        Ok(())
    }
    pub(super) fn task_return_family(&self, result: &SemanticTypeRecipe) -> bool {
        match result {
            SemanticTypeRecipe::ErrorUnion { result, .. } => task_payload(result),
            _ => task_payload(result),
        }
    }
    fn task_payload_return(
        &mut self,
        owner: SemanticDefinitionId,
        declared: &SemanticTypeRecipe,
        actual: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        let (declared, actual) = match (declared, actual) {
            (SemanticTypeRecipe::Managed(left), SemanticTypeRecipe::Managed(right)) => {
                (left.as_ref(), right.as_ref())
            }
            (
                SemanticTypeRecipe::Desc {
                    lifetime: a,
                    inner: left,
                },
                SemanticTypeRecipe::Desc {
                    lifetime: b,
                    inner: right,
                },
            ) => {
                if a != b {
                    return Err(bad("Task result family changes its descriptor occurrence"));
                }
                (left.as_ref(), right.as_ref())
            }
            _ => {
                return Err(bad(
                    "Task result family changes its declared owning/description category",
                ));
            }
        };
        let (
            SemanticTypeRecipe::Task {
                result: wanted,
                retained: declared,
            },
            SemanticTypeRecipe::Task {
                result: value,
                retained: actual,
            },
        ) = (declared, actual)
        else {
            return Err(bad(
                "Task result family changes its true sealed payload kind",
            ));
        };
        self.same(value, wanted)?;
        // Only the result-root omitted Context is inferred. An explicit input
        // Context remains an independently checked complete equality obligation.
        if !declared.domain.parts.is_empty()
            || !declared.ports.is_empty()
            || !declared.retained.is_empty()
            || !declared.nominal.is_empty()
        {
            self.same_task_shape(owner, actual, declared, depth + 1)?;
        }
        Ok(())
    }
    /// The source theorem and the executable TaskCreate keep the worker's
    /// true input indices. A direct return may advertise factory indices only
    /// when the original call's retained operands are those stable parameters.
    fn projected_constructor_result(
        &mut self, owner: SemanticDefinitionId, expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>, actual: &SemanticTypeRecipe, depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        let CoreExprKind::Call { arguments, .. } = &expression.kind else { return Ok(None) };
        let Some(site) = self.site(owner, SemanticOriginalNode::Expression(expression.id))?
            else { return Ok(None) };
        if !matches!(self.proof.intrinsics.get(&(owner, site.id)).map(|i| &i.kind),
            Some(IntrinsicKind::TaskCreate { owned: false, await_result: false, .. })) {
            return Ok(None);
        }
        let mut projected = copy_recipe(actual, self.limits, self.left)?;
        let SemanticTypeRecipe::ErrorUnion { result, .. } = &mut projected else { return Ok(None) };
        let SemanticTypeRecipe::Managed(task) = result.as_mut() else { return Ok(None) };
        let SemanticTypeRecipe::Task { retained, .. } = task.as_mut() else { return Ok(None) };
        **retained = self.normalized_task_shape(owner, retained, depth + 1)?;
        let mut origins = BTreeMap::new();
        let mut used = BTreeSet::new();
        let paths = retained.retained.iter_mut().map(|r| match r {
            SemanticRetainedPart::All {path, ..} | SemanticRetainedPart::One {path, ..} => path,
        }).chain(retained.nominal.iter_mut().map(|r| match r {
            SemanticNominalPart::FromType {path, ..} | SemanticNominalPart::One {path, ..} => path,
        }));
        for path in paths {
            self.pay(path.len().saturating_add(variables.len()).saturating_add(origins.len()).saturating_add(1), depth)?;
            let Some(LifeTypeProjection::Parameter(index)) = path.first_mut() else { continue };
            if let Some(p) = origins.get(index) { *index = *p; continue; }
            let Some(argument) = arguments.get(*index as usize + 1) else { return Ok(None) };
            let CoreExprKind::Name(name) = &argument.kind else { return Ok(None) };
            let Some(binding) = variables.get(name) else { return Ok(None) };
            let Query::Parameter(parameter) = binding.origin else { return Ok(None) };
            if binding.origin_owner != owner || binding.mutable || binding.written || !used.insert(parameter) {
                return Ok(None);
            }
            origins.insert(*index, parameter);
            *index = parameter;
        }
        Ok(Some(projected))
    }
    pub(super) fn task_return_value(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        declared: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let actual = self.expr(owner, expression, variables, None, depth + 1)?;
        let mut projected = false;
        let value = if let SemanticTypeRecipe::ErrorUnion { error, result } = declared {
            if let SemanticTypeRecipe::ErrorUnion {
                error: actual_error,
                result: actual_result,
            } = &actual
            {
                self.same(actual_error, error)?;
                match self.task_payload_return(owner, result, actual_result, depth + 1) {
                    Ok(()) => actual,
                    Err(e) if e.category == zil_base::ResultCategory::ResourceLimit => return Err(e),
                    Err(e) => {
                        let Some(SemanticTypeRecipe::ErrorUnion { result: projected_result, .. }) =
                            self.projected_constructor_result(owner, expression, variables, &actual, depth + 1)?
                        else { return Err(e); };
                        self.task_payload_return(owner, result, &projected_result, depth + 1)?;
                        projected = true;
                        copy_recipe(declared, self.limits, self.left)?
                    }
                }
            } else {
                let is_error = match self.same(&actual, error) {
                    Ok(()) => true,
                    Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => {
                        return Err(error);
                    }
                    Err(_) => false,
                };
                let is_result = match self.task_payload_return(owner, result, &actual, depth + 1) {
                    Ok(()) => true,
                    Err(error) if error.category == zil_base::ResultCategory::ResourceLimit => {
                        return Err(error);
                    }
                    Err(_) => false,
                };
                if is_error == is_result {
                    return Err(bad(
                        "original Task return injection has no unique declared arm",
                    ));
                }
                let success = if is_error {
                    copy_recipe(result, self.limits, self.left)?
                } else {
                    self.task_payload_return(owner, result, &actual, depth + 1)?;
                    copy_recipe(&actual, self.limits, self.left)?
                };
                let payload = copy_recipe(&actual, self.limits, self.left)?;
                self.pay(1, depth)?;
                self.proof
                    .proof_mut()?
                    .injections
                    .insert((owner, expression.id), Injection { is_error, payload });
                SemanticTypeRecipe::ErrorUnion {
                    error: Box::new(copy_recipe(error, self.limits, self.left)?),
                    result: Box::new(success),
                }
            }
        } else {
            self.task_payload_return(owner, declared, &actual, depth + 1)?;
            actual
        };
        let error_only = self
            .proof
            .injections
            .get(&(owner, expression.id))
            .is_some_and(|row| row.is_error);
        if !error_only {
            if let Some(previous) = self.proof.task_result_types.get(&owner) {
                let previous = copy_recipe(previous, self.limits, self.left)?;
                self.same(&previous, &value)?;
            } else {
                let copy = copy_recipe(&value, self.limits, self.left)?;
                self.pay(1, depth)?;
                self.proof
                    .proof_mut()?
                    .task_result_types
                    .insert(owner, copy);
            }
        }
        if !projected {
            let copy = copy_recipe(&value, self.limits, self.left)?;
            self.proof.proof_mut()?.types.insert((owner, Query::Expression(expression.id)), copy);
        }
        Ok(value)
    }
    pub(super) fn finish_task_result(
        &mut self,
        owner: SemanticDefinitionId,
        depth: usize,
    ) -> R<()> {
        let Some(result) = self.proof.task_result_types.get(&owner) else {
            return Ok(());
        };
        let result = copy_recipe(result, self.limits, self.left)?;
        self.pay(
            self.proof.task_returns.len().saturating_add(1).ilog2() as usize + 1,
            depth,
        )?;
        let nodes: Vec<_> = self
            .proof
            .task_returns
            .range((owner, NodeId(0))..=(owner, NodeId(u32::MAX)))
            .map(|(key, value)| (*key, value.expression))
            .collect();
        self.pay(nodes.len(), depth)?;
        for (key, expression) in nodes {
            let copy = copy_recipe(&result, self.limits, self.left)?;
            self.proof
                .proof_mut()?
                .task_returns
                .get_mut(&key)
                .ok_or_else(|| bad("Task return context lost its exact original exit"))?
                .ty = copy;
            let direct_creation = self.site(owner, SemanticOriginalNode::Expression(expression))?
                .and_then(|site| self.proof.intrinsics.get(&(owner, site.id)))
                .is_some_and(|call| matches!(call.kind,
                    IntrinsicKind::TaskCreate { owned: false, await_result: false, .. }));
            // Keep the physical constructor expression in the worker's
            // coordinates; the Return and Result entries carry the public Type.
            if !direct_creation {
                let copy = copy_recipe(&result, self.limits, self.left)?;
                self.proof.proof_mut()?.types.insert((owner, Query::Expression(expression)), copy);
            }
        }
        self.proof
            .proof_mut()?
            .types
            .insert((owner, Query::Result), result);
        Ok(())
    }
    pub(super) fn effective_result(
        &mut self,
        owner: SemanticDefinitionId,
        declared: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        if let Some(result) = self.proof.task_result_types.get(&owner) {
            return copy_recipe(result, self.limits, self.left);
        }
        self.resolve(owner, declared, depth + 1)
    }
}
