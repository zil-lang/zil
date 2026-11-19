//! A result boundary is selected by original operation provenance. Hidden
//! recipes retain their callee lexical domain until the actual call is checked.
use super::*;

#[derive(Clone)]
pub(crate) struct CalleeResultBoundary {
    pub caller: SemanticDefinitionId,
    pub call: SemanticSiteId,
    pub callee: SemanticDefinitionId,
    pub actuals: Vec<SemanticActualRecipe>,
    /// Fully solved operands in the original caller domain. These transport
    /// only the exact call's original parameter occurrences.
    pub parameters: Vec<SemanticParameterRecipe>,
    /// Complete all-return result in the original callee definition domain.
    pub result: SemanticTypeRecipe,
}

impl Check<'_, '_> {
    /// The hidden equation belongs exclusively to this completed definition.
    /// It can be read in its original Gamma, never renamed into a caller's
    /// same-numbered implicit or environment coordinates.
    pub(super) fn original_opaque_result(
        &mut self, definition: SemanticDefinitionId, hidden: &SemanticTypeRecipe, depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let declaration = self.definition(definition)?;
        self.pay(self.proof.complete.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        if !self.proof.complete.contains(&definition)
            || !matches!(&declaration.body, SemanticDefinitionBody::FunctionWithBody {
                signature: SemanticSignatureRecipe { result: SemanticTypeRecipe::Opaque { definition: declared, .. }, .. },
                body: SemanticOriginalFunctionBody::Defined(_), ..
            } if *declared == definition) {
            return Err(bad("opaque result has no completed original defining body"));
        }
        self.pay(self.proof.types.len().saturating_add(1).ilog2() as usize + 1, depth)?;
        let result = self.proof.types.get(&(definition, Query::Result))
            .ok_or_else(|| bad("opaque result lacks its complete original all-return equation"))?;
        let result = copy_recipe(result, self.limits, self.left)?;
        self.contexts.push(definition);
        let checked = (|| {
            let hidden = self.resolve(definition, hidden, depth + 1)?;
            self.same(&hidden, &result)
        })();
        self.contexts.pop();
        checked?;
        Ok(result)
    }
    /// Validate the full exported opaque application while leaving its hidden
    /// all-return equation in the defining Gamma. The exported nominal recipe
    /// consumes every mixed actual and carrier through the ordinary mapping.
    pub(super) fn opaque_application_result(
        &mut self, owner: SemanticDefinitionId, definition: SemanticDefinitionId,
        arguments: &[SemanticActualRecipe], hidden: &SemanticTypeRecipe, depth: usize,
    ) -> R<SemanticTypeRecipe> {
        let result = self.original_opaque_result(definition, hidden, depth + 1)?;
        let declaration = self.definition(definition)?;
        self.call_requirements(owner, declaration, arguments, depth + 1)?;
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &declaration.body else { unreachable!() };
        let exported = super::super::instantiate::ty(self.program, &self.proof, &signature.result,
            super::super::instantiate::DomainMapping { source: definition, use_definition: owner,
                arguments, parent_call: None }, self.limits, self.left)?;
        let SemanticTypeRecipe::Opaque { definition: selected, arguments: actuals, .. } = &exported else {
            return Err(bad("opaque application changes its exported nominal constructor"));
        };
        self.pay(actuals.len().saturating_add(arguments.len()), depth)?;
        for actual in actuals.iter().chain(arguments) {
            crate::dyn_metadata::semantic_operand_snapshot(actual, self.limits, self.left, 1)?;
        }
        if *selected != definition || actuals != arguments {
            return Err(bad("opaque application changes its full original mixed arguments"));
        }
        Ok(result)
    }
    pub(super) fn seal_inferred_type_sources(
        &mut self,
        owner: SemanticDefinitionId,
        callee: NodeId,
        actuals: &mut [SemanticActualRecipe],
        depth: usize,
    ) -> R<()> {
        self.pay(actuals.len(), depth)?;
        for (ordinal, actual) in actuals.iter_mut().enumerate() {
            if !matches!(
                actual,
                SemanticActualRecipe::Type(SemanticTypeRecipe::Opaque { .. })
            ) {
                continue;
            }
            self.pay(1, depth)?;
            if let Some(source) =
                self.proof
                    .inferred_type_sources
                    .get(&(owner, callee, ordinal as u32))
            {
                let node = if let Some((node, _, _)) = source {
                    *node
                } else {
                    let origins = self
                        .proof
                        .inferred_type_origins
                        .get(&(owner, callee, ordinal as u32))
                        .ok_or_else(|| {
                            bad("opaque common type loses its complete original sources")
                        })?;
                    common_closed_sources(
                        self.program,
                        &self.proof,
                        owner,
                        origins,
                        self.limits,
                        self.left,
                    )?;
                    // This is a representation of an already proved common
                    // original type. The actual getter must validate *every*
                    // boundary before publishing the shared actual Type.
                    origins[0].0
                };
                *actual = SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Expression(node),
                });
            }
        }
        Ok(())
    }
    pub(super) fn inferred_type_source(
        &mut self,
        owner: SemanticDefinitionId,
        callee: NodeId,
        template: &SemanticTypeRecipe,
        actual: &SemanticTypeRecipe,
        expression: &SemanticOriginalExpr,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if let CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } =
            &expression.kind
        {
            return self.inferred_type_source(owner, callee, template, actual, inner, depth + 1);
        }
        if let (
            SemanticTypeRecipe::Address { inner, .. },
            SemanticTypeRecipe::Environment {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal: environment,
            },
        ) = (template, actual)
            && let SemanticTypeRecipe::Formal(SemanticFormalRef {
                scope: SemanticScopeRef::Ancestor(u32::MAX),
                ordinal: formal,
            }) = inner.as_ref()
            && let SemanticTypeRecipe::Address { inner, .. } =
                self.observation_type(owner, actual, depth + 1)?
            && matches!(inner.as_ref(), SemanticTypeRecipe::Opaque { .. })
        {
            self.pay(2, depth)?;
            self.proof
                .proof_mut()?
                .inferred_pointee_environments
                .entry((owner, callee, *formal))
                .or_default()
                .insert(*environment);
        }
        if let SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(u32::MAX),
            ordinal,
        }) = template
        {
            // An original borrow of an opaque value preserves this exact
            // pointee query. Follow it before inference resolves the Type
            // formal, so forwarding keeps the original factory boundary.
            let source_query = matches!(actual, SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Expression(node),
            } if *node == expression.id);
            if source_query || matches!(actual, SemanticTypeRecipe::Opaque { .. }) {
                self.pay(3, depth)?;
                let query = Query::Expression(expression.id);
                if let Some(&(origin_owner, origin_call)) =
                    self.proof.call_result_origins.get(&(owner, query.clone()))
                {
                    let original = self.proof.types.get(&(owner, query)).ok_or_else(|| {
                        bad("opaque inference source loses its original type equation")
                    })?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        original,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    crate::dyn_metadata::semantic_operand_snapshot(
                        actual,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    if matches!(original, SemanticTypeRecipe::Opaque { .. })
                        && (source_query || original == actual)
                    {
                        let key = (owner, callee, *ordinal);
                        self.pay(1, depth)?;
                        let origins = self
                            .proof
                            .proof_mut()?
                            .inferred_type_origins
                            .entry(key)
                            .or_default();
                        *self.left = self
                            .left
                            .checked_sub(origins.len().saturating_add(1))
                            .ok_or_else(|| {
                                Diagnostic::limit(
                                    Phase::Types,
                                    "opaque source-set work exceeded",
                                    None,
                                )
                            })?;
                        if !origins.iter().any(|(_, old_owner, old_call)| {
                            (*old_owner, *old_call) == (origin_owner, origin_call)
                        }) {
                            origins.push((expression.id, origin_owner, origin_call));
                        }
                        let candidate = Some((expression.id, origin_owner, origin_call));
                        match self.proof.inferred_type_sources.get(&key) {
                            Some(Some((_, old_owner, old_call)))
                                if (*old_owner, *old_call) != (origin_owner, origin_call) =>
                            {
                                self.proof
                                    .proof_mut()?
                                    .inferred_type_sources
                                    .insert(key, None);
                            }
                            None => {
                                self.proof
                                    .proof_mut()?
                                    .inferred_type_sources
                                    .insert(key, candidate);
                            }
                            _ => {}
                        }
                    }
                }
            }
            return Ok(());
        }
        match (template, actual, &expression.kind) {
            (
                SemanticTypeRecipe::Address {
                    inner: template, ..
                },
                SemanticTypeRecipe::Address { inner: actual, .. },
                CoreExprKind::Unary {
                    op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                    operand,
                },
            ) => self.inferred_type_source(owner, callee, template, actual, operand, depth + 1)?,
            (
                SemanticTypeRecipe::Tuple(template),
                SemanticTypeRecipe::Tuple(actual),
                CoreExprKind::Tuple(fields),
            ) if template.len() == actual.len() && actual.len() == fields.len() => {
                self.pay(fields.len(), depth)?;
                for ((template, actual), field) in template.iter().zip(actual).zip(fields) {
                    self.inferred_type_source(owner, callee, template, actual, field, depth + 1)?;
                }
            }
            (
                SemanticTypeRecipe::Array {
                    element: template, ..
                },
                SemanticTypeRecipe::Array {
                    element: actual, ..
                },
                CoreExprKind::Array(fields),
            ) => {
                self.pay(fields.len(), depth)?;
                for field in fields {
                    self.inferred_type_source(owner, callee, template, actual, field, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn alias_result_origin(
        &mut self,
        owner: SemanticDefinitionId,
        target: Query,
        source: &Query,
        depth: usize,
    ) -> R<()> {
        self.pay(source.copy_work().saturating_add(target.copy_work()), depth)?;
        self.alias_view_bound(owner, &target, source, depth + 1)?;
        let Some(origin) = self
            .proof
            .call_result_origins
            .get(&(owner, source.clone()))
            .copied()
        else {
            return Ok(());
        };
        if let Some(previous) = self.proof.call_result_origins.get(&(owner, target.clone())) {
            if *previous != origin {
                return Err(pending(
                    "original result alias requires its complete control-flow boundary family",
                ));
            }
        }
        self.pay(1, depth)?;
        self.proof
            .proof_mut()?
            .call_result_origins
            .insert((owner, target), origin);
        Ok(())
    }

    pub(super) fn expression_result_origin(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        variables: &BTreeMap<String, Binding>,
        result: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        self.record_task_name_source(owner, expression, variables, depth + 1)?;
        if matches!(result, SemanticTypeRecipe::Desc { .. })
            && matches!(expression.kind, CoreExprKind::Name(_))
        {
            // An implicit description takes its source from the live owning
            // place. An existing descriptor instead transports its prior loan.
            self.place_borrow_bound(owner, expression, variables, depth + 1)?;
        } else {
            self.view_bound(owner, expression, variables, depth + 1)?;
        }
        if let CoreExprKind::Call { callee, arguments } = &expression.kind {
            if let Some(site) = self.site(owner, SemanticOriginalNode::Expression(expression.id))? {
                self.pay(1, depth)?;
                self.proof
                    .proof_mut()?
                    .call_callees
                    .insert((owner, site.id), callee.id);
                if !self.proof.injections.is_empty() {
                    let mut injections = Vec::new();
                    for (ordinal, argument) in arguments.iter().enumerate() {
                        self.pay(self.proof.injections.len().saturating_add(1).ilog2() as usize + 1, depth)?;
                        if self.proof.injections.contains_key(&(owner, argument.id)) {
                            self.pay(1, depth)?;
                            injections.push((ordinal, argument.id));
                        }
                    }
                    if !injections.is_empty() {
                        self.pay(1, depth)?;
                        self.proof.proof_mut()?.call_injections
                            .insert((owner, site.id), (arguments.len(), injections));
                    }
                }
            }
        }
        // This receipt describes a whole opaque value. Aggregates or projections
        // need their own complete component boundary, and cannot borrow a
        // same-shaped call elsewhere in the body.
        if !matches!(result, SemanticTypeRecipe::Opaque { .. }) {
            return Ok(());
        }
        let target = Query::Expression(expression.id);
        match &expression.kind {
            CoreExprKind::Call { .. } => {
                let Some(site) =
                    self.site(owner, SemanticOriginalNode::Expression(expression.id))?
                else {
                    return Err(bad(
                        "original opaque result call loses its actual operation site",
                    ));
                };
                if self.proof.call_targets.contains_key(&(owner, site.id)) {
                    self.pay(1, depth)?;
                    self.proof
                        .proof_mut()?
                        .call_result_origins
                        .insert((owner, target), (owner, site.id));
                }
            }
            CoreExprKind::Name(name) => {
                self.pay(name.len(), depth)?;
                let binding = variables
                    .get(name)
                    .ok_or_else(|| bad("original opaque alias loses its lexical binder"))?;
                // Crossing a closure boundary must use its sealed whole Type
                // environment, never a caller same-numbered operation.
                if binding.origin_owner == owner {
                    self.pay(binding.origin.copy_work(), depth)?;
                    let origin = binding.origin.clone();
                    self.alias_result_origin(owner, target, &origin, depth + 1)?;
                }
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                self.alias_result_origin(owner, target, &Query::Expression(value.id), depth + 1)?;
            }
            CoreExprKind::Member { object, member, .. } => {
                self.record_projected_type_actual(
                    owner, expression.id, object.id, member, result, depth + 1,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    /// A product field declared as a Type formal can retain the exact Type
    /// actual used to build its nominal application. This supplies the field's
    /// complete type domain. The field's value and loan still come from its
    /// independently checked place occurrence.
    fn record_projected_type_actual(
        &mut self,
        owner: SemanticDefinitionId,
        target: NodeId,
        object: NodeId,
        member: &str,
        result: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<()> {
        self.pay(member.len().saturating_add(1), depth)?;
        let Some(SemanticTypeRecipe::Nominal { definition, .. }) =
            self.proof.types.get(&(owner, Query::Expression(object)))
        else {
            return Ok(());
        };
        let definition = self.definition(*definition)?;
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields },
            ..
        } = &definition.body
        else {
            return Ok(());
        };
        self.pay(fields.len(), depth)?;
        let Some(SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        })) = fields
            .iter()
            .find(|field| field.name == member)
            .map(|field| &field.ty)
        else {
            return Ok(());
        };
        let Some(SemanticTypeRecipe::Nominal { arguments, .. }) =
            self.proof.types.get(&(owner, Query::Expression(object)))
        else {
            return Err(bad("original field loses its nominal Type application"));
        };
        let Some(SemanticActualRecipe::Type(
            actual @ SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source: SemanticOriginalTypeSource::Expression(node),
            },
        )) = arguments.get(*ordinal as usize)
        else {
            return Ok(());
        };
        let origin = Query::Expression(*node);
        if !self.proof.call_result_origins.contains_key(&(owner, origin.clone())) {
            return Ok(());
        }
        let actual = copy_recipe(actual, self.limits, self.left)?;
        let original = self
            .proof
            .types
            .get(&(owner, origin))
            .ok_or_else(|| bad("projected opaque Type actual has no original equation"))?;
        crate::dyn_metadata::semantic_operand_snapshot(original, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(result, self.limits, self.left, 1)?;
        if original != result {
            return Err(bad(
                "projected opaque Type actual differs from the checked field type",
            ));
        }
        self.pay(1, depth)?;
        self.proof
            .proof_mut()?
            .projected_type_actuals
            .insert((owner, target), actual);
        Ok(())
    }
    /// Canonicalize only original query equations. Opaque definitions and
    /// their complete mixed arguments remain nominal; their hidden body is
    /// never unfolded in the caller Gamma. A failed equation is not a mismatch.
    fn closed_identity_type(
        &mut self, owner: SemanticDefinitionId, ty: &SemanticTypeRecipe,
        active: &mut Vec<SemanticTypeRecipe>, depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(1, depth)?;
        if matches!(ty, SemanticTypeRecipe::OriginalType { .. }
            | SemanticTypeRecipe::AssociatedType { .. }
            | SemanticTypeRecipe::SelfType { .. }
            | SemanticTypeRecipe::Environment { scope: SemanticScopeRef::Ancestor(0), .. })
        {
            for previous in active.iter() {
                crate::dyn_metadata::semantic_operand_snapshot(previous, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?;
                if previous == ty {
                    return Err(pending("original type identity has a cyclic defining equation"));
                }
            }
            let saved = copy_recipe(ty, self.limits, self.left)?;
            active.push(saved);
            let result = (|| {
                let value = if matches!(ty, SemanticTypeRecipe::Environment { .. }) {
                    self.observation_type(owner, ty, depth + 1)?
                } else {
                    self.resolve(owner, ty, depth + 1)?
                };
                crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
                if value == *ty {
                    return Err(pending("original type identity has no finite defining equation"));
                }
                self.closed_identity_type(owner, &value, active, depth + 1)
            })();
            active.pop();
            return result;
        }
        // This walker retains every inner domain and phantom argument, and
        // deliberately excludes opaque hidden bodies in their defining Gamma.
        let queries = substitute::delayed_types(ty, self.limits, self.left)?;
        self.pay(queries.len(), depth)?;
        let mut equations = Vec::new();
        for query in queries {
            let value = self.closed_identity_type(owner, &query.ty, active, depth + 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&query.ty, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&value, self.limits, self.left, 1)?;
            if value != query.ty {
                if query.bound_depth != 0 || query.invocation_depth != 0 {
                    return Err(pending("original type identity requires its nested quantifier equation context"));
                }
                equations.push((query, value));
            }
        }
        if equations.is_empty() {
            copy_recipe(ty, self.limits, self.left)
        } else {
            substitute::replace_delayed(ty, &equations, self.limits, self.left)
        }
    }
}

impl Proof {
    pub(in crate::semantics) fn closed_opaque_result<'a>(
        &'a self,
        program: &SemanticModule,
        ty: &SemanticTypeRecipe,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<(SemanticDefinitionId, &'a SemanticTypeRecipe)>> {
        self.closed_opaque_result_inner(program, ty, limits, left, 0)
    }
    /// A closed generic Type application is independent of a particular
    /// runtime result occurrence only after its complete original domain is
    /// proved empty. This does not identify any value, creator or cleanup root.
    pub(in crate::semantics) fn closed_opaque_application<'a>(
        &'a self, program: &SemanticModule, owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe, limits: &Limits, left: &mut usize,
    ) -> R<Option<(SemanticDefinitionId, &'a SemanticTypeRecipe)>> {
        let SemanticTypeRecipe::Opaque { definition, arguments, .. } = ty else {
            return Ok(None);
        };
        *left = left.checked_sub(self.complete.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types,
                "closed opaque application owner lookup exceeded", None))?;
        if arguments.is_empty() || !self.complete.contains(&owner) { return Ok(None); }
        // The complete mixed vector, phantom arguments, declaration where
        // clauses, defining all-return equation and D-to-owner mapping are all
        // checked by this theorem. Its result is not definition-only cached.
        let zero = match zero_original_domain(program, self, owner, ty,
            limits, left, 0, &mut Vec::new(), None)
        {
            Ok(zero) => zero,
            Err(error) if error.category == zil_base::ResultCategory::Unsupported => return Ok(None),
            Err(error) => return Err(error),
        };
        if !zero { return Ok(None); }
        let result = self.types.get(&(*definition, Query::Result))
            .ok_or_else(|| bad("closed opaque application has no complete defining result"))?;
        Ok(Some((*definition, result)))
    }
    /// Original mixed Type/Const identity remains nominal even when both
    /// applications erase to the same closed physical closure representation.
    pub(in crate::semantics) fn equal_closed_actual_identity(
        &self, program: &SemanticModule, owner: SemanticDefinitionId,
        expected: &SemanticActualRecipe, actual: &SemanticActualRecipe,
        limits: &Limits, left: &mut usize,
    ) -> R<bool> {
        let mut check = super::nominal_where::borrowed_check(program, self, owner, limits, left)?;
        match (expected, actual) {
            (SemanticActualRecipe::Type(expected), SemanticActualRecipe::Type(actual)) => {
                let expected = check.closed_identity_type(owner, expected, &mut Vec::new(), 1)?;
                let actual = check.closed_identity_type(owner, actual, &mut Vec::new(), 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&expected, limits, check.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&actual, limits, check.left, 1)?;
                Ok(expected == actual)
            }
            (SemanticActualRecipe::Const { carrier: expected, value: expected_value },
             SemanticActualRecipe::Const { carrier: actual, value: actual_value }) => {
                let expected = check.closed_identity_type(owner, expected, &mut Vec::new(), 1)?;
                let actual = check.closed_identity_type(owner, actual, &mut Vec::new(), 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&expected, limits, check.left, 1)?;
                crate::dyn_metadata::semantic_operand_snapshot(&actual, limits, check.left, 1)?;
                if expected != actual { return Ok(false); }
                check.same_const(&expected, expected_value, actual_value)
            }
            // This optional bridge proves a wholly zero-domain application;
            // it cannot supply any Life or unresolved inference operand.
            _ => Ok(false),
        }
    }

    fn closed_opaque_result_inner<'a>(
        &'a self,
        program: &SemanticModule,
        ty: &SemanticTypeRecipe,
        limits: &Limits,
        left: &mut usize,
        depth: usize,
    ) -> R<Option<(SemanticDefinitionId, &'a SemanticTypeRecipe)>> {
        let Some((definition, result)) = opaque_result(program, self, ty, limits, left, depth + 1)?
        else {
            return Ok(None);
        };
        *left = left
            .checked_sub(self.closed_opaque_domains.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original opaque theorem lookup exceeded",
                    None,
                )
            })?;
        let closed = match self.closed_opaque_domains.get(&definition) {
            Some(closed) => *closed,
            None => zero_original_domain(
                program,
                self,
                definition,
                result,
                limits,
                left,
                depth + 1,
                &mut Vec::new(),
                None,
            )?,
        };
        Ok(closed.then_some((definition, result)))
    }
}

