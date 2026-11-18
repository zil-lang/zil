//! Call-local outer regions retain every original source. Their identities
//! come from the frozen Source occurrence directory, never the first input's
//! image. These directed bounds cannot become nominal lifetime equalities.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Role {
    Life(u32),
    Implicit(u32),
    Head(u32),
    Invocation(u32),
    Bound { depth: u32, index: u32 },
    ConstructorLife(u32),
    TemporaryDescription(u32),
}
impl Role {
    fn suffix(self) -> ([u32; 4], usize) {
        let (kind, ordinal) = match self {
            Self::Life(ordinal) => (0, ordinal),
            Self::Implicit(ordinal) => (1, ordinal),
            Self::Head(ordinal) => (2, ordinal),
            Self::Invocation(ordinal) => (3, ordinal),
            Self::TemporaryDescription(ordinal) => (5, ordinal),
            Self::Bound { depth, index } => return ([u32::MAX, 4, depth, index], 4),
            Self::ConstructorLife(ordinal) => return ([u32::MAX - 1, 0, ordinal, 0], 3),
        };
        ([u32::MAX, kind, ordinal, 0], 3)
    }
}

#[derive(Clone)]
pub(crate) struct CallRegion {
    pub call: NodeId,
    role: Role,
    pub sources: Vec<LocalRoleSource>,
    pub escapes: bool,
    pub required_outlives: Vec<SemanticLifeRecipe>,
    pub inputs: BTreeMap<u32, NodeId>,
}

pub(super) struct Input<'a> {
    pub variable: Option<Variable>,
    pub selector: Option<Role>,
    pub expression: &'a SemanticOriginalExpr,
    pub actual: SemanticTypeRecipe,
    pub expected: SemanticOuterLifeRecipe,
    pub parameter: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Variable {
    Life(u32),
    Implicit(u32),
    Invocation(u32),
}
pub(super) fn variable(lifetime: &SemanticOuterLifeRecipe) -> Option<Variable> {
    match lifetime {
        SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Formal(reference))
            if reference.scope == SemanticScopeRef::Ancestor(u32::MAX) =>
        {
            Some(Variable::Life(reference.ordinal))
        }
        SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit { scope, ordinal })
            if *scope == SemanticScopeRef::Ancestor(u32::MAX) =>
        {
            Some(Variable::Implicit(*ordinal))
        }
        SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Environment { scope, ordinal })
            if *scope == SemanticScopeRef::Ancestor(u32::MAX) =>
        {
            Some(Variable::Invocation(*ordinal))
        }
        _ => None,
    }
}

