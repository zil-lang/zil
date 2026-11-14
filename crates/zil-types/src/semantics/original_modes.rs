//! Passing modes elaborate the declared resource-name recipe. They never turn
//! an observed descriptor into an owner; creation/transfer syntax must supply it.
use super::*;
impl Check<'_, '_> {
    pub(super) fn mode_type(
        &mut self,
        owner: SemanticDefinitionId,
        mode: PassingMode,
        ty: SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        // Source selects resource categories before allocating description
        // occurrences. Passing modes cannot erase a real descriptor occurrence.
        if matches!(mode, PassingMode::Create | PassingMode::Give) {
            match ty {
                SemanticTypeRecipe::Desc { .. } => {
                    return Err(bad(
                        "create/give requires an owning category, not an explicit descriptor",
                    ));
                }
                SemanticTypeRecipe::ErrorUnion { error, result } => {
                    let result = self.mode_type(owner, mode, *result, depth + 1)?;
                    return Ok(SemanticTypeRecipe::ErrorUnion {
                        error,
                        result: Box::new(result),
                    });
                }
                _ => {}
            }
        }
        Ok(ty)
    }
    pub(super) fn initializer_owns(
        &mut self,
        owner: SemanticDefinitionId,
        initializer: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        match &initializer.kind {
            CoreExprKind::Unary {
                op: UnaryOp::Give, ..
            } => Ok(true),
            CoreExprKind::Record { ty, .. } => {
                let ty = self.resolve(owner, ty, depth + 1)?;
                let nominal = match &ty {
                    SemanticTypeRecipe::Desc { inner, .. } | SemanticTypeRecipe::Managed(inner) => {
                        inner.as_ref()
                    }
                    _ => &ty,
                };
                Ok(
                    matches!(nominal, SemanticTypeRecipe::Nominal { definition, .. } if matches!(self.definition(*definition)?.body, SemanticDefinitionBody::Nominal { kind: LifeNominalKind::Resource, .. })),
                )
            }
            CoreExprKind::Call { callee, .. } => {
                let Some(path) = self.declaration_path(callee, variables, depth + 1)? else {
                    return Ok(false);
                };
                if let Some(target) = self.declaration_target(owner, &path, depth + 1)? {
                    let target = self.definition(target)?;
                    let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &target.body
                    else {
                        return Ok(false);
                    };
                    Ok(matches!(
                        signature.result_mode,
                        PassingMode::Create | PassingMode::Give
                    ) || matches!(&signature.result, SemanticTypeRecipe::Managed(_)))
                } else {
                    Ok(path.len() == 1 && matches!(path[0].as_str(), "store_new" | "bytes_new"))
                }
            }
            CoreExprKind::Group(value) => self.initializer_owns(owner, value, variables, depth + 1),
            CoreExprKind::Ascribed { ty, .. } => {
                let ty = self.resolve(owner, ty, depth + 1)?;
                Ok(matches!(ty, SemanticTypeRecipe::Managed(_)))
            }
            _ => Ok(false),
        }
    }
}