impl Proof {
    pub(in crate::semantics) fn has_inferred_pointee_environments(&self) -> bool {
        !self.inferred_pointee_environments.is_empty()
    }

    pub(in crate::semantics) fn inferred_pointee_environments(
        &self,
        owner: SemanticDefinitionId,
        call: SemanticSiteId,
        formal: u32,
        holder: Option<SemanticDefinitionId>,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<&BTreeSet<u32>>> {
        if self.inferred_pointee_environments.is_empty() || !self.complete.contains(&owner) {
            return Ok(None);
        }
        *left = left
            .checked_sub(self.inferred_pointee_environments.len().saturating_add(1).ilog2() as usize + 2)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "inferred capture Type lookup exceeded", None))?;
        let Some(callee) = self.call_callees.get(&(owner, call)) else {
            return Ok(None);
        };
        if let Some(holder) = holder
            && self.inferred_call_target(owner, call, limits, left)? != Some(holder)
        {
            return Ok(None);
        }
        Ok(self.inferred_pointee_environments.get(&(owner, *callee, formal)))
    }

    /// Read a checked definition-local equation for invariant type queries.
    /// An opaque call boundary still must reconstruct its actual producer.
    pub(in crate::semantics) fn closed_world_recipe(
        &self,
        owner: SemanticDefinitionId,
        source: &SemanticOriginalTypeSource,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<&SemanticTypeRecipe>> {
        crate::dyn_metadata::semantic_operand_snapshot(source, limits, left, 1)?;
        let work = self.complete.len().saturating_add(1).ilog2() as usize
            + self.types.len().saturating_add(1).ilog2() as usize
            + self.call_result_origins.len().saturating_add(1).ilog2() as usize + 3;
        *left = left.checked_sub(work).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "relative original world recipe work exceeded", None)
        })?;
        let key = (owner, Query::from(source.clone()));
        if !self.complete.contains(&owner) || self.call_result_origins.contains_key(&key) {
            return Ok(None);
        }
        Ok(self.types.get(&key))
    }

    pub(in crate::semantics) fn call_result_boundary(
        &self,
        current: SemanticDefinitionId,
        owner: SemanticDefinitionId,
        source: &SemanticOriginalTypeSource,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<CalleeResultBoundary>> {
        // An ancestor query is interpreted only after the consumer has selected
        // that real ancestor instance. Sealed closure environments remain on
        // their existing checked path.
        if current != owner {
            return Ok(None);
        }
        let source_work = match source {
            SemanticOriginalTypeSource::BindingProjection { pattern, .. } => {
                pattern.len().saturating_add(1)
            }
            SemanticOriginalTypeSource::NameAt { name, .. } => name.len().saturating_add(1),
            _ => 1,
        };
        *left = left
            .checked_sub(source_work.saturating_add(4))
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original result boundary work exceeded", None)
            })?;
        let query = Query::from(source.clone());
        let Some(&(caller, call)) = self.call_result_origins.get(&(owner, query)) else {
            return Ok(None);
        };
        if caller != owner || !self.complete.contains(&caller) {
            return Err(bad(
                "original result boundary escapes its proved caller domain",
            ));
        }
        let callee = *self
            .call_targets
            .get(&(caller, call))
            .ok_or_else(|| bad("original result boundary has no real callee binding"))?;
        if !self.complete.contains(&callee) {
            return Err(bad(
                "original result boundary callee has no complete body proof",
            ));
        }
        let signature = self
            .direct_calls
            .get(&(caller, call))
            .or_else(|| {
                self.method_calls
                    .get(&(caller, call))
                    .map(|call| &call.signature)
            })
            .ok_or_else(|| bad("original result boundary lacks its full solved call scheme"))?;
        let SemanticTypeRecipe::Opaque { definition, .. } = &signature.result else {
            return Err(bad(
                "original opaque result boundary changes its call result constructor",
            ));
        };
        if *definition != callee {
            return Err(pending(
                "original opaque result needs its complete provider definition boundary",
            ));
        }
        let actuals = self
            .calls
            .get(&(caller, call))
            .ok_or_else(|| bad("original result boundary omits its full mixed call actuals"))?;
        let result = self.types.get(&(callee, Query::Result)).ok_or_else(|| {
            bad("original result boundary lacks its complete all-return equation")
        })?;
        *left = left
            .checked_sub(actuals.len().saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(
                    Phase::Types,
                    "original result boundary actual work exceeded",
                    None,
                )
            })?;
        let mut mapped_actuals = Vec::new();
        for ordinal in 0..actuals.len() {
            mapped_actuals.push(self.call_actual(caller, call, ordinal as u32, limits, left)?);
        }
        crate::dyn_metadata::semantic_operand_snapshot(result, limits, left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(signature, limits, left, 1)?;
        Ok(Some(CalleeResultBoundary {
            caller,
            call,
            callee,
            actuals: mapped_actuals,
            parameters: signature.parameters.clone(),
            result: result.clone(),
        }))
    }
}

