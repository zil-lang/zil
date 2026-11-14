//! Static C callback formation has no runtime argument evaluation.
use super::*;

impl Check<'_, '_> {
    pub(super) fn callback_intrinsic(
        &mut self,
        owner: SemanticDefinitionId,
        site: SemanticSiteId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        wanted: Option<&SemanticTypeRecipe>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        let CoreExprKind::Call { callee, arguments } = &expression.kind else {
            unreachable!()
        };
        let path = self.declaration_path(callee, variables, depth + 1)?;
        if path.is_none()
            || self
                .declaration_target(owner, path.as_ref().unwrap(), depth + 1)?
                .is_some()
        {
            return Err(bad(
                "reserved c_callback cannot be shadowed by a value or declaration",
            ));
        }
        let [operand] = arguments.as_slice() else {
            return Err(bad(
                "c_callback requires exactly one static ordinary function symbol",
            ));
        };
        // Generic application and runtime expressions are not static callback symbols.
        if !matches!(
            operand.kind,
            CoreExprKind::Name(_) | CoreExprKind::Member { .. }
        ) {
            return Err(bad(
                "c_callback requires a nongeneric static ordinary function symbol",
            ));
        }
        let path = self
            .declaration_path(operand, variables, depth + 1)?
            .ok_or_else(|| bad("c_callback target is a runtime value or capture"))?;
        let target = self
            .declaration_target(owner, &path, depth + 1)?
            .ok_or_else(|| bad("c_callback target has no exact original declaration"))?;
        let definition = self.definition(target)?;
        if definition.kind != SemanticDefinitionKind::Function
            || definition.parent.is_some()
            || !definition.environment.is_empty()
            || !definition.parameters.is_empty()
        {
            return Err(bad(
                "c_callback target must be an ordinary environment-free nongeneric function",
            ));
        }
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            body: SemanticOriginalFunctionBody::Defined(_),
            foreign: None,
            receiver_parameter: None,
            protocol_contracts,
            ..
        } = &definition.body
        else {
            return Err(bad(
                "c_callback target must have a real ordinary defined body",
            ));
        };
        self.pay(signature.parameters.len(), depth)?;
        if signature.bound_lifetimes != 0
            || signature.invocation_lifetimes != 0
            || signature.call_kind != CallKind::Direct
            || !protocol_contracts.is_empty()
            || !matches!(
                signature.result_mode,
                PassingMode::Infer | PassingMode::Copy
            )
            || signature
                .parameters
                .iter()
                .any(|p| !matches!(p.mode, PassingMode::Infer | PassingMode::Copy))
        {
            return Err(bad(
                "c_callback target changes the finite direct C ABI or carries a language protocol",
            ));
        }
        if !self.proof.complete.contains(&target) && !self.active.contains(&target) {
            self.function(definition, None)?;
        }
        self.call_requirements(owner, definition, &[], depth + 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(signature, self.limits, self.left, 1)?;
        let mut signature = signature.clone();
        self.pay(signature.parameters.len(), depth)?;
        for parameter in &mut signature.parameters {
            parameter.ty = self.resolve(target, &parameter.ty, depth + 1)?;
            if !callback_scalar_recipe(&parameter.ty, false) {
                return Err(bad(
                    "c_callback parameters require exact integers, f32, f64 or c_handle; Bool is not a C scalar",
                ));
            }
        }
        signature.result = self.resolve(target, &signature.result, depth + 1)?;
        if !callback_scalar_recipe(&signature.result, true) {
            return Err(bad("c_callback result requires an exact C scalar or Unit"));
        }
        let result = SemanticTypeRecipe::Code {
            kind: SemanticCodeKind::ForeignCallback,
            signature: Box::new(signature),
        };
        self.record_intrinsic(
            owner,
            site,
            IntrinsicKind::ForeignCallback { target },
            Vec::new(),
            &result,
            wanted,
            depth + 1,
        )?;
        Ok(result)
    }
}
fn callback_scalar_recipe(recipe: &SemanticTypeRecipe, result: bool) -> bool {
    matches!(recipe, SemanticTypeRecipe::Atom(ty) if crate::foreign::callback_scalar(ty) || result && *ty == Type::Unit)
}
