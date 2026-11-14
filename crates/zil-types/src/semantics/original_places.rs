//! Original availability of directly owned Package fields. This state is local
//! ownership state, and never a type-class or lifetime equality.
use super::*;

impl Check<'_, '_> {
    pub(super) fn package_field(
        &mut self,
        owner: SemanticDefinitionId,
        object: &SemanticOriginalExpr,
        member: &str,
        variables: &mut BTreeMap<String, Binding>,
        read: bool,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(member.len(), depth)?;
        let CoreExprKind::Name(name) = &object.kind else {
            return Ok(None);
        };
        self.pay(name.len(), depth)?;
        let Some(binding) = variables.get(name) else {
            return Ok(None);
        };
        let ty = self.observation_type(owner, &binding.ty, depth + 1)?;
        let SemanticTypeRecipe::Nominal { definition, .. } = &ty else {
            return Ok(None);
        };
        if !matches!(
            self.definition(*definition)?.body,
            SemanticDefinitionBody::Nominal {
                kind: LifeNominalKind::Package,
                ..
            }
        ) {
            return Ok(None);
        }
        if binding.used || !binding.initialized || read && binding.taken_fields.contains(member) {
            return Err(bad(
                "original package field is not available at this source point",
            ));
        }
        self.snapshot(
            owner,
            SemanticOriginalNode::Expression(object.id),
            variables,
        )?;
        let original = variables
            .get(name)
            .ok_or_else(|| bad("original package place loses its binder"))?;
        let copy = copy_recipe(&original.ty, self.limits, self.left)?;
        self.proof
            .types
            .insert((owner, Query::Expression(object.id)), copy);
        variables
            .get_mut(name)
            .ok_or_else(|| bad("original package place loses its binder"))?
            .observed = true;
        self.field(owner, &ty, member, depth + 1).map(Some)
    }

    fn owned_package(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(self.predicate_active.len().saturating_add(1), depth)?;
        let ty = self.observation_type(owner, ty, depth + 1)?;
        if self
            .predicate_active
            .iter()
            .any(|(domain, kind, value)| *domain == owner && *kind == 9 && value == &ty)
        {
            return Err(bad(
                "original owning Package has a recursive physical field cycle",
            ));
        }
        let SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } = &ty
        else {
            return Ok(false);
        };
        let definition = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            kind: LifeNominalKind::Package,
            body: SemanticNominalBody::Product { fields },
        } = &definition.body
        else {
            return Ok(false);
        };
        self.pay(fields.len(), depth)?;
        let mut field_types = Vec::new();
        for field in fields {
            field_types.push(self.instantiate_type(&field.ty, arguments, depth + 1)?);
        }
        let active = copy_recipe(&ty, self.limits, self.left)?;
        self.predicate_active.push((owner, 9, active));
        let answer = (|| {
            let mut owns = false;
            for field in field_types {
                if let SemanticTypeRecipe::Managed(resource) = &field {
                    if !self.resource_head(owner, resource, depth + 1)? {
                        return Ok(false);
                    }
                    owns = true;
                } else if self.copy_type(owner, &field, depth + 1)? {
                    continue;
                } else if self.owned_package(owner, &field, depth + 1)? {
                    owns = true;
                } else {
                    return Ok(false);
                }
            }
            Ok(owns)
        })();
        self.predicate_active.pop();
        answer
    }

    pub(super) fn give_place(
        &mut self,
        owner: SemanticDefinitionId,
        operand: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        if let CoreExprKind::Member { object, member, .. } = &operand.kind {
            if let CoreExprKind::Name(name) = &object.kind {
                if let Some(field) =
                    self.package_field(owner, object, member, variables, true, depth + 1)?
                {
                    let owns = if let SemanticTypeRecipe::Managed(resource) = &field {
                        self.resource_head(owner, resource, depth + 1)?
                    } else {
                        self.owned_package(owner, &field, depth + 1)?
                    };
                    if !owns {
                        return Err(bad("original give requires an owning Package field"));
                    }
                    let binding = variables
                        .get_mut(name)
                        .ok_or_else(|| bad("original give loses its owning Package binder"))?;
                    if binding.borrowed {
                        return Err(bad(
                            "original give moves a Package field from a borrowed capture",
                        ));
                    }
                    self.pay(member.len().saturating_add(1), depth)?;
                    binding.taken_fields.insert(member.clone());
                    return Ok(field);
                }
            }
        }
        let CoreExprKind::Name(name) = &operand.kind else {
            return Err(pending(
                "original give requires a managed lexical place or a direct owning Package field",
            ));
        };
        let value = variables
            .get_mut(name)
            .ok_or_else(|| bad("original give has no lexical binder"))?;
        if value.used || !value.initialized || value.borrowed || !value.taken_fields.is_empty() {
            return Err(bad(
                "original give transfers an unavailable or borrowed value",
            ));
        }
        let source = copy_recipe(&value.ty, self.limits, self.left)?;
        let head = self.observation_type(owner, &source, depth + 1)?;
        let SemanticTypeRecipe::Managed(resource) = &head else {
            return Err(bad("original give requires an owning resource head"));
        };
        if !self.resource_head(owner, resource, depth + 1)? {
            return Err(bad("original give payload is not a declared resource head"));
        }
        value.used = true;
        value.observed = true;
        Ok(head)
    }
}