impl Proof {
    pub(in crate::semantics) fn forwarded_opaque_boundaries(
        &self,
        owner: SemanticDefinitionId,
        limits: &Limits,
        left: &mut usize,
    ) -> R<Option<Vec<CalleeResultBoundary>>> {
        *left = left.checked_sub(self.opaque_forwarded_returns.len().saturating_add(1).ilog2() as usize + 1)
            .ok_or_else(|| Diagnostic::limit(Phase::Types, "forwarded opaque return lookup exceeded", None))?;
        let Some(returns) = self.opaque_forwarded_returns.get(&owner) else { return Ok(None); };
        let mut boundaries = Vec::new();
        for expression in returns {
            let Some(boundary) = self.call_result_boundary(owner, owner,
                &SemanticOriginalTypeSource::Expression(*expression), limits, left)? else {
                // A partially anchored family does not publish a result.
                return Ok(None);
            };
            boundaries.push(boundary);
        }
        Ok(Some(boundaries))
    }

    pub(in crate::semantics) fn call_actual_boundaries<'a>(
        &self,
        caller: SemanticDefinitionId,
        call: SemanticSiteId,
        formal: u32,
        limits: &Limits,
        left: &mut usize,
        program: impl FnOnce(&mut usize) -> R<&'a SemanticModule>,
    ) -> R<Option<Vec<CalleeResultBoundary>>> {
        let Some(callee) = self.call_callees.get(&(caller, call)) else {
            return Ok(None);
        };
        let Some(origins) = self.inferred_type_origins.get(&(caller, *callee, formal)) else {
            return Ok(None);
        };
        if origins.len() < 2 {
            return Ok(None);
        }
        common_closed_sources(program(left)?, self, caller, origins, limits, left)?;
        let mut boundaries = Vec::new();
        for (node, _, _) in origins {
            let boundary = self
                .call_result_boundary(
                    caller,
                    caller,
                    &SemanticOriginalTypeSource::Expression(*node),
                    limits,
                    left,
                )?
                .ok_or_else(|| {
                    bad("common opaque source lacks its exact original call boundary")
                })?;
            boundaries.push(boundary);
        }
        Ok(Some(boundaries))
    }
}
fn common_closed_sources(
    program: &SemanticModule,
    proof: &Proof,
    caller: SemanticDefinitionId,
    origins: &[(NodeId, SemanticDefinitionId, SemanticSiteId)],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    *left = left
        .checked_sub(origins.len().saturating_add(1))
        .ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "common opaque source work exceeded", None)
        })?;
    if origins.len() < 2 {
        return Err(bad("common opaque proof has fewer than two sources"));
    }
    let mut common = None;
    for (node, owner, site) in origins {
        if *owner != caller {
            return Err(pending(
                "common opaque sources cross an unproved lexical boundary",
            ));
        }
        let target = *proof
            .call_targets
            .get(&(*owner, *site))
            .ok_or_else(|| bad("opaque source has no real original callee"))?;
        let definition = original_definition(program, proof, target, limits, left)?;
        if !definition.parameters.is_empty()
            || definition.parent.is_some()
            || !proof.complete.contains(&target)
        {
            return Err(pending(
                "common opaque contexts require an independent full generic/ancestor result theorem",
            ));
        }
        let ty = proof
            .types
            .get(&(caller, Query::Expression(*node)))
            .ok_or_else(|| bad("opaque source loses its exact expression equation"))?;
        if !matches!(ty, SemanticTypeRecipe::Opaque { definition, arguments, .. } if *definition == target && arguments.is_empty())
        {
            return Err(bad(
                "opaque source changes its complete callee/actual context",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(ty, limits, left, 1)?;
        if common.is_some_and(|previous| previous != ty) {
            return Err(bad("opaque sources have different original complete types"));
        }
        common = Some(ty);
        if !zero_original_domain(program, proof, target, ty, limits, left, 0, &mut Vec::new(), None)? {
            return Err(pending(
                "common opaque sources lack a proved complete closed result domain",
            ));
        }
    }
    Ok(())
}
fn original_definition<'a>(
    program: &'a SemanticModule,
    proof: &Proof,
    id: SemanticDefinitionId,
    limits: &Limits,
    left: &mut usize,
) -> R<&'a SemanticDefinition> {
    let _ = limits;
    *left = left
        .checked_sub(proof.definition_positions.len().saturating_add(1).ilog2() as usize + 2)
        .ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "closed original definition lookup exceeded",
                None,
            )
        })?;
    let position = proof
        .definition_positions
        .get(&id)
        .ok_or_else(|| bad("closed original type has no definition in its immutable context"))?;
    program
        .definitions
        .get(*position)
        .filter(|row| row.id == id)
        .ok_or_else(|| {
            bad("closed original definition index changes its full declaration identity")
        })
}
struct CurrentDomainGamma<'a> {
    contexts: &'a [SemanticDefinitionId],
}

