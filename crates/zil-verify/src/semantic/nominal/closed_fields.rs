//! Decide which original field recipes are functions of the containing
//! declaration's checked, ordered, closed type arguments. For a nested
//! nominal every structural obligation must already be one of those checked
//! on the same argument of the parent. This judgement only qualifies a
//! previously completed physical inventory selection.
use super::*;

pub(super) fn independent(
    context: &CheckedContext,
    module: &TypedModule,
    root: &SemanticDefinition,
    body: &SemanticNominalBody,
    b: &mut Budget,
) -> Check<bool> {
    b.spend(root.parameters.len().saturating_add(1))?;
    let formals: Vec<usize> = (0..root.parameters.len()).collect();
    let mut proof = Fields {
        context,
        module,
        root,
        active: vec![root.id],
    };
    proof.body(body, &formals, 0, b)
}

struct Fields<'a> {
    context: &'a CheckedContext,
    module: &'a TypedModule,
    root: &'a SemanticDefinition,
    active: Vec<SemanticDefinitionId>,
}

fn local_formal(recipe: &SemanticTypeRecipe) -> Option<usize> {
    match recipe {
        SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        }) => Some(*ordinal as usize),
        _ => None,
    }
}

fn requirements_implied(
    child_ordinal: usize,
    root_ordinal: usize,
    required: &[SemanticRequirementRecipe],
    established: &[SemanticRequirementRecipe],
    b: &mut Budget,
) -> Check<bool> {
    for rule in required {
        let SemanticRequirementRecipe::Structural { ty, bound } = rule else {
            return Ok(false);
        };
        b.spend(established.len().saturating_add(1))?;
        if local_formal(ty) != Some(child_ordinal)
            || !established.iter().any(|role| {
                matches!(role, SemanticRequirementRecipe::Structural { ty, bound: actual }
                if local_formal(ty) == Some(root_ordinal) && actual == bound)
            })
        {
            return Ok(false);
        }
    }
    Ok(true)
}

impl Fields<'_> {
    fn body(
        &mut self,
        body: &SemanticNominalBody,
        formals: &[usize],
        depth: usize,
        b: &mut Budget,
    ) -> Check<bool> {
        match body {
            SemanticNominalBody::Product { fields } => {
                for field in fields {
                    if !self.field(&field.ty, formals, depth, b)? {
                        return Ok(false);
                    }
                }
            }
            SemanticNominalBody::Enum { variants } => {
                b.spend(variants.len())?;
                for variant in variants {
                    for field in &variant.fields {
                        if !self.field(field, formals, depth, b)? {
                            return Ok(false);
                        }
                    }
                }
            }
        }
        Ok(true)
    }
    fn field(
        &mut self,
        recipe: &SemanticTypeRecipe,
        formals: &[usize],
        depth: usize,
        b: &mut Budget,
    ) -> Check<bool> {
        b.spend(1)?;
        if depth >= b.limits.max_call_depth.min(128) {
            return Ok(false);
        }
        match recipe {
            SemanticTypeRecipe::Atom(ty) => Ok(ty.is_b0_scalar()),
            SemanticTypeRecipe::Formal(_) => {
                Ok(local_formal(recipe).is_some_and(|i| i < formals.len()))
            }
            SemanticTypeRecipe::Managed(inner) | SemanticTypeRecipe::OwnedStore(inner) => {
                self.field(inner, formals, depth + 1, b)
            }
            SemanticTypeRecipe::Tuple(children) => {
                for child in children {
                    if !self.field(child, formals, depth + 1, b)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            SemanticTypeRecipe::ErrorUnion { error, result } => {
                Ok(self.field(error, formals, depth + 1, b)?
                    && self.field(result, formals, depth + 1, b)?)
            }
            SemanticTypeRecipe::Nominal {
                definition,
                arguments,
            } => {
                b.spend(self.active.len().saturating_add(arguments.len()))?;
                if self.active.contains(definition) {
                    return Ok(false);
                }
                let child = self.context.definition(self.module, *definition, b)?;
                let SemanticDefinitionBody::Nominal {
                    declaration, body, ..
                } = &child.body
                else {
                    return Ok(false);
                };
                if child.parent.is_some()
                    || !child.environment.is_empty()
                    || !child.implicit_lifetimes.is_empty()
                    || !child.requirements.is_empty()
                    || arguments.len() != child.parameters.len()
                {
                    return Ok(false);
                }
                let mut mapped = Vec::with_capacity(arguments.len());
                for (i, (actual, formal)) in arguments.iter().zip(&child.parameters).enumerate() {
                    let (SemanticActualRecipe::Type(ty), SemanticFormal::Type { requirements }) =
                        (actual, formal)
                    else {
                        return Ok(false);
                    };
                    let Some(&root_ordinal) = local_formal(ty).and_then(|j| formals.get(j)) else {
                        return Ok(false);
                    };
                    let Some(SemanticFormal::Type {
                        requirements: outer,
                    }) = self.root.parameters.get(root_ordinal)
                    else {
                        return Ok(false);
                    };
                    if !requirements_implied(i, root_ordinal, requirements, outer, b)? {
                        return Ok(false);
                    }
                    mapped.push(root_ordinal);
                }
                if !dynamic_metadata::nominal_scope_instances(self.module, child.id, b)?.is_empty()
                {
                    return Ok(false);
                }
                let alternatives =
                    dynamic_metadata::nominal_instances(self.module, *declaration, b)?;
                if !super::closed::closed_inventory(&alternatives, b)? {
                    return Ok(false);
                }
                self.active.push(*definition);
                let result = self.body(body, &mapped, depth + 1, b);
                self.active.pop();
                result
            }
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composed_nominal_requires_the_same_obligation_on_the_mapped_formal() {
        let mut b = Budget::new(&Limits::default());
        let requirement = |ordinal, bound| SemanticRequirementRecipe::Structural {
            ty: SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }),
            bound,
        };
        let copy = DynStructuralBound::Copy;
        let store = DynStructuralBound::StoreElement;
        let child = [requirement(1, copy)];
        assert!(requirements_implied(1, 0, &child, &[requirement(0, copy)], &mut b).unwrap());
        assert!(!requirements_implied(1, 0, &child, &[requirement(1, copy)], &mut b).unwrap());
        assert!(!requirements_implied(1, 0, &child, &[requirement(0, store)], &mut b).unwrap());
        assert!(!requirements_implied(0, 1, &child, &[requirement(1, copy)], &mut b).unwrap());
    }
}
