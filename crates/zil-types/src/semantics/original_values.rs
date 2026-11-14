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
        let result = self.resolve(target, &signature.result, depth + 1)?;
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