fn zero_original_domain(
    program: &SemanticModule,
    proof: &Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
    depth: usize,
    active: &mut Vec<(SemanticDefinitionId, SemanticTypeRecipe)>,
    current: Option<&CurrentDomainGamma<'_>>,
) -> R<bool> {
    *left = left
        .checked_sub(active.len().saturating_add(1))
        .ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original closed-domain work exceeded", None)
        })?;
    if depth > limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "original closed-domain depth exceeded",
            None,
        ));
    }
    crate::dyn_metadata::semantic_operand_snapshot(ty, limits, left, 1)?;
    for (scope, value) in active.iter() {
        crate::dyn_metadata::semantic_operand_snapshot(value, limits, left, 1)?;
        if *scope == owner && value == ty {
            return Ok(false);
        }
    }
    active.push((owner, ty.clone()));
    let result =
        zero_original_domain_inner(program, proof, owner, ty, limits, left, depth + 1, active, current);
    active.pop();
    result
}
fn opaque_result<'a>(
    program: &SemanticModule,
    proof: &'a Proof,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
    depth: usize,
) -> R<Option<(SemanticDefinitionId, &'a SemanticTypeRecipe)>> {
    let SemanticTypeRecipe::Opaque {
        definition,
        arguments,
        hidden,
    } = ty
    else {
        return Ok(None);
    };
    let source = original_definition(program, proof, *definition, limits, left)?;
    if !arguments.is_empty()
        || !source.parameters.is_empty()
        || !source.environment.is_empty()
        || source.parent.is_some()
        || !proof.complete.contains(definition)
        || !matches!(
            &source.body,
            SemanticDefinitionBody::FunctionWithBody {
                signature: SemanticSignatureRecipe {
                    result: SemanticTypeRecipe::Opaque { definition: declared, arguments, .. }, ..
                },
                body: SemanticOriginalFunctionBody::Defined(_),
                ..
            } if declared == definition && arguments.is_empty()
        )
    {
        return Ok(None);
    }
    // Return-role declarations were independently discharged by function().
    // Other caller obligations cannot silently disappear at this boundary.
    for requirement in &source.requirements {
        if !matches!(
            requirement,
            SemanticRequirementRecipe::Callable {
                ty: SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0),
                    source: SemanticOriginalTypeSource::Result
                },
                ..
            }
        ) {
            return Ok(None);
        }
    }
    let result = proof
        .types
        .get(&(*definition, Query::Result))
        .ok_or_else(|| bad("closed opaque omits its complete original all-return equation"))?;
    // The exact defining-result query names this all-return equation without
    // expanding and cloning its complete environment at every consumer.
    if !matches!(
        hidden.as_ref(),
        SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Result,
        }
    ) {
        let mut check =
            super::nominal_where::borrowed_check(program, proof, *definition, limits, left)?;
        let hidden = check.resolve(*definition, hidden, depth + 1)?;
        check.same(&hidden, result)?;
    }
    Ok(Some((*definition, result)))
}
/// A generic opaque keeps its complete mixed application even if some
/// arguments are absent from its runtime captures. This is an original
/// zero-domain theorem, separate from ClosedEnvironment and from an actual
/// closure/header proposal.
fn zero_opaque_application(
    program: &SemanticModule,
    proof: &Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
    depth: usize,
    active: &mut Vec<(SemanticDefinitionId, SemanticTypeRecipe)>,
    current: Option<&CurrentDomainGamma<'_>>,
) -> R<bool> {
    let SemanticTypeRecipe::Opaque { definition, arguments, hidden } = ty else {
        return Ok(false);
    };
    let source = original_definition(program, proof, *definition, limits, left)?;
    if source.parent.is_some()
        || !source.environment.is_empty()
        || source.parameters.len() != arguments.len()
        || !proof.complete.contains(definition)
        || !matches!(&source.body, SemanticDefinitionBody::FunctionWithBody {
            signature: SemanticSignatureRecipe {
                result: SemanticTypeRecipe::Opaque { definition: declared, .. }, ..
            },
            foreign: None,
            body: SemanticOriginalFunctionBody::Defined(_), ..
        } if declared == definition)
    {
        return Ok(false);
    }
    let result = proof.types.get(&(*definition, Query::Result))
        .ok_or_else(|| bad("opaque zero-domain application lacks its complete all-return equation"))?;
    {
        let mut check = if let Some(gamma) = current.filter(|gamma| gamma.contexts.last() == Some(&owner)) {
            // Reborrow only the currently checked declaration Gamma. This does
            // not assert that its body, returns or cleanup are complete.
            original_definition(program, proof, owner, limits, left)?;
            *left = left.checked_sub(gamma.contexts.len().saturating_mul(
                std::mem::size_of::<SemanticDefinitionId>()
            ).saturating_add(1))
                .ok_or_else(|| Diagnostic::limit(Phase::Types,
                    "current original zero-domain Gamma work exceeded", None))?;
            Check {
                program, limits, left: &mut *left, proof: ProofAccess::Borrowed(proof),
                active: BTreeSet::new(), capture_writes: BTreeMap::new(), loops: Vec::new(),
                trait_active: Vec::new(), contexts: gamma.contexts.to_vec(),
                closure_result_goal: None,
                integer_initializers: BTreeMap::new(),
                contextual_code_goals: BTreeMap::new(),
                contextual_code_need: None,
                contextual_code_inputs: BTreeMap::new(),
                contextual_code_calls: BTreeSet::new(),
                predicate_active: Vec::new(), stage_depth: 0, cleanup_depth: 0,
                requested_snapshot: None, snapshot_sources: None, escape_active: Vec::new(),
            }
        } else {
            super::nominal_where::borrowed_check(program, proof, owner, limits, left)?
        };
        // The full caller application discharges the declaration's original
        // requirements. No result/header hint supplies a missing premise.
        check.call_requirements(owner, source, arguments, depth + 1)?;
        if !matches!(hidden.as_ref(), SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(0),
            source: SemanticOriginalTypeSource::Result,
        }) {
            check.contexts.push(*definition);
            let equation = (|| {
                let hidden = check.resolve(*definition, hidden, depth + 1)?;
                check.same(&hidden, result)
            })();
            check.contexts.pop();
            equation?;
        }
    }
    // Validate complete kinds/carriers and the source-to-use lexical mapping
    // before considering whether any original domain is empty.
    let specialized = super::super::instantiate::ty(
        program, proof, result,
        super::super::instantiate::DomainMapping {
            source: *definition, use_definition: owner, arguments, parent_call: None,
        }, limits, left,
    )?;
    *left = left.checked_sub(arguments.len().saturating_add(1))
        .ok_or_else(|| Diagnostic::limit(Phase::Types,
            "opaque complete mixed zero-domain work exceeded", None))?;
    let mut all = true;
    for argument in arguments {
        all &= match argument {
            SemanticActualRecipe::Type(ty) =>
                zero_original_domain(program, proof, owner, ty, limits, left, depth + 1, active, current)?,
            SemanticActualRecipe::Const { carrier, value } => {
                matches!(value, SemanticConstRecipe::Value(_) | SemanticConstRecipe::IntegerLiteral(_))
                    && zero_original_domain(program, proof, owner, carrier, limits, left, depth + 1, active, current)?
            }
            // A phantom Life is still part of the original complete domain.
            SemanticActualRecipe::Life(_) | SemanticActualRecipe::Infer => false,
        };
    }
    all &= zero_original_domain(program, proof, owner, &specialized, limits, left, depth + 1, active, current)?;
    Ok(all)
}

