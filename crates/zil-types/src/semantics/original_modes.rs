//! Passing modes elaborate the declared resource-name recipe. They never turn
//! an observed descriptor into an owner; creation/transfer syntax must supply it.
use super::*;
impl Check<'_, '_> {
    pub(super) fn mode_type(
        &mut self,
        owner: SemanticDefinitionId,
        mode: PassingMode,
        mut ty: SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        if matches!(mode, PassingMode::Create | PassingMode::Give) {
            match ty {
                SemanticTypeRecipe::Desc { inner, .. } => ty = SemanticTypeRecipe::Managed(inner),
                SemanticTypeRecipe::ErrorUnion { error, result } if mode == PassingMode::Create => {
                    ty = SemanticTypeRecipe::ErrorUnion {
                        error,
                        result: Box::new(self.mode_type(owner, mode, *result, depth + 1)?),
                    };
                }
                _ => {}
            }
        }
        if matches!(mode, PassingMode::Borrow | PassingMode::BorrowMut) {
            if let SemanticTypeRecipe::Address { inner, .. } = &mut ty {
                if let SemanticTypeRecipe::Desc {
                    inner: resource, ..
                } = inner.as_ref()
                {
                    if self.resource_head(owner, resource, depth + 1)? {
                        let resource = copy_recipe(resource, self.limits, self.left)?;
                        *inner = Box::new(SemanticTypeRecipe::Managed(Box::new(resource)));
                    }
                }
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
                    ))
                } else {
                    Ok(path.len() == 1 && matches!(path[0].as_str(), "store_new" | "bytes_new"))
                }
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                self.initializer_owns(owner, value, variables, depth + 1)
            }
            _ => Ok(false),
        }
    }
}
