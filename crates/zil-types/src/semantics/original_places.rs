//! Original availability of directly owned Package fields. This state is local
//! ownership state, and never a type-class or lifetime equality.
use super::*;

#[derive(Clone)]
pub(crate) struct FieldReborrow {
    pub place: calls::MethodReceiverPlace,
    pub source: SemanticTypeRecipe,
    pub result: SemanticTypeRecipe,
}

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
            if !matches!(value, SemanticTypeRecipe::Address { mutable: true, .. }) {
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
        }
        let field = self.place(owner, expression, variables, depth + 1)?;
        let field = self.observation_type(owner, &field, depth + 1)?;
        if let SemanticTypeRecipe::Address {
            mutable: true,
            inner,
            ..
        } = &field
        {
            // Reading a stored exclusive address through a holder creates a
            // child window. It never copies or transfers the stored address.
            let base = self
                .proof
                .types
                .get(&(owner, Query::Expression(object.id)))
                .ok_or_else(|| bad("stored address view loses its original holder"))?;
            let base = copy_recipe(base, self.limits, self.left)?;
            let base = self.observation_type(owner, &base, depth + 1)?;
            if !matches!(base, SemanticTypeRecipe::Address { mutable: true, .. }) {
                if !matches!(base, SemanticTypeRecipe::Nominal { .. }) {
                    return Err(bad("exclusive stored field requires a mutable original holder"));
                }
                self.writable(owner, object, variables, depth + 1)?;
            }
            let place = self
                .method_receiver_place(owner, expression, variables, depth + 1)?
                .ok_or_else(|| {
                    pending("stored address view requires its complete original root path")
                })?;
            let result = SemanticTypeRecipe::Address {
                mutable: true,
                lifetime: self.borrow_life(owner, expression.id)?,
                inner: Box::new(copy_recipe(inner, self.limits, self.left)?),
            };
            let stored_result = copy_recipe(&result, self.limits, self.left)?;
            self.pay(1, depth)?;
            self.proof.proof_mut()?.field_reborrows.insert(
                (owner, expression.id),
                FieldReborrow {
                    place,
                    source: field,
                    result: stored_result,
                },
            );
            return Ok(result);
        }
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
        Ok(self
            .local_inline_field(owner, object, member, variables, read, false, depth)?
            .map(|(field, _)| field))
    }

    pub(super) fn local_inline_field(
        &mut self,
        owner: SemanticDefinitionId,
        object: &SemanticOriginalExpr,
        member: &str,
        variables: &mut BTreeMap<String, Binding>,
        read: bool,
        allow_tuple: bool,
        depth: usize,
    ) -> R<Option<(SemanticTypeRecipe, bool)>> {
        self.pay(member.len(), depth)?;
        let CoreExprKind::Name(name) = &object.kind else {
            return Ok(None);
        };
        self.pay(name.len(), depth)?;
        let Some(binding) = variables.get(name) else {
            return Ok(None);
        };
        let ty = self.observation_type(owner, &binding.ty, depth + 1)?;
        let tuple = match &ty {
            SemanticTypeRecipe::Nominal { definition, .. } if matches!(
                self.definition(*definition)?.body,
                SemanticDefinitionBody::Nominal { kind: LifeNominalKind::Package, .. },
            ) => false,
            SemanticTypeRecipe::Nominal { definition, .. } if member == "inputs" && matches!(
                self.definition(*definition)?.body,
                SemanticDefinitionBody::Nominal { declaration: zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD, .. },
            ) => false,
            SemanticTypeRecipe::Tuple(_) if allow_tuple => true,
            _ => return Ok(None),
        };
        if binding.used || !binding.initialized || read && binding.taken_fields.contains(member) {
            return Err(bad(
                "original inline field is not available at this source point",
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
        self.field(owner, &ty, member, depth + 1)
            .map(|field| Some((field, tuple)))
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
                } else if matches!(field, SemanticTypeRecipe::Address { mutable: true, .. }) {
                    // Moving the whole child transfers this stored address with it.
                    // The field contributes no destructor to the child's owner set.
                    continue;
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
        prepared: &SemanticTypeRecipe,
        evaluated_rvalue: bool,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Group(inner) => {
                self.consume_callable_place(owner, inner, variables, prepared,
                    evaluated_rvalue, depth + 1)
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
            CoreExprKind::Unary { op: UnaryOp::Dereference, .. } => {
                // place() already checked/evaluated the pointer exactly once.
                // Observing its pointee did not transfer it: keep the original
                // independent Copy gate instead of consuming a borrowed owner.
                if !self.copy_type(owner, prepared, depth + 1)? {
                    return Err(bad("original dereference cannot transfer a non-Copy pointee"));
                }
                Ok(())
            }
            _ if evaluated_rvalue => {
                // This exact prepare, not a cached QueryExpression, already
                // executed the rvalue's complete input/capture transfers.
                // Consumption and remaining-environment cleanup are checked
                // by the selected operation; evaluating it again double-moves.
                Ok(())
            }
            _ => Err(bad("callable transfer lacks its actual evaluated preparation")),
        }
    }

    pub(super) fn check_callable_give_argument(
        &mut self,
        owner: SemanticDefinitionId,
        mut argument: &SemanticOriginalExpr,
        actual: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        while let CoreExprKind::Group(inner) = &argument.kind {
            self.pay(1, depth)?;
            argument = inner;
        }
        if !matches!(
            argument.kind,
            CoreExprKind::Unary {
                op: UnaryOp::Give,
                ..
            }
        ) && self.owned_callable_for_give(owner, actual, depth + 1)?
        {
            return Err(bad(
                "original callable Give argument requires the explicit transfer expression",
            ));
        }
        Ok(())
    }

    fn owned_callable_for_give(
        &mut self,
        owner: SemanticDefinitionId,
        value: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        if let Some((kind, _)) = self.concrete_callable(value, depth + 1)? {
            return Ok(kind != CallableKind::FunctionPointer);
        }
        let premises = self.original_requirements(owner, depth + 1)?;
        for requirement in premises.iter() {
            let candidates =
                self.callable_premises(requirement, value, &mut Vec::new(), depth + 1)?;
            if candidates
                .iter()
                .any(|(_, kind, _, _)| *kind != CallableKind::FunctionPointer)
            {
                return Ok(true);
            }
        }
        Ok(false)
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
        if let CoreExprKind::Group(inner) = &operand.kind {
            let actual = self.give_place(owner, inner, variables, depth + 1)?;
            let copy = copy_recipe(&actual, self.limits, self.left)?;
            self.proof
                .proof_mut()?
                .types
                .insert((owner, Query::Expression(operand.id)), copy);
            return Ok(actual);
        }
        if let CoreExprKind::Member { object, member, .. } = &operand.kind {
            if let CoreExprKind::Name(name) = &object.kind {
                if let Some((field, tuple)) =
                    self.local_inline_field(owner, object, member, variables, true, true, depth + 1)?
                {
                    let owns = if let SemanticTypeRecipe::Managed(resource) = &field {
                        self.resource_head(owner, resource, depth + 1)?
                    } else {
                        !tuple && self.owned_package(owner, &field, depth + 1)?
                    };
                    if !owns {
                        return Err(bad("original give requires an owning inline field"));
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
                "original give requires an owning lexical place or a direct owning inline field",
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
        if let SemanticTypeRecipe::Managed(resource) = &head {
            if !self.resource_head(owner, resource, depth + 1)? {
                return Err(bad("original give payload is not a declared resource head"));
            }
        } else if !matches!(head, SemanticTypeRecipe::Address { mutable: true, .. })
            && !self.owned_callable_for_give(owner, &head, depth + 1)?
        {
            return Err(bad(
                "original give requires a managed resource, actual by-value callable, or owned exclusive address",
            ));
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
            CoreExprKind::Call { .. } => {
                self.pay(self.proof.types.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                if let Some(ty) = self.proof.types.get(&(owner, Query::Expression(expression.id))) {
                    let ty = copy_recipe(ty, self.limits, self.left)?;
                    if matches!(self.observation_type(owner, &ty, depth + 1)?, SemanticTypeRecipe::Address { mutable: true, .. }) {
                        return Ok(Some(calls::MethodReceiverPlace { owner,
                            root: SemanticOriginalTypeSource::Expression(expression.id), fields: Vec::new() }));
                    }
                }
                Ok(None)
            }
            CoreExprKind::Member { object, member, .. } => {
                let Some(mut root) =
                    self.method_receiver_place(owner, object, variables, depth + 1)?
                else {
                    return Ok(None);
                };
                let base = if matches!(root.root, SemanticOriginalTypeSource::Expression(_)) {
                    self.pay(self.proof.types.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                    let ty = self.proof.types.get(&(owner, Query::Expression(object.id)))
                        .ok_or_else(|| bad("returned receiver projection lacks its evaluated original parent"))?;
                    copy_recipe(ty, self.limits, self.left)?
                } else { self.place(owner, object, variables, depth + 1)? };
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
