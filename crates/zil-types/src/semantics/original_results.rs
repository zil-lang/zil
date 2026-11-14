//! A result boundary is selected by original operation provenance. Hidden
//! recipes retain their callee lexical domain until the actual call is checked.
use super::*;

#[derive(Clone)]
pub(crate) struct CalleeResultBoundary {
    pub caller: SemanticDefinitionId,
    pub call: SemanticSiteId,
    pub callee: SemanticDefinitionId,
    pub actuals: Vec<SemanticActualRecipe>,
    /// Complete all-return result in the original callee definition domain.
    pub result: SemanticTypeRecipe,
}

impl Check<'_, '_> {
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
        if let SemanticTypeRecipe::Formal(SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(u32::MAX),
            ordinal,
        }) = template
        {
            if matches!(actual, SemanticTypeRecipe::Opaque { .. }) {
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
                    if original == actual {
                        let key = (owner, callee, *ordinal);
                        self.pay(1, depth)?;
                        let origins = self.proof.inferred_type_origins.entry(key).or_default();
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
                                self.proof.inferred_type_sources.insert(key, None);
                            }
                            None => {
                                self.proof.inferred_type_sources.insert(key, candidate);
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
        if let CoreExprKind::Call { callee, .. } = &expression.kind {
            if let Some(site) = self.site(owner, SemanticOriginalNode::Expression(expression.id))? {
                self.pay(1, depth)?;
                self.proof.call_callees.insert((owner, site.id), callee.id);
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
            _ => {}
        }
        Ok(())
    }
}

impl Proof {
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
        Ok(Some(CalleeResultBoundary {
            caller,
            call,
            callee,
            actuals: mapped_actuals,
            result: result.clone(),
        }))
    }
}

impl Proof {
    pub(in crate::semantics) fn call_actual_boundaries(
        &self,
        program: &SemanticModule,
        caller: SemanticDefinitionId,
        call: SemanticSiteId,
        formal: u32,
        limits: &Limits,
        left: &mut usize,
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
        common_closed_sources(program, self, caller, origins, limits, left)?;
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
        let definition = original_definition(program, target, limits, left)?;
        if !definition.parameters.is_empty()
            || definition.parent.is_some()
            || !proof.complete.contains(&target)
        {
            return Err(pending("common opaque contexts require an independent full generic/ancestor result theorem"));
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
        if !zero_original_domain(program, proof, target, ty, limits, left, 0, &mut Vec::new())? {
            return Err(pending(
                "common opaque sources lack a proved complete closed result domain",
            ));
        }
    }
    Ok(())
}
fn original_definition<'a>(
    program: &'a SemanticModule,
    id: SemanticDefinitionId,
    limits: &Limits,
    left: &mut usize,
) -> R<&'a SemanticDefinition> {
    let _ = limits;
    *left = left
        .checked_sub(program.definitions.len().saturating_add(1))
        .ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "closed original definition lookup exceeded",
                None,
            )
        })?;
    let mut rows = program.definitions.iter().filter(|row| row.id == id);
    let row = rows
        .next()
        .ok_or_else(|| bad("closed original type has no definition"))?;
    if rows.next().is_some() {
        return Err(bad("closed original definition is ambiguous"));
    }
    Ok(row)
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
        zero_original_domain_inner(program, proof, owner, ty, limits, left, depth + 1, active);
    active.pop();
    result
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
) -> R<bool> {
    let recurse = |scope, ty: &SemanticTypeRecipe, left: &mut usize, active: &mut Vec<_>| {
        zero_original_domain(program, proof, scope, ty, limits, left, depth + 1, active)
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
                target = original_definition(program, target, limits, left)?
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
                target = original_definition(program, target, limits, left)?
                    .parent
                    .ok_or_else(|| bad("closed environment escapes its real lexical parent"))?;
            }
            let (parent, value) = proof
                .environments
                .get(&(target, *ordinal))
                .ok_or_else(|| bad("closed type loses its original sealed environment"))?;
            recurse(*parent, value, left, active)?
        }
        SemanticTypeRecipe::Opaque {
            definition,
            arguments,
            hidden,
        } => {
            let source = original_definition(program, *definition, limits, left)?;
            arguments.is_empty()
                && source.parameters.is_empty()
                && source.parent.is_none()
                && proof.complete.contains(definition)
                && recurse(*definition, hidden, left, active)?
        }
        SemanticTypeRecipe::Closure {
            definition,
            arguments,
            environment,
        } => {
            let child = original_definition(program, *definition, limits, left)?;
            if child.parent != Some(owner)
                || !arguments.is_empty()
                || !child.parameters.is_empty()
                || child.environment.len() != environment.len()
                || !proof.complete.contains(definition)
            {
                false
            } else {
                let mut all = true;
                for value in environment {
                    all &= match value {
                        SemanticActualRecipe::Type(ty) => recurse(owner, ty, left, active)?,
                        _ => false,
                    };
                }
                all
            }
        }
        SemanticTypeRecipe::Managed(inner) | SemanticTypeRecipe::OwnedStore(inner) => {
            recurse(owner, inner, left, active)?
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
            let source = original_definition(program, *definition, limits, left)?;
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
        // Every unresolved or lifetime-bearing constructor remains unproved.
        // No empty claimed slots/ports, Closed bound, or actual type hint is a
        // zero-domain theorem for one of these constructors.
        _ => false,
    })
}