fn zero_original_domain_inner(
    program: &SemanticModule,
    proof: &Proof,
    owner: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
    depth: usize,
    active: &mut Vec<(SemanticDefinitionId, SemanticTypeRecipe)>,
    current: Option<&CurrentDomainGamma<'_>>,
) -> R<bool> {
    let recurse = |scope, ty: &SemanticTypeRecipe, left: &mut usize, active: &mut Vec<_>| {
        zero_original_domain(program, proof, scope, ty, limits, left, depth + 1, active, current)
    };
    Ok(match ty {
        SemanticTypeRecipe::Atom(ty) => {
            ty.is_b0_scalar()
                || matches!(
                    ty,
                    Type::Unit | Type::ByteBuffer | Type::ForeignHandle | Type::CWideString
                )
        }
        SemanticTypeRecipe::Exact(value) => {
            value.classes.is_empty()
                && value.life.binder_count == 0
                && value.life.slots.is_empty()
                && value.life.retained.is_empty()
                && value.life.nominal_arguments.is_empty()
                && (value.ty.is_b0_scalar() || value.ty == Type::Unit)
        }
        SemanticTypeRecipe::OriginalType {
            scope: SemanticScopeRef::Ancestor(hops),
            source,
        } => {
            let mut target = owner;
            for _ in 0..*hops {
                target = original_definition(program, proof, target, limits, left)?
                    .parent
                    .ok_or_else(|| bad("closed query escapes its real lexical parent"))?;
            }
            let value = proof
                .types
                .get(&(target, Query::from(source.clone())))
                .ok_or_else(|| bad("closed original query has no equation"))?;
            recurse(target, value, left, active)?
        }
        SemanticTypeRecipe::Environment {
            scope: SemanticScopeRef::Ancestor(hops),
            ordinal,
        } => {
            let mut target = owner;
            for _ in 0..*hops {
                target = original_definition(program, proof, target, limits, left)?
                    .parent
                    .ok_or_else(|| bad("closed environment escapes its real lexical parent"))?;
            }
            let (parent, value) = proof
                .environments
                .get(&(target, *ordinal))
                .ok_or_else(|| bad("closed type loses its original sealed environment"))?;
            recurse(*parent, value, left, active)?
        }
        SemanticTypeRecipe::Opaque { .. } => {
            match opaque_result(program, proof, ty, limits, left, depth + 1)? {
                Some((definition, result)) => {
                    *left = left
                        .checked_sub(
                            proof.closed_opaque_domains.len().saturating_add(1).ilog2() as usize
                                + 1,
                        )
                        .ok_or_else(|| {
                            Diagnostic::limit(
                                Phase::Types,
                                "original nested opaque theorem lookup exceeded",
                                None,
                            )
                        })?;
                    match proof.closed_opaque_domains.get(&definition) {
                        Some(closed) => *closed,
                        None => recurse(definition, result, left, active)?,
                    }
                }
                None => zero_opaque_application(
                    program, proof, owner, ty, limits, left, depth + 1, active, current,
                )?,
            }
        }
        SemanticTypeRecipe::Closure {
            definition,
            arguments,
            environment,
        } => {
            let child = original_definition(program, proof, *definition, limits, left)?;
            if child.kind != SemanticDefinitionKind::Closure
                || child.parent.is_none()
                || !arguments.is_empty()
                || !child.parameters.is_empty()
                || child.environment.len() != environment.len()
                || !proof.complete.contains(definition)
                || proof.borrowed_capture_heads.contains(definition)
            {
                false
            } else {
                let mut all = true;
                // The use may have crossed a real factory boundary. The
                // declaration retains its original parent; all actual whole
                // environment Types are interpreted in this use's scope.
                for (formal, value) in child.environment.iter().zip(environment) {
                    all &= match (&formal.kind, value) {
                        (SemanticFormal::Type { .. }, SemanticActualRecipe::Type(ty)) => {
                            recurse(owner, ty, left, active)?
                        }
                        _ => false,
                    };
                }
                all
            }
        }
        SemanticTypeRecipe::Managed(inner)
        | SemanticTypeRecipe::OwnedStore(inner)
        | SemanticTypeRecipe::Mutex(inner)
        | SemanticTypeRecipe::Registration(inner)
        | SemanticTypeRecipe::AsyncRegistration(inner)
        | SemanticTypeRecipe::Slice(inner) => recurse(owner, inner, left, active)?,
        SemanticTypeRecipe::Address {
            lifetime: SemanticOuterLifeRecipe::Static,
            inner,
            ..
        }
        | SemanticTypeRecipe::Desc {
            lifetime: SemanticOuterLifeRecipe::Static,
            inner,
        } => recurse(owner, inner, left, active)?,
        SemanticTypeRecipe::Task { result, retained } => {
            // Task's original domain includes every input segment. Only
            // complete structural zero-domain proofs erase those segments;
            // ClosedEnvironment or an equal actual image cannot do so.
            let sequence = |parts: &SemanticClassSequenceRecipe,
                            left: &mut usize,
                            active: &mut Vec<_>|
             -> R<bool> {
                *left = left.checked_sub(parts.parts.len()).ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "original Task zero-domain sequence work exceeded",
                        None,
                    )
                })?;
                let mut all = true;
                for part in &parts.parts {
                    all &= match part {
                        SemanticClassSequencePart::AllClasses { ty, path } if path.is_empty() => {
                            recurse(owner, ty, left, active)?
                        }
                        _ => false,
                    };
                }
                Ok(all)
            };
            *left = left
                .checked_sub(
                    retained
                        .ports
                        .len()
                        .saturating_add(retained.retained.len())
                        .saturating_add(retained.nominal.len()),
                )
                .ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "original Task zero-domain occurrence work exceeded",
                        None,
                    )
                })?;
            let mut all =
                recurse(owner, result, left, active)? && sequence(&retained.domain, left, active)?;
            for port in &retained.ports {
                all &= match port {
                    SemanticPortPart::FromType { ty, classes } => {
                        recurse(owner, ty, left, active)? && sequence(classes, left, active)?
                    }
                    SemanticPortPart::One { .. } => false,
                };
            }
            for row in &retained.retained {
                all &= match row {
                    SemanticRetainedPart::All { parents, .. } => sequence(parents, left, active)?,
                    SemanticRetainedPart::One { .. } => false,
                };
            }
            for row in &retained.nominal {
                all &= match row {
                    SemanticNominalPart::FromType { ty, classes, .. } => {
                        recurse(owner, ty, left, active)? && sequence(classes, left, active)?
                    }
                    SemanticNominalPart::One { .. } => false,
                };
            }
            all
        }
        SemanticTypeRecipe::Tuple(fields) | SemanticTypeRecipe::AnonymousSum(fields) => {
            let mut all = true;
            for field in fields {
                all &= recurse(owner, field, left, active)?;
            }
            all
        }
        SemanticTypeRecipe::Array { element, length } => {
            matches!(
                length.as_ref(),
                SemanticConstRecipe::Value(_) | SemanticConstRecipe::IntegerLiteral(_)
            ) && recurse(owner, element, left, active)?
        }
        SemanticTypeRecipe::ErrorUnion { error, result } => {
            recurse(owner, error, left, active)? && recurse(owner, result, left, active)?
        }
        SemanticTypeRecipe::Nominal {
            definition,
            arguments,
        } => {
            let source = original_definition(program, proof, *definition, limits, left)?;
            crate::dyn_metadata::semantic_operand_snapshot(source, limits, left, 1)?;
            let mut all =
                source.implicit_lifetimes.is_empty() && source.parameters.len() == arguments.len();
            for actual in arguments {
                all &= match actual {
                    SemanticActualRecipe::Type(ty) => recurse(owner, ty, left, active)?,
                    SemanticActualRecipe::Const { carrier, value } => {
                        matches!(
                            value,
                            SemanticConstRecipe::Value(_) | SemanticConstRecipe::IntegerLiteral(_)
                        ) && recurse(owner, carrier, left, active)?
                    }
                    _ => false,
                };
            }
            let body = match &source.body {
                SemanticDefinitionBody::Nominal { body, .. } => body,
                _ => return Err(bad("closed nominal type selects a nonnominal definition")),
            };
            let mut fields = Vec::new();
            match body {
                SemanticNominalBody::Product { fields: rows } => {
                    for field in rows {
                        fields.push(&field.ty);
                    }
                }
                SemanticNominalBody::Enum { variants } => {
                    for variant in variants {
                        fields.extend(variant.fields.iter());
                    }
                }
            }
            *left = left
                .checked_sub(fields.len().saturating_add(1))
                .ok_or_else(|| {
                    Diagnostic::limit(Phase::Types, "closed nominal field work exceeded", None)
                })?;
            for field in fields {
                let value = super::super::instantiate::ty(
                    program,
                    proof,
                    field,
                    super::super::instantiate::DomainMapping {
                        source: *definition,
                        use_definition: owner,
                        arguments,
                        parent_call: None,
                    },
                    limits,
                    left,
                )?;
                all &= recurse(owner, &value, left, active)?;
            }
            all
        }
        // Every other unresolved or variable-lifetime constructor remains unproved.
        // No empty claimed slots/ports, Closed bound, or actual type hint is a
        // zero-domain theorem for one of these constructors.
        _ => false,
    })
}

