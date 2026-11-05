//! Independent complete scope-vector admission. Authority supplies authentic
//! source inventories; raw kinds, values, dependency graphs and order are checked here.
use super::*;

struct Visit<'a, 'm, A> {
    raw: &'a CtArtifactProgram,
    limits: &'a Limits,
    authority: &'a A,
    meter: &'m mut CtVerificationMeter,
    state: Vec<u8>,
    next: usize,
    weights: Option<Vec<usize>>,
    heights: Option<Vec<usize>>,
    lookup_bound: usize,
}
impl<A: CtArtifactAuthority> Visit<'_, '_, A> {
    fn reference(&mut self, reference: CtAmbientLifeReference, depth: usize) -> Check<usize> {
        let scope = self
            .raw
            .ambient_life_scopes
            .get(reference.scope.0 as usize)
            .ok_or_else(|| reject(None, "CT ambient scope reference is outside inventory"))?;
        self.meter.charge(
            self.raw
                .ambient_life_scopes
                .len()
                .saturating_add(scope.binder_site.len())
                .saturating_add(32),
            None,
        )?;
        charge_lookup(
            scope,
            self.weights
                .as_ref()
                .map_or(0, |weights| weights[reference.scope.0 as usize]),
            self.lookup_bound,
            self.meter,
        )?;
        if self
            .authority
            .scope_parameter_kind(scope, reference.parameter as usize)
            != Some(CtScopeParameterKind::Lifetime)
        {
            return Err(reject(
                None,
                "CT ambient reference does not name an actual lifetime ordinal",
            ));
        }
        self.scope(reference.scope.0 as usize, depth)
    }
    fn scope(&mut self, index: usize, depth: usize) -> Check<usize> {
        if depth > self.limits.max_call_depth || depth > self.raw.budget.max_call_depth as usize {
            return Err(limit(
                None,
                "CT instantiated scope dependency depth exceeded",
            ));
        }
        match self.state[index] {
            2 => {
                let height = if let Some(heights) = &self.heights {
                    self.meter.charge(1, None)?;
                    heights[index]
                } else {
                    0
                };
                if depth.saturating_add(height) > self.limits.max_call_depth
                    || depth.saturating_add(height) > self.raw.budget.max_call_depth as usize
                {
                    return Err(limit(
                        None,
                        "CT instantiated scope dependency depth exceeded",
                    ));
                }
                return Ok(height);
            }
            1 => return Err(reject(None, "CT instantiated scope dependency cycle")),
            _ => {}
        }
        self.state[index] = 1;
        let raw = self.raw;
        let mut height = 0;
        if let Some(parent) = raw.ambient_life_scopes[index].enclosing {
            self.meter.charge(1, None)?;
            if parent.0 as usize >= raw.ambient_life_scopes.len() {
                return Err(reject(None, "CT lexical host scope is outside inventory"));
            }
            height = self.scope(parent.0 as usize, depth + 1)?.saturating_add(1);
        }
        if let CtScopeInstance::Instantiated(actuals) = &raw.ambient_life_scopes[index].instance {
            self.meter.charge(actuals.len(), None)?;
            for actual in actuals {
                match actual {
                    CtScopeActual::Type(graph) => {
                        height = height.max(self.graph(graph, depth + 1)?);
                    }
                    CtScopeActual::Lifetime(reference) => {
                        height = height.max(self.reference(*reference, depth + 1)? + 1);
                    }
                    CtScopeActual::Scalar(_) => {}
                }
            }
        }
        if index != self.next {
            return Err(reject(
                None,
                "CT ambient scope table is not in first actual use order",
            ));
        }
        self.state[index] = 2;
        if let Some(heights) = &mut self.heights {
            self.meter.charge(1, None)?;
            heights[index] = height;
        }
        self.next += 1;
        Ok(height)
    }
    fn graph(&mut self, graph: &CtTypeRef, depth: usize) -> Check<usize> {
        self.meter
            .charge(graph.ambient_classes.len().saturating_mul(32), None)?;
        let mut previous = None;
        let mut height = 0;
        for row in &graph.ambient_classes {
            let key = (row.owner, row.class);
            if previous.is_some_and(|old| old >= key) {
                return Err(reject(
                    None,
                    "CT ambient class mappings repeat or are not ordered",
                ));
            }
            previous = Some(key);
            height = height.max(self.reference(row.source, depth)? + 1);
        }
        Ok(height)
    }
}

