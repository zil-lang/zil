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
        let left = self.normalized_task_shape(owner, left, depth + 1)?;
        let right = self.normalized_task_shape(owner, right, depth + 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&left, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(&right, self.limits, self.left, 1)?;
        if left != right {
            self.pay(384, depth)?;
            let mut error =
                bad("original Task changes its complete input Context identity or projection");
            error.notes.push(format!("original Task Context: domain {}/{}, ports {}/{}, retained {}/{}, nominal {}/{}; exact tables equal {}/{}/{}/{}",
                left.domain.parts.len(), right.domain.parts.len(), left.ports.len(), right.ports.len(),
                left.retained.len(), right.retained.len(), left.nominal.len(), right.nominal.len(),
                left.domain == right.domain, left.ports == right.ports, left.retained == right.retained, left.nominal == right.nominal));
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
    pub(super) fn task_return_value(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        declared: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let actual = self.expr(owner, expression, variables, None, depth + 1)?;
        let value = if let SemanticTypeRecipe::ErrorUnion { error, result } = declared {
            if let SemanticTypeRecipe::ErrorUnion {
                error: actual_error,
                result: actual_result,
            } = &actual
            {
                self.same(actual_error, error)?;
                self.task_payload_return(owner, result, actual_result, depth + 1)?;
                actual
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
        let copy = copy_recipe(&value, self.limits, self.left)?;
        self.proof
            .proof_mut()?
            .types
            .insert((owner, Query::Expression(expression.id)), copy);
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
            let copy = copy_recipe(&result, self.limits, self.left)?;
            self.proof
                .proof_mut()?
                .types
                .insert((owner, Query::Expression(expression)), copy);
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