impl Check<'_, '_> {
    pub(super) fn cache_closed_opaque_domains(&mut self) -> R<()> {
        self.pay(self.program.definitions.len(), 0)?;
        for definition in &self.program.definitions {
            let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &definition.body
            else {
                continue;
            };
            if !matches!(&signature.result, SemanticTypeRecipe::Opaque { definition: owner, .. } if *owner == definition.id)
            {
                continue;
            }
            self.pay(
                self.proof
                    .closed_opaque_domains
                    .len()
                    .saturating_add(1)
                    .ilog2() as usize
                    + 1,
                0,
            )?;
            if self
                .proof
                .closed_opaque_domains
                .contains_key(&definition.id)
            {
                continue;
            }
            let closed = self
                .proof
                .closed_opaque_result_inner(
                    self.program,
                    &signature.result,
                    self.limits,
                    self.left,
                    0,
                )?
                .is_some();
            self.pay(1, 0)?;
            self.proof
                .proof_mut()?
                .closed_opaque_domains
                .insert(definition.id, closed);
        }
        Ok(())
    }
    /// Complete original result domain, independent of Mono or claimed class
    /// counts. The returned value always retains its real callee definition.
    pub(super) fn closed_opaque_result(
        &mut self,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<(SemanticDefinitionId, SemanticTypeRecipe)>> {
        match self.proof.closed_opaque_result_inner(
            self.program,
            ty,
            self.limits,
            self.left,
            depth + 1,
        )? {
            Some((definition, result)) => Ok(Some((
                definition,
                copy_recipe(result, self.limits, self.left)?,
            ))),
            None => Ok(None),
        }
    }
    pub(super) fn original_zero_domain(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        let current = (self.contexts.last().copied() == Some(owner))
            .then_some(CurrentDomainGamma { contexts: &self.contexts });
        zero_original_domain(
            self.program,
            &self.proof,
            owner,
            ty,
            self.limits,
            self.left,
            depth + 1,
            &mut Vec::new(),
            current.as_ref(),
        )
    }
}

#[cfg(test)]
mod closed_world_recipe_tests {
    use super::*;

    #[test]
    fn sharing_a_recipe_cannot_skip_its_actual_opaque_call_boundary() {
        let owner = SemanticDefinitionId { module: zil_base::ModuleId(0), index: 0 };
        let node = NodeId(3);
        let source = SemanticOriginalTypeSource::Expression(node);
        let mut proof = Proof::default();
        proof.complete.insert(owner);
        let expected = SemanticTypeRecipe::Atom(Type::I32);
        proof.types.insert((owner, Query::Expression(node)), expected.clone());
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        assert_eq!(proof.closed_world_recipe(owner, &source, &limits, &mut left).unwrap(), Some(&expected));
        proof.call_result_origins.insert((owner, Query::Expression(node)), (owner, SemanticSiteId(0)));
        assert!(proof.closed_world_recipe(owner, &source, &limits, &mut left).unwrap().is_none());
    }
}