impl Check<'_, '_> {
    pub(super) fn region_key(
        &mut self,
        mut owner: SemanticDefinitionId,
        lifetime: &SemanticOuterLifeRecipe,
        depth: usize,
    ) -> R<Option<(SemanticDefinitionId, u32)>> {
        self.pay(1, depth)?;
        let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
            scope: SemanticScopeRef::Ancestor(distance),
            ordinal,
        }) = lifetime
        else {
            return Ok(None);
        };
        if *distance == u32::MAX {
            return Ok(None);
        }
        for _ in 0..*distance {
            self.pay(1, depth)?;
            owner = self
                .definition(owner)?
                .parent
                .ok_or_else(|| bad("call region leaves its real lexical owner"))?;
        }
        self.pay(1, depth)?;
        Ok(self
            .proof
            .call_regions
            .contains_key(&(owner, *ordinal))
            .then_some((owner, *ordinal)))
    }

    pub(super) fn record_region_source(
        &mut self,
        owner: SemanticDefinitionId,
        region: &SemanticOuterLifeRecipe,
        expression: &SemanticOriginalExpr,
        lifetime: &SemanticOuterLifeRecipe,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        let Some(key) = self.region_key(owner, region, depth + 1)? else {
            return Ok(());
        };
        if region == lifetime {
            return Ok(());
        }
        if key.0 != owner {
            return Err(bad(
                "call region sources cannot be extended outside their original lexical owner",
            ));
        }
        let escape = self.role_source(owner, expression, lifetime, variables, depth + 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(lifetime, self.limits, self.left, 1)?;
        let source = LocalRoleSource {
            at: SemanticOriginalNode::Expression(expression.id),
            lifetime: lifetime.clone(),
            escape,
        };
        let count = self
            .proof
            .call_regions
            .get(&key)
            .ok_or_else(|| bad("call region source directory disappeared"))?
            .sources
            .len();
        self.pay(count.saturating_add(1), depth)?;
        let row = self
            .proof
            .proof_mut()?
            .call_regions
            .get_mut(&key)
            .ok_or_else(|| bad("call region source directory disappeared"))?;
        if !row.sources.contains(&source) {
            row.sources.push(source);
        }
        Ok(())
    }

    pub(super) fn call_region_life(
        &mut self,
        owner: SemanticDefinitionId,
        call: NodeId,
        role: Role,
        depth: usize,
    ) -> R<SemanticOuterLifeRecipe> {
        self.pay(1, depth)?;
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &definition.body
        else {
            return Err(bad("call region has no complete original function"));
        };
        self.pay(site_nodes.len().saturating_add(sites.len()), depth)?;
        let mut selected = None;
        for node in site_nodes {
            if node.node != SemanticOriginalNode::Expression(call) {
                continue;
            }
            for site in sites {
                self.pay(1, depth)?;
                let matching_kind = if matches!(role, Role::ConstructorLife(_)) {
                    matches!(site.kind, SemanticSiteRecipeKind::Construct { .. })
                } else {
                    matches!(site.kind, SemanticSiteRecipeKind::OriginalCall)
                };
                if node.site == site.id && matching_kind {
                    if selected.replace(site).is_some() {
                        return Err(bad("call region has several original call occurrences"));
                    }
                }
            }
        }
        let site = selected.ok_or_else(|| bad("call region has no exact original Call site"))?;
        let (suffix, length) = role.suffix();
        let expected_length = site
            .occurrence_path
            .len()
            .checked_add(length)
            .ok_or_else(|| bad("call region occurrence length overflows"))?;
        if expected_length > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "call region occurrence depth exceeded",
                None,
            ));
        }
        let mut ordinal = None;
        for row in &definition.implicit_lifetimes {
            self.pay(row.occurrence_path.len().saturating_add(1), depth)?;
            if row.occurrence_path.len() == expected_length
                && row.occurrence_path.starts_with(&site.occurrence_path)
                && row.occurrence_path[site.occurrence_path.len()..] == suffix[..length]
                && ordinal.replace(row.ordinal).is_some()
            {
                return Err(bad("call region repeats its complete Source role selector"));
            }
        }
        let ordinal = ordinal.ok_or_else(|| {
            bad("call region lacks its independent full-ordinal Source occurrence")
        })?;
        if matches!(role, Role::TemporaryDescription(_)) {
            // This coordinate identifies an actual full-expression place.
            // It is a source of the call loan and never another call region.
            return Ok(SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0), ordinal,
            }));
        }
        self.pay(1, depth)?;
        if let Some(previous) = self.proof.call_regions.get(&(owner, ordinal)) {
            if previous.call != call || previous.role != role {
                return Err(bad(
                    "call region is reused by another original call or role",
                ));
            }
        } else {
            self.proof.proof_mut()?.call_regions.insert(
                (owner, ordinal),
                CallRegion {
                    call,
                    role,
                    sources: Vec::new(),
                    escapes: false,
                    required_outlives: Vec::new(),
                    inputs: BTreeMap::new(),
                },
            );
        }
        Ok(SemanticOuterLifeRecipe::Class(
            SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            },
        ))
    }
}

