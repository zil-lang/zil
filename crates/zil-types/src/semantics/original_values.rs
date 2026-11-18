//! Static function values are resolved in the original lexical namespace.
//! Their targets and complete schemes are independent of Typed specialization.
use super::*;

#[derive(Clone)]
pub(crate) struct FunctionValue {
    pub target: SemanticDefinitionId,
    pub actuals: Vec<SemanticActualRecipe>,
    pub kind: SemanticCodeKind,
    pub signature: SemanticSignatureRecipe,
}
impl Check<'_, '_> {
    /// A thin function context supplies types to the child's own binders.
    /// Captures and the checked result are validated after its body runs.
    pub(super) fn contextual_code_inputs(
        &mut self,
        owner: SemanticDefinitionId,
        child: &SemanticDefinition,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<()> {
        let Some(SemanticTypeRecipe::Code { kind: SemanticCodeKind::FunctionPointer, signature }) = wanted else {
            return Ok(());
        };
        let SemanticDefinitionBody::FunctionWithBody { signature: declared, .. } = &child.body else {
            return Err(bad("thin closure has no original body signature"));
        };
        if child.parent != Some(owner) || signature.parameters.len() != declared.parameters.len()
            || signature.call_kind != CallKind::Direct || signature.bound_lifetimes != 0 || signature.invocation_lifetimes != 0
        {
            return Err(bad("thin closure context changes its original arity, calling mode or lifetime domain"));
        }
        for (index, (input, parameter)) in signature.parameters.iter().zip(&declared.parameters).enumerate() {
            let ordinal = u32::try_from(index).map_err(|_| bad("thin closure parameter ordinal overflow"))?;
            let form = &parameter.ty;
            if matches!(form, SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Parameter(n),
            } if *n == ordinal) {
                let expected = self.rehome(owner, child, &input.ty, depth + 1)?;
                self.contextual_code_equation(child.id, Query::Parameter(ordinal), expected, depth + 1)?;
            }
        }
        if matches!(declared.result, SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result,
        }) {
            let expected = self.rehome(owner, child, &signature.result, depth + 1)?;
            self.contextual_code_equation(child.id, Query::Result, expected, depth + 1)?;
        }
        Ok(())
    }

    fn contextual_code_equation(&mut self, child: SemanticDefinitionId, query: Query,
        expected: SemanticTypeRecipe, depth: usize) -> R<()> {
        self.pay(query.copy_work().saturating_add(self.proof.types.len().saturating_add(1).ilog2() as usize + 1), depth)?;
        if let Some(old) = self.proof.types.get(&(child, query.clone())) {
            let old = copy_recipe(old, self.limits, self.left)?;
            self.contexts.push(child);
            let equal = self.same(&old, &expected);
            self.contexts.pop();
            equal?;
        } else {
            self.proof.proof_mut()?.types.insert((child, query), expected);
        }
        Ok(())
    }

    pub(super) fn checked_code_closure(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        child: &SemanticDefinition,
        expected: &SemanticSignatureRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        if !self.captures.get(&child.id).is_some_and(|captures| captures.is_empty()) {
            return Err(bad("function pointer conversion requires a closure with no captures"));
        }
        if self.function_value_copy(owner, SemanticCodeKind::FunctionPointer, expected, depth + 1)? != Some(true) {
            return Err(bad("thin closure requires its complete original Copy signature"));
        }
        for (index, value) in expected.parameters.iter().map(|p| &p.ty).chain(std::iter::once(&expected.result)).enumerate() {
            let query = if index == expected.parameters.len() { Query::Result } else { Query::Parameter(index as u32) };
            let value = self.rehome(owner, child, value, depth + 1)?;
            let actual = self.proof.types.get(&(child.id, query))
                .ok_or_else(|| bad("thin closure omits a checked original parameter or result"))?;
            let actual = copy_recipe(actual, self.limits, self.left)?;
            self.contexts.push(child.id);
            let equal = self.same(&actual, &value);
            self.contexts.pop();
            equal?;
        }
        crate::dyn_metadata::semantic_operand_snapshot(expected, self.limits, self.left, 1)?;
        self.pay(1, depth)?;
        self.proof.proof_mut()?.function_values.insert((owner, node), FunctionValue {
            target: child.id, actuals: Vec::new(), kind: SemanticCodeKind::FunctionPointer, signature: expected.clone(),
        });
        crate::dyn_metadata::semantic_operand_snapshot(expected, self.limits, self.left, 1)?;
        Ok(SemanticTypeRecipe::Code { kind: SemanticCodeKind::FunctionPointer, signature: Box::new(expected.clone()) })
    }
    pub(super) fn fn_value(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(1, depth)?;
        if !matches!(
            expression.kind,
            CoreExprKind::Name(_)
                | CoreExprKind::Member { .. }
                | CoreExprKind::GenericApply { .. }
                | CoreExprKind::TypeApply { .. }
        ) {
            return Ok(None);
        }
        let Some(path) = self.declaration_path(expression, variables, depth + 1)? else {
            return Ok(None);
        };
        let Some(target) = self.declaration_target(owner, &path, depth + 1)? else {
            return Ok(None);
        };
        let definition = self.definition(target)?;
        if definition.kind != SemanticDefinitionKind::Function {
            return Ok(None);
        }
        if !definition.environment.is_empty() {
            return Err(bad(
                "static function value captures an original environment",
            ));
        }
        if !self.proof.complete.contains(&target) && !self.active.contains(&target) {
            self.function(definition, None)?;
        }
        let SemanticDefinitionBody::FunctionWithBody {
            signature, body, ..
        } = &definition.body
        else {
            return Err(bad(
                "function value omits its complete original declaration",
            ));
        };
        let foreign = matches!(body, SemanticOriginalFunctionBody::External);
        if !matches!(
            body,
            SemanticOriginalFunctionBody::Defined(_) | SemanticOriginalFunctionBody::External
        ) {
            return Err(bad("function value has no actual executable declaration"));
        }
        if signature.call_kind != CallKind::Direct {
            return Err(pending(
                "function value needs a code type preserving its fixed Suspend ABI",
            ));
        }
        let written = self.written(
            owner,
            expression,
            &definition.parameters,
            variables,
            depth + 1,
        )?;
        self.pay(written.len(), depth)?;
        let actuals = written
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                pending("generic function value requires all original mixed actuals explicitly")
            })?;
        self.call_requirements(owner, definition, &actuals, depth + 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(signature, self.limits, self.left, 1)?;
        let mut signature = signature.clone();
        for parameter in &mut signature.parameters {
            let ty = self.resolve(target, &parameter.ty, depth + 1)?;
            parameter.ty = self.mode_type(target, parameter.mode, ty, depth + 1)?;
        }
        let result = self.effective_result(target, &signature.result, depth + 1)?;
        signature.result = self.mode_type(target, signature.result_mode, result, depth + 1)?;
        let kind = if foreign {
            SemanticCodeKind::ForeignCallback
        } else if matches!(
            wanted,
            Some(SemanticTypeRecipe::Code {
                kind: SemanticCodeKind::BorrowedFunctionPointer,
                ..
            })
        ) || signature
            .parameters
            .iter()
            .any(|parameter| matches!(parameter.ty, SemanticTypeRecipe::Address { .. }))
            || matches!(signature.result, SemanticTypeRecipe::Address { .. })
        {
            SemanticCodeKind::BorrowedFunctionPointer
        } else {
            SemanticCodeKind::FunctionPointer
        };
        let recipe = SemanticTypeRecipe::Code {
            kind,
            signature: Box::new(signature),
        };
        let recipe = super::super::instantiate::ty(
            self.program,
            &self.proof,
            &recipe,
            super::super::instantiate::DomainMapping {
                source: target,
                use_definition: owner,
                arguments: &actuals,
                parent_call: None,
            },
            self.limits,
            self.left,
        )?;
        let SemanticTypeRecipe::Code { kind, signature } = recipe else {
            unreachable!()
        };
        if self.function_value_copy(owner, kind, &signature, depth + 1)? != Some(true) {
            return Err(bad(
                "function value lacks its complete original code Copy scheme",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(
            signature.as_ref(),
            self.limits,
            self.left,
            1,
        )?;
        let result = SemanticTypeRecipe::Code {
            kind,
            signature: signature.clone(),
        };
        if let Some(wanted) = wanted {
            self.same(&result, wanted)?;
        }
        self.pay(1, depth)?;
        self.proof.proof_mut()?.function_values.insert(
            (owner, expression.id),
            FunctionValue {
                target,
                actuals,
                kind,
                signature: *signature,
            },
        );
        Ok(Some(result))
    }
    pub(super) fn function_value_copy(
        &mut self,
        owner: SemanticDefinitionId,
        kind: SemanticCodeKind,
        signature: &SemanticSignatureRecipe,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(1, depth)?;
        if kind == SemanticCodeKind::BorrowedFunctionPointer
            || kind == SemanticCodeKind::ForeignCallback
        {
            // Copy duplicates only the code address. The complete invocation
            // and C ABI eligibility are separate, mandatory consumers.
            return Ok(Some(true));
        }
        if signature.call_kind != CallKind::Direct {
            return Ok(Some(false));
        }
        self.pay(signature.parameters.len(), depth)?;
        for parameter in &signature.parameters {
            if !self.copy_type(owner, &parameter.ty, depth + 1)? {
                return Ok(Some(false));
            }
        }
        Ok(Some(self.copy_type(owner, &signature.result, depth + 1)?))
    }
}
