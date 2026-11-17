//! Immutable original callable-use modes select an emitted invocation. This
//! directory supplies no actual operand, signature, borrow or cleanup proof.
use super::*;

fn index(
    program: &SemanticModule,
    proof: &super::super::original::Proof,
    limits: &Limits,
    left: &mut usize,
    span: Span,
) -> R<BTreeMap<(Origin, NodeId), CallableKind>> {
    pay(left, program.instances.len(), span)?;
    let mut instances = BTreeMap::new();
    let mut owners = BTreeMap::new();
    for row in &program.instances {
        let origin = match &row.target {
            SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) =>
                Origin { module: *module, creator: *function, closure: None },
            SemanticInstanceTarget::Function(SemanticFunctionTarget::CoreClosure {
                module, creator, expression, occurrence: 0,
            }) => Origin { module: *module, creator: *creator, closure: Some(*expression) },
            _ => continue,
        };
        pay(left, lookup_cost(instances.len()).saturating_add(lookup_cost(owners.len())).saturating_add(2), span)?;
        if instances.len() >= limits.max_nodes || owners.len() >= limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "original callable origin directory limit exceeded", Some(span)));
        }
        if instances.insert(row.id, (origin, row.definition)).is_some()
            || owners.insert(origin, row.definition).is_some_and(|old| old != row.definition)
        {
            return Err(invalid(span, "actual callable code has ambiguous original lexical ownership"));
        }
    }
    pay(left, program.sites.len(), span)?;
    let mut modes = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    for site in &program.sites {
        let SemanticSiteAddress::Core { expression, occurrence: 0, role: SemanticEmissionRole::Call } = site.at else { continue; };
        pay(left, lookup_cost(instances.len()).saturating_add(lookup_cost(proof.callable_calls.len())).saturating_add(2), span)?;
        let Some((origin, owner)) = instances.get(&site.instance).copied() else { continue; };
        let Some(call) = proof.callable_calls.get(&(owner, site.recipe)) else { continue; };
        pay(left, lookup_cost(proof.complete.len()).saturating_add(lookup_cost(modes.len())).saturating_add(lookup_cost(seen.len())).saturating_add(3), span)?;
        if !proof.complete.contains(&owner) || !matches!(site.action,
            SemanticSiteAction::CoreBodyCall | SemanticSiteAction::CallableCall { .. } | SemanticSiteAction::Call { .. })
        {
            return Err(invalid(span, "callable use lacks its complete original call theorem"));
        }
        if modes.len() >= limits.max_nodes || seen.len() >= limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "original callable use directory limit exceeded", Some(span)));
        }
        if !seen.insert((site.instance, expression))
            || modes.insert((origin, expression), call.kind).is_some_and(|old| old != call.kind)
        {
            return Err(invalid(span, "one exact callable use changes its original invocation mode"));
        }
    }
    Ok(modes)
}

impl Ledger {
    pub(crate) fn original_callable_kind(
        &self, function: FunctionId, callee: NodeId, limits: &Limits,
        left: &mut usize, span: Span,
    ) -> R<Option<CallableKind>> {
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(frame) = self.frames.get(&function) else { return Ok(None); };
        let Some(call) = frame.stack.last().copied() else { return Ok(None); };
        pay(left, lookup_cost(frame.expressions.len()), span)?;
        let expression = frame.expressions.get(&call)
            .ok_or_else(|| invalid(span, "active callable use has no real emitted expression"))?;
        // This API cannot lend the outer invocation's mode to an argument,
        // another expression, a same-numbered child, or an arbitrary Type.
        if expression.callee != Some(callee) { return Ok(None); }
        let program = self.original.context().map(|context| &context.semantic)
            .or_else(|| self.frozen.as_deref().map(|frozen| &frozen.semantic));
        let Some(program) = program else { return Ok(None); };
        if self.original_callable_modes.borrow().is_none() {
            let view = self.original.borrow();
            let proof = view.as_ref().or_else(|| self.frozen.as_deref().map(|frozen| &frozen.original))
                .ok_or_else(|| invalid(span, "callable mode query has no immutable original proof"))?;
            let modes = index(program, proof, limits, left, span)?;
            *self.original_callable_modes.borrow_mut() = Some(modes);
        }
        let modes = self.original_callable_modes.borrow();
        let modes = modes.as_ref().expect("complete original callable mode directory");
        pay(left, lookup_cost(modes.len()), span)?;
        Ok(modes.get(&(frame.origin, call)).copied())
    }
}