fn outer(ty: &SemanticTypeRecipe) -> R<&SemanticOuterLifeRecipe> {
    match ty {
        SemanticTypeRecipe::Address { lifetime, .. }
        | SemanticTypeRecipe::Desc { lifetime, .. } => Ok(lifetime),
        _ => Err(bad("call region input loses its original borrowed carrier")),
    }
}
impl Check<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn settle_call_inputs(
        &mut self,
        owner: SemanticDefinitionId,
        call: &SemanticOriginalExpr,
        inputs: &[Input<'_>],
        formals: &[SemanticFormal],
        solutions: &mut [Option<SemanticActualRecipe>],
        implicit: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        invocation: &mut BTreeMap<u32, SemanticOuterLifeRecipe>,
        variables: &BTreeMap<String, Binding>,
        depth: usize,
    ) -> R<()> {
        self.pay(inputs.len(), depth)?;
        let mut settled = BTreeMap::new();
        for input in inputs {
            let Some(variable) = input.variable else {
                continue;
            };
            self.pay(1, depth)?;
            if settled.contains_key(&variable) {
                continue;
            }
            let existing = match variable {
                Variable::Life(ordinal) => match solutions.get(ordinal as usize) {
                    Some(Some(SemanticActualRecipe::Life(life))) => {
                        Some(SemanticOuterLifeRecipe::Class(life.clone()))
                    }
                    Some(None) => None,
                    _ => {
                        return Err(bad(
                            "call outer role changes its complete mixed formal kind",
                        ));
                    }
                },
                Variable::Implicit(ordinal) => implicit.get(&ordinal).cloned(),
                Variable::Invocation(ordinal) => invocation.get(&ordinal).cloned(),
            };
            let selected = if let Some(existing) = existing {
                existing
            } else {
                // An exact common *original input* is already a rigid caller
                // contract. Every source is checked, including later arguments.
                // Local storage and inferred bindings always receive a distinct
                // call region, even when initially supplied by one source.
                let first = outer(&input.actual)?;
                let mut common_input = !matches!(first, SemanticOuterLifeRecipe::Static)
                    && self.input_lifetime(owner, first, depth + 1)?.is_some();
                for source in inputs
                    .iter()
                    .filter(|source| source.variable == Some(variable))
                {
                    self.pay(1, depth)?;
                    let lifetime = outer(&source.actual)?;
                    let equal = match (first, lifetime) {
                        (SemanticOuterLifeRecipe::Class(a), SemanticOuterLifeRecipe::Class(b)) => {
                            self.same_life(a, b)?
                        }
                        _ => false,
                    };
                    let escape =
                        self.role_source(owner, source.expression, lifetime, variables, depth + 1)?;
                    common_input &= equal && matches!(escape, OriginalRoleEscape::Input { .. });
                }
                if common_input {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        first,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    first.clone()
                } else {
                    self.call_region_life(
                        owner,
                        call.id,
                        input.selector.ok_or_else(|| {
                            bad("call outer inference omits its original role selector")
                        })?,
                        depth + 1,
                    )?
                }
            };
            self.unify_outer(
                &input.expected,
                &selected,
                formals,
                solutions,
                implicit,
                invocation,
                depth + 1,
            )?;
            settled.insert(variable, selected);
        }
        // Publish every directed source before checking any use, so this is
        // independent of argument order. No source-to-region equation is an Eq.
        for input in inputs {
            let target = input
                .variable
                .and_then(|v| settled.get(&v))
                .unwrap_or(&input.expected);
            if let Some(key) = self.region_key(owner, target, depth + 1)? {
                if key.0 == owner {
                    self.pay(1, depth)?;
                    let row = self
                        .proof
                        .proof_mut()?
                        .call_regions
                        .get_mut(&key)
                        .ok_or_else(|| bad("call region lost its argument directory"))?;
                    if row.call != call.id {
                        // Another argument may have fixed this lifetime to a
                        // prior call. Its source directory belongs to that call.
                        continue;
                    }
                    if row.inputs.insert(input.parameter, input.expression.id)
                        .is_some_and(|old| old != input.expression.id) {
                        return Err(bad("call region changes its exact original argument occurrence"));
                    }
                }
            }
            self.record_region_source(
                owner,
                target,
                input.expression,
                outer(&input.actual)?,
                variables,
                depth + 1,
            )?;
        }
        for input in inputs {
            let target = input
                .variable
                .and_then(|v| settled.get(&v))
                .unwrap_or(&input.expected);
            let source = outer(&input.actual)?;
            if source == target {
                continue;
            }
            self.pay(self.proof.temporary_descriptions.len().saturating_add(1).ilog2() as usize + 1, depth)?;
            if self.proof.temporary_descriptions.get(&(owner, call.id, input.parameter))
                .is_some_and(|receipt| receipt.argument == input.expression.id) {
                // The original temporary source has its own complete argument
                // receipt. World must prove its actual fresh store and Desc.
                continue;
            }
            if matches!(source, SemanticOuterLifeRecipe::Static) {
                // The literal/source remains Static. Actual call admission
                // independently proves its endpoint and complete pointee.
                continue;
            }
            let inner = match &input.actual {
                SemanticTypeRecipe::Address { inner, .. }
                | SemanticTypeRecipe::Desc { inner, .. } => inner.as_ref(),
                _ => unreachable!(),
            };
            let operand = match &input.expression.kind {
                CoreExprKind::Unary {
                    op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                    operand,
                } => operand.as_ref(),
                _ => input.expression,
            };
            if let Some(key) = self.region_key(owner, target, depth + 1)? {
                let region = self.proof.call_regions.get(&key)
                    .ok_or_else(|| bad("inherited argument lost its original call"))?;
                if region.call != call.id {
                    let role = self.projected_borrow_role(owner, input.expression, operand, variables,
                        inner, source, target, true, false, depth + 1)?
                        .ok_or_else(|| bad("inherited call loan lacks its actual source role"))?;
                    let outlives = match (source, target) {
                        (SemanticOuterLifeRecipe::Class(source), SemanticOuterLifeRecipe::Class(target)) =>
                            self.life_outlives(owner, source, target, depth + 1)?,
                        _ => false,
                    };
                    if !outlives {
                        let direct_local = |role: &BorrowRole| match (role.root, role.steps.as_slice()) {
                            (ViewBorrowRoot::Local(root), [ViewBorrowStep { kind: ViewBorrowStepKind::Local { binding }, .. }])
                                if *binding == root => Some(root),
                            _ => None,
                        };
                        let current = direct_local(&role);
                        let count = self.proof.call_regions[&key].inputs.len();
                        self.pay(count.saturating_mul(self.proof.borrow_roles.len().saturating_add(1).ilog2() as usize + 1), depth)?;
                        let region = &self.proof.call_regions[&key];
                        if key.0 != owner || operand.id == input.expression.id || current.is_none()
                            || !region.inputs.values().any(|prior| self.proof.borrow_roles.get(&(owner, *prior))
                                .and_then(direct_local) == current) {
                            return Err(bad("inherited call loan selects a different original storage cell"));
                        }
                    }
                    let proof = self.proof.proof_mut()?;
                    proof.borrow_roles.insert((owner, input.expression.id), role);
                    proof.call_borrows.insert((owner, call.id, input.parameter), input.expression.id);
                    continue;
                }
            }
            self.contextual_borrow_role(
                owner,
                input.expression,
                operand,
                variables,
                inner,
                source,
                target,
                depth + 1,
            )?;
            if let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            }) = target
            {
                self.pay(
                    self.proof.call_regions.len().saturating_add(1).ilog2() as usize + 1,
                    depth,
                )?;
                if !self.proof.call_regions.contains_key(&(owner, *ordinal)) {
                    self.pay(
                        self.proof.call_borrows.len().saturating_add(1).ilog2() as usize + 1,
                        depth,
                    )?;
                    let key = (owner, call.id, input.parameter);
                    if let Some(prior) = self.proof.call_borrows.get(&key) {
                        if *prior != input.expression.id {
                            return Err(bad(
                                "one original contextual call input changes its source occurrence",
                            ));
                        }
                    } else {
                        if self.proof.call_borrows.len() >= self.limits.max_nodes {
                            return Err(Diagnostic::limit(
                                Phase::Types,
                                "original contextual call input limit exceeded",
                                None,
                            ));
                        }
                        self.proof
                            .proof_mut()?
                            .call_borrows
                            .insert(key, input.expression.id);
                    }
                }
            }
            // This is a call-boundary loan. Keep the source expression's
            // original complete sigma; its independent TypeUse still checks
            // the actual read. Only the selected call parameter uses r.
        }
        Ok(())
    }

    pub(super) fn escape_call_region(
        &mut self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        active: &mut BTreeSet<(SemanticDefinitionId, Option<NodeId>, Option<u32>)>,
        depth: usize,
    ) -> R<()> {
        self.pay(1, depth)?;
        if !active.insert((owner, None, Some(ordinal))) {
            return Err(bad("escaping call region has a cyclic source equation"));
        }
        let row = self
            .proof
            .call_regions
            .get(&(owner, ordinal))
            .ok_or_else(|| bad("escaping call region has no original source receipt"))?;
        self.pay(row.sources.len(), depth)?;
        let row = self
            .proof
            .call_regions
            .get(&(owner, ordinal))
            .ok_or_else(|| bad("escaping call region disappeared"))?;
        let sources = row
            .sources
            .iter()
            .map(|source| source.escape.clone())
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return Err(bad("escaping call region has no complete incoming sources"));
        }
        for source in &sources {
            self.escape_source(source, active, depth + 1)?;
        }
        active.remove(&(owner, None, Some(ordinal)));
        self.proof
            .proof_mut()?
            .call_regions
            .get_mut(&(owner, ordinal))
            .ok_or_else(|| bad("escaping call region disappeared"))?
            .escapes = true;
        Ok(())
    }
}

