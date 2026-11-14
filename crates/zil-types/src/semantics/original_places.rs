//! Original availability of directly owned Package fields. This state is local
//! ownership state, and never a type-class or lifetime equality.
use super::*;

impl Check<'_, '_> {
    /// Ordinary affine fields of a directly owned Package transfer their own
    /// responsibility. A resource-head field remains an observation unless an
    /// explicit Give selects it; a projection through a borrowed owner cannot
    /// acquire an affine field merely because its type happens to match.
    pub(super) fn observe_member(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let CoreExprKind::Member { object, member, .. } = &expression.kind else {
            return Err(bad("original member observation changes its node kind"));
        };
        if let Some(field) =
            self.package_field(owner, object, member, variables, true, depth + 1)?
        {
            let value = self.observation_type(owner, &field, depth + 1)?;
            if !matches!(
                value,
                SemanticTypeRecipe::Managed(_) | SemanticTypeRecipe::Desc { .. }
            ) && !self.copy_type(owner, &value, depth + 1)?
            {
                let CoreExprKind::Name(name) = &object.kind else {
                    unreachable!()
                };
                let binding = variables.get_mut(name).ok_or_else(|| {
                    bad("original Package field transfer loses its owning binder")
                })?;
                if binding.borrowed {
                    return Err(bad(
                        "original affine field transfer uses a borrowed capture",
                    ));
                }
                self.pay(member.len().saturating_add(1), depth)?;
                binding.taken_fields.insert(member.clone());
                self.observe_capture(owner, name, binding, depth + 1)?;
                return Ok(value);
            }
            return self.observe_field(owner, expression, value, depth + 1);
        }
        let field = self.place(owner, expression, variables, depth + 1)?;
        self.observe_field(owner, expression, field, depth + 1)
    }
    pub(super) fn observe_field(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        field: SemanticTypeRecipe,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        let field = self.observation_type(owner, &field, depth + 1)?;
        if let SemanticTypeRecipe::Managed(inner) = &field {
            if !self.resource_head(owner, inner, depth + 1)? {
                return Err(bad("original managed field has no true resource payload"));
            }
            let lifetime = self.borrow_life(owner, expression.id)?;
            self.pay(1, depth)?;
            return Ok(SemanticTypeRecipe::Desc {
                lifetime,
                inner: Box::new(copy_recipe(inner, self.limits, self.left)?),
            });
        }
        if matches!(field, SemanticTypeRecipe::Desc { .. })
            || self.copy_type(owner, &field, depth + 1)?
        {
            return Ok(field);
        }
        Err(pending(
            "original affine field observation needs its explicit view/transfer rule",
        ))
    }

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
            .proof_mut()?
            .types
            .insert((owner, Query::Expression(object.id)), copy);
        let binding = variables
            .get_mut(name)
            .ok_or_else(|| bad("original package place loses its binder"))?;
        binding.observed = true;
        self.observe_capture(owner, name, binding, depth + 1)?;
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
            ..
        } = &definition.body
        else {
            return Ok(false);
        };
        self.pay(fields.len(), depth)?;
        let mut field_types = Vec::new();
        for field in fields {
            field_types.push(self.instantiate_field(
                owner,
                definition.id,
                &field.ty,
                arguments,
                depth + 1,
            )?);
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

    /// Invoke a proved FnOnce role by transferring exactly its owning place.
    /// Field transfers leave the remaining Package inventory available.
    pub(super) fn consume_callable_place(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Group(inner) => {
                self.consume_callable_place(owner, inner, variables, depth + 1)
            }
            CoreExprKind::Name(_) => {
                self.expr(owner, expression, variables, None, depth + 1)?;
                Ok(())
            }
            CoreExprKind::Member { object, member, .. } => {
                let CoreExprKind::Name(name) = &object.kind else {
                    return Err(pending(
                        "FnOnce projection needs its complete owning place path",
                    ));
                };
                let Some(field) =
                    self.package_field(owner, object, member, variables, true, depth + 1)?
                else {
                    return Err(bad(
                        "FnOnce field transfer requires its owning Package binder",
                    ));
                };
                let copy = self.copy_type(owner, &field, depth + 1)?;
                let binding = variables.get_mut(name).ok_or_else(|| {
                    bad("FnOnce field transfer loses its original Package binder")
                })?;
                if !copy && binding.borrowed {
                    return Err(bad(
                        "FnOnce field transfer consumes a borrowed Package field",
                    ));
                }
                if !copy {
                    self.pay(member.len().saturating_add(1), depth)?;
                    binding.taken_fields.insert(member.clone());
                }
                self.observe_capture(owner, name, binding, depth + 1)?;
                Ok(())
            }
            _ => {
                // Rvalues still undergo their complete original expression
                // typing; no field or owner is inferred from a matching type.
                self.expr(owner, expression, variables, None, depth + 1)?;
                Ok(())
            }
        }
    }

    pub(super) fn give_place(
        &mut self,
        owner: SemanticDefinitionId,
        operand: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        self.snapshot(
            owner,
            SemanticOriginalNode::Expression(operand.id),
            variables,
        )?;
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
                    self.observe_capture(owner, name, binding, depth + 1)?;
                    let copy = copy_recipe(&field, self.limits, self.left)?;
                    self.proof
                        .proof_mut()?
                        .types
                        .insert((owner, Query::Expression(operand.id)), copy);
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
        self.observe_capture(owner, name, value, depth + 1)?;
        let copy = copy_recipe(&head, self.limits, self.left)?;
        self.proof
            .proof_mut()?
            .types
            .insert((owner, Query::Expression(operand.id)), copy);
        Ok(head)
    }
}