fn charge_lookup(
    scope: &CtAmbientLifeScope,
    weight: usize,
    bound: usize,
    meter: &mut CtVerificationMeter,
) -> Check<()> {
    let work = if scope.enclosing.is_some()
        || matches!(scope.instance, CtScopeInstance::Instantiated(_))
    {
        weight.saturating_mul(bound)
    } else {
        32usize.saturating_mul(bound.saturating_sub(1))
    };
    meter.charge(work, None)
}

pub(super) fn check(
    raw: &CtArtifactProgram,
    limits: &Limits,
    authority: &impl CtArtifactAuthority,
    meter: &mut CtVerificationMeter,
) -> Check<usize> {
    let count = raw.ambient_life_scopes.len();
    if count > limits.max_nodes || count as u64 > raw.budget.max_generated_nodes {
        return Err(limit(None, "CT ambient scope inventory exceeds quota"));
    }
    meter.charge(count.saturating_mul(8).saturating_add(1), None)?;
    if count as u64 > raw.budget.max_temporary_bytes {
        return Err(limit(None, "CT ambient inventory scratch exceeds quota"));
    }
    // Source authority includes private scalar/Type instances that need no raw
    // ambient row. The count is a work bound only, never scope authorization.
    let lookup_bound = if count == 0 {
        0
    } else {
        authority
            .active_scope_count()
            .unwrap_or_else(|| raw.functions.len().saturating_add(count).saturating_add(1))
    };
    if lookup_bound > limits.max_nodes {
        return Err(limit(
            None,
            "CT authentic scope lookup inventory exceeds quota",
        ));
    }
    let root_bound = authority.runtime_scope_root_count();
    if root_bound > limits.max_nodes {
        return Err(limit(
            None,
            "CT runtime root authority inventory exceeds quota",
        ));
    }
    let mut runtime_roots = if root_bound == 0 {
        None
    } else {
        if count.saturating_mul(2 * std::mem::size_of::<usize>() + 2) as u64
            > raw.budget.max_temporary_bytes
        {
            return Err(limit(None, "CT runtime root inventory scratch exhausted"));
        }
        meter.charge(count, None)?;
        Some(vec![false; count])
    };
    let mut weights: Option<Vec<usize>> = None;
    for (index, scope) in raw.ambient_life_scopes.iter().enumerate() {
        meter.charge(
            scope
                .binder_site
                .len()
                .saturating_add(count)
                .saturating_add(32),
            None,
        )?;
        if scope.binder_site.len() > limits.max_call_depth {
            return Err(limit(None, "CT binder-site depth exceeds quota"));
        }
        let weight = if scope.enclosing.is_some()
            || matches!(scope.instance, CtScopeInstance::Instantiated(_))
        {
            if weights.is_none() {
                // State, comparison weights and dependency heights coexist.
                let bytes = count.saturating_mul(2 * std::mem::size_of::<usize>() + 1);
                if bytes as u64 > raw.budget.max_temporary_bytes {
                    return Err(limit(
                        None,
                        "CT scope comparison inventory scratch exhausted",
                    ));
                }
                meter.charge(count.saturating_mul(2), None)?;
                weights = Some(vec![0; count]);
            }
            let weight = identity::scope_comparison_weight(
                scope,
                raw,
                limits,
                meter,
                count.saturating_mul(2 * std::mem::size_of::<usize>() + 1),
            )?;
            weights.as_mut().unwrap()[index] = weight;
            weight
        } else {
            0
        };
        if let Some(roots) = &mut runtime_roots {
            // Complete structural equality includes every nested actual graph.
            meter.charge(
                weight
                    .saturating_add(scope.binder_site.len())
                    .saturating_add(32)
                    .saturating_mul(root_bound),
                None,
            )?;
            roots[index] = authority.authorized_runtime_scope_root(scope);
        }
        for (prior_index, prior) in raw.ambient_life_scopes[..index].iter().enumerate() {
            meter.charge(
                prior
                    .binder_site
                    .len()
                    .saturating_add(scope.binder_site.len())
                    .saturating_add(32),
                None,
            )?;
            if prior.declaration == scope.declaration
                && matches!(prior.instance, CtScopeInstance::Instantiated(_))
                && matches!(scope.instance, CtScopeInstance::Instantiated(_))
            {
                meter.charge(
                    weight.saturating_add(weights.as_ref().unwrap()[prior_index]),
                    None,
                )?;
            }
            if prior == scope {
                return Err(reject(None, "CT ambient scope is duplicated"));
            }
        }
        charge_lookup(scope, weight, lookup_bound, meter)?;
        let parameters = authority.scope_parameter_count(scope).ok_or_else(|| {
            reject(
                None,
                "CT ambient scope is not an actual active source binder",
            )
        })?;
        if parameters > limits.max_nodes {
            return Err(limit(None, "CT ambient parameter inventory exceeds quota"));
        }
        meter.charge(
            parameters.saturating_mul(
                count
                    .saturating_add(scope.binder_site.len())
                    .saturating_add(32),
            ),
            None,
        )?;
        let actuals = match &scope.instance {
            CtScopeInstance::Definition => None,
            CtScopeInstance::Instantiated(actuals) => {
                if actuals.len() != parameters {
                    return Err(reject(
                        None,
                        "CT instantiated full actual vector arity differs",
                    ));
                }
                Some(actuals)
            }
        };
        let mut lifetime = false;
        for ordinal in 0..parameters {
            charge_lookup(scope, weight, lookup_bound, meter)?;
            let kind = authority
                .scope_parameter_kind(scope, ordinal)
                .ok_or_else(|| {
                    reject(None, "CT ambient source parameter inventory is incomplete")
                })?;
            lifetime |= kind == CtScopeParameterKind::Lifetime;
            if let Some(actuals) = actuals {
                match (kind, &actuals[ordinal]) {
                    (CtScopeParameterKind::Type, CtScopeActual::Type(_))
                    | (CtScopeParameterKind::Lifetime, CtScopeActual::Lifetime(_)) => {}
                    (CtScopeParameterKind::Scalar(domain), CtScopeActual::Scalar(value)) => {
                        if constant_type(value) != Some(domain) {
                            return Err(reject(None, "CT instantiated scalar domain differs"));
                        }
                        if matches!(value, Constant::Usize(n) if raw.target_pointer_width == 32 && *n > u64::from(u32::MAX))
                        {
                            return Err(reject(None, "CT instantiated usize exceeds target width"));
                        }
                    }
                    _ => {
                        return Err(reject(
                            None,
                            "CT instantiated actual kind differs at full original ordinal",
                        ));
                    }
                }
            }
        }
        if !lifetime && !runtime_roots.as_ref().is_some_and(|roots| roots[index]) {
            meter.charge(count, None)?;
            if !raw
                .ambient_life_scopes
                .iter()
                .any(|child| child.enclosing == Some(CtAmbientScopeId(index as u32)))
            {
                return Err(reject(
                    None,
                    "CT ambient scope without life binders is not a lexical host",
                ));
            }
        }
    }
    let mut visit = Visit {
        raw,
        limits,
        authority,
        meter,
        state: vec![0; count],
        next: 0,
        heights: weights.as_ref().map(|_| vec![0; count]),
        weights,
        lookup_bound,
    };
    for (arguments, context) in raw
        .record_schemas
        .iter()
        .map(|s| (&s.arguments, &s.life_context))
        .chain(
            raw.enum_schemas
                .iter()
                .map(|s| (&s.arguments, &s.life_context)),
        )
    {
        visit
            .meter
            .charge(arguments.len().saturating_add(1), None)?;
        if let Some(context) = context {
            visit.graph(context, 0)?;
        }
        for argument in arguments {
            if let CtSchemaArgument::Type(graph) = argument {
                visit.graph(graph, 0)?;
            }
        }
    }
    for template in &raw.templates {
        visit.meter.charge(template.syntax.nodes.len(), None)?;
        for node in &template.syntax.nodes {
            if let CtSyntaxNode::Token(CtToken {
                name:
                    Some(CtIdent {
                        member_owner_type: Some(graph),
                        ..
                    }),
                ..
            }) = node
            {
                visit.graph(graph, 0)?;
            }
        }
    }
    for function in &raw.functions {
        for instruction in function.blocks.iter().flat_map(|block| &block.instructions) {
            if let CtInstructionKind::TypeInContext { ty } = &instruction.kind {
                visit.graph(ty, 0)?;
            }
        }
    }
    if let Some(roots) = runtime_roots {
        visit.meter.charge(count, None)?;
        for (index, authorized) in roots.into_iter().enumerate() {
            if authorized && visit.state[index] != 2 {
                visit.scope(index, 0)?;
            }
        }
    }
    if visit.next != count {
        return Err(reject(
            None,
            "CT ambient scope inventory contains unused rows",
        ));
    }
    // The later whole-artifact authenticator compares this new nested table
    // once more. Return its complete payload bound for that distinct visit.
    Ok(visit.weights.as_ref().map_or(0, |weights| {
        weights.iter().copied().fold(0usize, usize::saturating_add)
    }))
}