impl Check<'_, '_> {
    pub(super) fn call_region_outlives(
        &mut self,
        owner: SemanticDefinitionId,
        source: &SemanticLifeRecipe,
        target: &SemanticLifeRecipe,
        active: &mut Vec<(SemanticLifeRecipe, SemanticLifeRecipe)>,
        depth: usize,
    ) -> R<bool> {
        self.pay(active.len().saturating_add(1), depth)?;
        if self.same_life(source, target)? {
            return Ok(true);
        }
        if active.iter().any(|(a, b)| a == source && b == target) {
            return Ok(false);
        }
        crate::dyn_metadata::semantic_operand_snapshot(source, self.limits, self.left, 1)?;
        crate::dyn_metadata::semantic_operand_snapshot(target, self.limits, self.left, 1)?;
        active.push((source.clone(), target.clone()));
        let result = self.call_region_outlives_inner(owner, source, target, active, depth + 1);
        active.pop();
        result
    }
    fn call_region_outlives_inner(
        &mut self,
        owner: SemanticDefinitionId,
        source: &SemanticLifeRecipe,
        target: &SemanticLifeRecipe,
        active: &mut Vec<(SemanticLifeRecipe, SemanticLifeRecipe)>,
        depth: usize,
    ) -> R<bool> {
        let source_outer = SemanticOuterLifeRecipe::Class(source.clone());
        let target_outer = SemanticOuterLifeRecipe::Class(target.clone());
        if let Some(key) = self.region_key(owner, &target_outer, depth + 1)? {
            // Only same-owner recipes can be read without an explicit lexical
            // substitution. Captured ancestor facts keep their own coordinates.
            if key.0 == owner {
                let sources = self.region_source_lifetimes(key, depth + 1)?;
                for incoming in sources {
                    if let SemanticOuterLifeRecipe::Class(incoming) = incoming {
                        if self.call_region_outlives(owner, source, &incoming, active, depth + 1)? {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        if let Some(sources) = self.local_role_source_lifetimes(owner, target, depth + 1)? {
            for incoming in sources {
                if let SemanticOuterLifeRecipe::Class(incoming) = incoming {
                    if self.call_region_outlives(owner, source, &incoming, active, depth + 1)? {
                        return Ok(true);
                    }
                }
            }
        }
        if let Some(key) = self.region_key(owner, &source_outer, depth + 1)? {
            if key.0 == owner {
                let sources = self.region_source_lifetimes(key, depth + 1)?;
                let mut all = !sources.is_empty();
                for incoming in sources {
                    all &= match incoming {
                        SemanticOuterLifeRecipe::Static => true,
                        SemanticOuterLifeRecipe::Class(incoming) => {
                            self.call_region_outlives(owner, &incoming, target, active, depth + 1)?
                        }
                    };
                }
                if all {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        target,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    let count = self
                        .proof
                        .call_regions
                        .get(&key)
                        .ok_or_else(|| bad("call region bound disappeared"))?
                        .required_outlives
                        .len();
                    self.pay(count.saturating_add(1), depth)?;
                    let row = self
                        .proof
                        .proof_mut()?
                        .call_regions
                        .get_mut(&key)
                        .ok_or_else(|| bad("call region bound lost its original sources"))?;
                    if !row.required_outlives.contains(target) {
                        row.required_outlives.push(target.clone());
                    }
                    return Ok(true);
                }
            }
        }
        let requirements = self.original_requirements(owner, depth + 1)?;
        for requirement in requirements.iter() {
            self.pay(1, depth)?;
            if let SemanticRequirementRecipe::Outlives { longer, shorter } = requirement {
                if self.same_life(source, longer)?
                    && self.call_region_outlives(owner, shorter, target, active, depth + 1)?
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    fn region_source_lifetimes(
        &mut self,
        key: (SemanticDefinitionId, u32),
        depth: usize,
    ) -> R<Vec<SemanticOuterLifeRecipe>> {
        let row = self
            .proof
            .call_regions
            .get(&key)
            .ok_or_else(|| bad("call region has no complete sources"))?;
        for source in &row.sources {
            crate::dyn_metadata::semantic_operand_snapshot(
                &source.lifetime,
                self.limits,
                self.left,
                1,
            )?;
        }
        Ok(row.sources.iter().map(|s| s.lifetime.clone()).collect())
    }
    pub(super) fn recheck_call_region_bounds(
        &mut self,
        owner: SemanticDefinitionId,
        depth: usize,
    ) -> R<()> {
        self.pay(self.proof.call_regions.len(), depth)?;
        let mut obligations = Vec::new();
        for ((region_owner, ordinal), row) in &self.proof.call_regions {
            if *region_owner == owner {
                for target in &row.required_outlives {
                    crate::dyn_metadata::semantic_operand_snapshot(
                        target,
                        self.limits,
                        self.left,
                        1,
                    )?;
                    obligations.push((*ordinal, target.clone()));
                }
            }
        }
        for (ordinal, target) in obligations {
            let source = SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            };
            if !self.call_region_outlives(owner, &source, &target, &mut Vec::new(), depth + 1)? {
                return Err(bad(
                    "call region's complete updated sources do not outlive its required use",
                ));
            }
        }
        Ok(())
    }
}

impl Check<'_, '_> {
    pub(super) fn static_call_region_sources(
        &mut self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        active: &mut BTreeSet<(SemanticDefinitionId, Option<NodeId>, Option<u32>)>,
        depth: usize,
    ) -> R<bool> {
        self.pay(active.len().saturating_add(1), depth)?;
        if !active.insert((owner, None, Some(ordinal))) {
            return Err(bad("static call region has a cyclic source equation"));
        }
        let row = self
            .proof
            .call_regions
            .get(&(owner, ordinal))
            .ok_or_else(|| bad("static call region has no original source receipt"))?;
        self.pay(row.sources.len(), depth)?;
        let row = self
            .proof
            .call_regions
            .get(&(owner, ordinal))
            .ok_or_else(|| bad("static call region disappeared"))?;
        let sources = row
            .sources
            .iter()
            .map(|source| source.escape.clone())
            .collect::<Vec<_>>();
        let mut all = !sources.is_empty();
        for source in sources {
            all &= match source {
                OriginalRoleEscape::Static => true,
                OriginalRoleEscape::Role { owner, binding } => {
                    self.static_role_sources(owner, binding, active, depth + 1)?
                }
                OriginalRoleEscape::CallRegion { owner, ordinal } => {
                    self.static_call_region_sources(owner, ordinal, active, depth + 1)?
                }
                OriginalRoleEscape::Input { .. } | OriginalRoleEscape::LocalPlace { .. } => false,
            };
        }
        active.remove(&(owner, None, Some(ordinal)));
        Ok(all)
    }
}