impl Check<'_, '_> {
    pub(super) fn method_receiver_place(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &mut BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<Option<calls::MethodReceiverPlace>> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Group(inner) => {
                self.method_receiver_place(owner, inner, variables, depth + 1)
            }
            CoreExprKind::Name(name) => {
                let binding = variables
                    .get(name)
                    .ok_or_else(|| bad("method receiver loses its original lexical binder"))?;
                if binding.origin_owner != owner {
                    return Ok(None);
                }
                self.pay(binding.origin.copy_work(), depth)?;
                let root = match &binding.origin {
                    Query::Parameter(ordinal) => SemanticOriginalTypeSource::Parameter(*ordinal),
                    Query::Binding(node) => SemanticOriginalTypeSource::Binding(*node),
                    Query::BindingProjection { statement, pattern } => {
                        SemanticOriginalTypeSource::BindingProjection {
                            statement: *statement,
                            pattern: pattern.clone(),
                        }
                    }
                    _ => return Ok(None),
                };
                Ok(Some(calls::MethodReceiverPlace {
                    owner,
                    root,
                    fields: Vec::new(),
                }))
            }
            CoreExprKind::Member { object, member, .. } => {
                let Some(mut root) =
                    self.method_receiver_place(owner, object, variables, depth + 1)?
                else {
                    return Ok(None);
                };
                let base = self.place(owner, object, variables, depth + 1)?;
                let mut base = self.observation_type(owner, &base, depth + 1)?;
                loop {
                    self.pay(1, depth)?;
                    match base {
                        SemanticTypeRecipe::Address { inner, .. }
                        | SemanticTypeRecipe::Desc { inner, .. }
                        | SemanticTypeRecipe::Managed(inner) => base = *inner,
                        _ => break,
                    }
                }
                let SemanticTypeRecipe::Nominal { definition, .. } = base else {
                    return Ok(None);
                };
                let declaration = self.definition(definition)?;
                let SemanticDefinitionBody::Nominal {
                    body: SemanticNominalBody::Product { fields },
                    ..
                } = &declaration.body
                else {
                    return Ok(None);
                };
                self.pay(fields.len().saturating_add(member.len()), depth)?;
                let ordinal = fields
                    .iter()
                    .position(|field| field.name == *member)
                    .ok_or_else(|| {
                        bad("method receiver field is absent from its real original declaration")
                    })?;
                self.pay(root.fields.len().saturating_add(1), depth)?;
                root.fields.push((definition, ordinal as u32));
                Ok(Some(root))
            }
            _ => Ok(None),
        }
    }
}
