//! Original call-local outer regions. Source supplies occurrence coordinates;
//! this kernel independently reconstructs all incoming sources and directed
//! bounds. Neither a concrete class image nor a loan equates two lifetimes.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Life(u32),
    Implicit(u32),
    Head(u32),
    TemporaryDescription(u32),
    Invocation(u32),
    Bound(u32, u32),
    ConstructorLife(u32),
}
impl Role {
    fn suffix(self) -> ([u32; 4], usize) {
        match self {
            Self::Life(n) => ([u32::MAX, 0, n, 0], 3),
            Self::Implicit(n) => ([u32::MAX, 1, n, 0], 3),
            Self::Head(n) => ([u32::MAX, 2, n, 0], 3),
            Self::TemporaryDescription(n) => ([u32::MAX, 5, n, 0], 3),
            Self::Invocation(n) => ([u32::MAX, 3, n, 0], 3),
            Self::Bound(d, n) => ([u32::MAX, 4, d, n], 4),
            Self::ConstructorLife(n) => ([u32::MAX - 1, 0, n, 0], 3),
        }
    }
}

pub(in crate::semantic) fn is_region_occurrence(recipe: &SemanticSiteRecipe, path: &[u32]) -> bool {
    let Some(suffix) = path.strip_prefix(recipe.occurrence_path.as_slice()) else {
        return false;
    };
    match recipe.kind {
        SemanticSiteRecipeKind::OriginalCall => matches!(suffix,
            [u32::MAX, 0..=3 | 5, _] | [u32::MAX, 4, _, _]),
        SemanticSiteRecipeKind::Construct { .. } =>
            matches!(suffix, [marker, 0, _] if *marker == u32::MAX - 1),
        _ => false,
    }
}
#[derive(Clone)]
pub(super) struct Source {
    pub node: NodeId,
    pub life: Option<Term>, // None is an independently checked Static source.
    pub input: bool,
    pub anchor: Option<LocalAnchor>,
    direct_local: Option<NodeId>,
    pub upper: Option<Term>,
}
pub(super) struct Region {
    pub call: NodeId,
    pub role: Role,
    pub sources: Vec<Source>,
}
pub(super) struct Use {
    pub source: Source,
    pub target: Term,
    pub ordinal: u32,
}
pub(super) struct Input {
    pub source: Source,
    pub ordinal: u32,
    pub life: Term,
    pub role: Option<Role>,
}
impl Solver<'_, '_> {
    pub(super) fn call_variable(&mut self, role: Role, b: &mut Budget) -> CompileResult<Term> {
        let term = self.term(Kind::LifeVariable(None), b)?;
        b.lookup(self.call_variables.len(), WORK)?;
        self.call_variables.insert(term, role);
        Ok(term)
    }
    pub(super) fn mark_call_variable(
        &mut self,
        term: Term,
        role: Role,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.lookup(self.call_variables.len(), WORK)?;
        if self
            .call_variables
            .insert(term, role)
            .is_some_and(|old| old != role)
        {
            return Err(fail(
                "original call variable has competing declaration role coordinates",
            ));
        }
        Ok(())
    }
    pub(super) fn mark_call_parameter(
        &mut self,
        term: Term,
        recipe: &SemanticTypeRecipe,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if matches!(
            recipe,
            SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. }
        ) {
            b.lookup(self.call_parameters.len(), WORK)?;
            self.call_parameters.insert(term);
        }
        Ok(())
    }
    pub(super) fn deferred_call_type(
        &mut self,
        expected: Term,
        b: &mut Budget,
    ) -> CompileResult<Term> {
        b.lookup(self.call_parameters.len(), WORK)?;
        if !self.call_parameters.contains(&expected) {
            return Ok(expected);
        }
        let root = self.type_root(expected, b)?;
        let fresh = match self.state.arena[root] {
            Kind::Address { mutable, inner, .. } => {
                let life = self.term(Kind::LifeVariable(None), b)?;
                Some(Kind::Address {
                    mutable,
                    life,
                    inner,
                })
            }
            Kind::Desc { inner, .. } => {
                let life = self.term(Kind::LifeVariable(None), b)?;
                Some(Kind::Desc { life, inner })
            }
            _ => None,
        };
        match fresh {
            Some(kind) => self.term(kind, b),
            None => Ok(expected),
        }
    }
    pub(super) fn call_occurrence(
        &mut self,
        frame: &Frame,
        call: NodeId,
        role: Role,
        prototype: Option<SemanticDefinitionId>,
        b: &mut Budget,
    ) -> CompileResult<Term> {
        let index = self.inventory.original_index(frame.definition, b)?;
        let event = match role {
            Role::ConstructorLife(_) => original_index::EventRole::Construct,
            _ => original_index::EventRole::Call,
        };
        let absent = || fail("call region has no real original Call occurrence");
        let site = index.recipe(index.event_site(call, event, b)?.ok_or_else(absent)?, b)?;
        if !matches!(role, Role::ConstructorLife(_)) && !matches!(site.kind, SemanticSiteRecipeKind::OriginalCall) {
            return Err(absent());
        }
        let (suffix, length) = role.suffix();
        let size = site
            .occurrence_path
            .len()
            .checked_add(length)
            .ok_or_else(|| resource("call region occurrence length overflow"))?;
        if size > b.partial_depth {
            return Err(resource(
                "call region occurrence exceeds its complete depth",
            ));
        }
        b.spend(size.saturating_add(1), WORK)?;
        let mut path = Vec::new();
        path.try_reserve_exact(size).map_err(|_| resource("call region occurrence allocation failed"))?;
        path.extend_from_slice(&site.occurrence_path);
        path.extend_from_slice(&suffix[..length]);
        let ordinal = self.inventory.implicit_ordinal(frame.definition, &path, b)?;
        let ordinal = match ordinal {
            Some(ordinal) => ordinal,
            None => {
                b.spend(size.saturating_mul(16).saturating_add(512), WORK)?;
                let mut errors = fail("call region lacks its complete Source role occurrence");
                errors[0].notes.push(format!(
                    "original call region owner {}:{} expression {}; role kind {} full ordinal {} bound depth {:?}; original Call path {:?}; required suffix {:?}; original prototype {:?}",
                    frame.definition.module.0, frame.definition.index, call.0,
                    suffix[1], suffix[length - 1], (length == 4).then_some(suffix[2]),
                    site.occurrence_path, &suffix[..length], prototype,
                ));
                if let Some(prototype) = prototype {
                    let original = self.inventory.definition(prototype, b)?;
                    b.spend(
                        original
                            .identity
                            .declaration_path
                            .iter()
                            .map(|name| name.len().saturating_mul(10).saturating_add(8))
                            .sum::<usize>(),
                        WORK,
                    )?;
                    errors[0].notes.push(format!(
                        "original call prototype {}:{} declaration path {:?}",
                        prototype.module.0, prototype.index, original.identity.declaration_path
                    ));
                }
                return Err(errors);
            }
        };
        b.lookup(self.state.call_regions.len(), WORK)?;
        if let Some(previous) = self.state.call_regions.get(&(frame.definition, ordinal)) {
            if previous.call != call || previous.role != role {
                return Err(fail(
                    "call region occurrence belongs to another call or role",
                ));
            }
        } else {
            self.state.call_regions.insert(
                (frame.definition, ordinal),
                Region {
                    call,
                    role,
                    sources: Vec::new(),
                },
            );
        }
        self.term(Kind::Life(Life::Implicit(frame.definition, ordinal)), b)
    }
    pub(super) fn store_view_input(
        &self, frame: &Frame, expr: &SemanticOriginalExpr, b: &mut Budget,
    ) -> CompileResult<Option<u32>> {
        if self.state.intrinsic_anchors.is_empty() && self.state.store_view_inputs.is_empty() {
            return Ok(None);
        }
        let mut expr = expr;
        while let CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } = &expr.kind {
            b.spend(1, WORK)?;
            expr = inner;
        }
        let key = (frame.definition, expr.id);
        b.lookup(self.state.intrinsics.len(), WORK)?;
        let anchor = if matches!(self.state.intrinsics.get(&key),
            Some(sealed::Intrinsic::StoreAt | sealed::Intrinsic::StoreAtMut))
        {
            // The Store checker established this anchor from the original manager
            // place, including its field projection, before publishing the borrow.
            b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
            self.state.intrinsic_anchors.get(&key)
        } else {
            if self.state.store_view_inputs.is_empty() { return Ok(None); }
            b.lookup(self.state.local_reads.len(), WORK)?;
            let Some(LocalAnchor::Binding(binding)) = self.state.local_reads.get(&key) else {
                return Ok(None);
            };
            b.lookup(self.state.store_view_inputs.len(), WORK)?;
            return Ok(self.state.store_view_inputs.get(&(frame.definition, *binding)).copied());
        };
        match anchor {
            Some(LocalAnchor::Parameter(input)) => Ok(Some(*input)),
            Some(LocalAnchor::Binding(binding)) => {
                b.lookup(self.state.store_view_inputs.len(), WORK)?;
                Ok(self.state.store_view_inputs.get(&(frame.definition, *binding)).copied())
            }
            _ => Ok(None),
        }
    }
    fn original_input_source(
        &self,
        frame: &Frame,
        source: &Source,
        expr: &SemanticOriginalExpr,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let Some(life) = source.life else {
            return Ok(false);
        };
        let ordinal = match source.anchor {
            Some(LocalAnchor::Parameter(ordinal)) => ordinal,
            _ => match self.store_view_input(frame, expr, b)? {
                Some(ordinal) => ordinal,
                None => return Ok(false),
            },
        };
        let Some(parameter) = frame.parameters.get(ordinal as usize) else {
            return Err(fail(
                "call input source leaves its original parameter vector",
            ));
        };
        let candidate = match self.state.arena[self.type_root(*parameter, b)?] {
            Kind::Address { life, .. } | Kind::Desc { life, .. } => life,
            _ => return Ok(false),
        };
        self.same_original(life, candidate, b, 0)
    }
    pub(super) fn collect_call_input(
        &mut self,
        frame: &mut Frame,
        expression: &SemanticOriginalExpr,
        actual: Term,
        expected: Term,
        ordinal: usize,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Input>> {
        self.step(depth, b)?;
        b.lookup(self.call_parameters.len(), WORK)?;
        if !self.call_parameters.contains(&expected) {
            self.call_argument_value(actual, expected, b, depth + 1)?;
            return Ok(None);
        }
        let expected_root = self.type_root(expected, b)?;
        let life = match self.state.arena[expected_root] {
            Kind::Address { life, .. } | Kind::Desc { life, .. } => life,
            _ => {
                self.call_argument_value(actual, expected, b, depth + 1)?;
                return Ok(None);
            }
        };
        let deferred = self.deferred_call_type(expected, b)?;
        self.call_argument_value(actual, deferred, b, depth + 1)?;
        let actual_root = self.type_root(actual, b)?;
        let source_life = match self.state.arena[actual_root] {
            Kind::Address { life, .. } | Kind::Desc { life, .. } => Some(life),
            Kind::Atom(ref ty) if strings::is_byte_type(ty) => None,
            _ => {
                return Err(fail(
                    "call region source changes its complete borrowed constructor",
                ));
            }
        };
        let mut source = Source {
            node: expression.id,
            life: source_life,
            input: false,
            anchor: None,
            direct_local: None,
            upper: None,
        };
        let mut source_expression = expression;
        while let CoreExprKind::Group(inner) = &source_expression.kind {
            self.step(depth + 1, b)?;
            source_expression = inner;
        }
        b.lookup(self.state.borrows.len(), WORK)?;
        b.lookup(self.state.descriptions.len(), WORK)?;
        if let Some(borrow) = self
            .state
            .borrows
            .get(&(frame.definition, source_expression.id))
        {
            source.anchor = borrow.anchor;
            if let Some(LocalAnchor::Binding(local)) = borrow.anchor
                && !borrow.indirect && borrow.path.is_empty()
                && borrow.index.is_none() && borrow.slice.is_none() {
                source.direct_local = Some(local);
            }
            source.upper = borrow.role.as_ref().and_then(|role| role.upper);
        } else if let Some(description) = self
            .state
            .descriptions
            .get(&(frame.definition, source_expression.id))
        {
            source.anchor = Some(description.anchor);
            source.upper = description.role.upper;
        } else {
            source.anchor = self
                .place(frame, expression, false, b, depth + 1)?
                .and_then(|place| place.anchor);
        }
        if !self.state.prepared_address_inputs.is_empty() {
            b.lookup(self.state.prepared_address_inputs.len(), WORK)?;
            if let Some(anchor) = self.state.prepared_address_inputs.get(&(frame.definition, expression.id)) {
                source.anchor = Some(*anchor);
            }
        }
        source.input = self.original_input_source(frame, &source, expression, b)?;
        let ordinal =
            u32::try_from(ordinal).map_err(|_| resource("call parameter ordinal overflow"))?;
        b.lookup(self.call_variables.len(), WORK)?;
        let role = self
            .call_variables
            .get(&life)
            .copied()
            .map(|role| match role {
                Role::Implicit(_)
                    if matches!(self.state.arena[expected_root], Kind::Desc { .. }) =>
                {
                    Role::Head(ordinal)
                }
                role => role,
            });
        Ok(Some(Input {
            source,
            ordinal,
            life,
            role,
        }))
    }
    pub(super) fn settle_call_inputs(
        &mut self,
        frame: &mut Frame,
        call: NodeId,
        prototype: Option<SemanticDefinitionId>,
        inputs: &[Input],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let mut selected = BTreeMap::new();
        for input in inputs {
            let variable = self.root(input.life, b)?;
            b.lookup(selected.len(), WORK)?;
            if selected.contains_key(&variable) {
                continue;
            }
            let target = if matches!(self.state.arena[variable], Kind::LifeVariable(None)) {
                let mut common = input.source.input && input.source.life.is_some();
                for incoming in inputs {
                    b.spend(1, WORK)?;
                    if self.root(incoming.life, b)? != variable {
                        continue;
                    }
                    common &= incoming.source.input;
                    common &= match (input.source.life, incoming.source.life) {
                        (Some(first), Some(next)) => {
                            self.same_original(first, next, b, depth + 1)?
                        }
                        _ => false,
                    };
                }
                if common {
                    input.source.life.unwrap()
                } else {
                    self.call_occurrence(
                        frame,
                        call,
                        input.role.ok_or_else(|| {
                            fail("fresh call outer has no original declaration role")
                        })?,
                        prototype,
                        b,
                    )?
                }
            } else {
                variable
            };
            selected.insert(variable, target);
        }
        // Snapshot the original variable groups before solving any of them.
        let mut targets = Vec::new();
        for input in inputs {
            let variable = self.root(input.life, b)?;
            let target = *selected
                .get(&variable)
                .ok_or_else(|| fail("call outer group disappeared"))?;
            Self::push(&mut targets, target, b)?;
            if let Kind::Life(Life::Implicit(owner, ordinal)) =
                self.state.arena[self.root(target, b)?]
            {
                b.lookup(self.state.call_regions.len(), WORK)?;
                if let Some(region) = self.state.call_regions.get_mut(&(owner, ordinal)) {
                    if owner == frame.definition && region.call == call {
                        Self::push(&mut region.sources, input.source.clone(), b)?;
                    }
                }
            }
        }
        for (input, target) in inputs.iter().zip(targets) {
            b.lookup(self.state.call_uses.len(), WORK)?;
            let uses = self
                .state
                .call_uses
                .entry((frame.definition, call))
                .or_default();
            Self::push(
                uses,
                Use {
                    source: input.source.clone(),
                    target,
                    ordinal: input.ordinal,
                },
                b,
            )?;
            self.unify(input.life, target, depth + 1, b)?;
            if let Some(source) = input.source.life {
                if !self.call_source_allows(frame, source, target, b, depth + 1)?
                    && !self.local_storage_covers_call(frame, &input.source, target, b)? {
                    let mut errors = fail("original call outer exceeds one of its complete incoming source bounds");
                    let mut describe = |term: Term| -> CompileResult<String> {
                        Ok(match &self.state.arena[self.root(term, b)?] {
                            Kind::Life(life) => format!("{life:?}"),
                            kind => format!("term {} kind {:?}", term, std::mem::discriminant(kind)),
                        })
                    };
                    let source_note = describe(source)?;
                    let target_note = describe(target)?;
                    let upper_note = match input.source.upper { Some(upper) => describe(upper)?, None => "none".into() };
                    errors[0].notes.push(format!("call {:?} prototype {:?} original argument {} source node {:?}: sigma {}; target {}; actual input {}; upper {}",
                        call, prototype, input.ordinal, input.source.node, source_note, target_note, input.source.input, upper_note));
                    return Err(errors);
                }
            }
        }
        Ok(())
    }
    // Two loans of the same actual stack cell have distinct occurrences.
    // The cell nevertheless covers an inner call region already bounded by
    // that very same direct local storage in the same activation.
    fn local_storage_covers_call(&self, frame: &Frame, source: &Source, target: Term, b: &mut Budget) -> CompileResult<bool> {
        let Some(local) = source.direct_local else { return Ok(false) };
        let Kind::Life(Life::Implicit(owner, ordinal)) = self.state.arena[self.root(target, b)?] else { return Ok(false) };
        if owner != frame.definition { return Ok(false); }
        b.lookup(self.state.call_regions.len(), WORK)?;
        let Some(region) = self.state.call_regions.get(&(owner, ordinal)) else { return Ok(false) };
        b.spend(region.sources.len(), WORK)?;
        Ok(region.sources.iter().any(|incoming| incoming.direct_local == Some(local)))
    }
    pub(super) fn call_source_allows(
        &mut self,
        frame: &Frame,
        source: Term,
        target: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.call_source_allows_inner(frame, source, target, &mut BTreeSet::new(), b, depth)
    }
    fn call_source_allows_inner(
        &mut self, frame: &Frame, source: Term, target: Term,
        active: &mut BTreeSet<(SemanticDefinitionId, u32)>, b: &mut Budget, depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        if self.same_original(source, target, b, depth + 1)? {
            return Ok(true);
        }
        if let Kind::Life(Life::Implicit(owner, ordinal)) =
            self.state.arena[self.root(target, b)?]
        {
            b.lookup(self.state.call_regions.len(), WORK)?;
            if let Some(region) = self.state.call_regions.get(&(owner, ordinal)) {
                b.spend(region.sources.len(), WORK)?;
                let sources = region.sources.clone();
                for incoming in sources {
                    if let Some(life) = incoming.life {
                        if self.same_original(source, life, b, depth + 1)? {
                            return Ok(true);
                        }
                        if let Some(upper) = incoming.upper {
                            if self.same_original(source, upper, b, depth + 1)? {
                                return Ok(true);
                            }
                        }
                    }
                }
            }
        }
        if let Kind::Life(Life::Implicit(owner, ordinal)) =
            self.state.arena[self.root(target, b)?]
            && let Some(parents) = self.reference_region_sources(frame, owner, ordinal, b, depth + 1)? {
            b.lookup(active.len(), WORK)?;
            if !active.insert((owner, ordinal)) { return Ok(false); }
            let mut allowed = false;
            for parent in parents {
                // The target label is below every real RHS. A proven path
                // source >= one such parent therefore proves source >= label.
                // Evaluate every dependency; none creates a global LifeEqual.
                allowed |= self.call_source_allows_inner(frame, source, parent, active, b, depth + 1)?;
            }
            active.remove(&(owner, ordinal));
            if allowed { return Ok(true); }
        }
        self.view_upper_allows(frame, source, target, &mut BTreeSet::new(), b, depth + 1)
    }
}

impl Checked {
    pub(in crate::semantic) fn prepared_shared_call_input(
        &self, owner: SemanticDefinitionId, site: SemanticSiteId,
        inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<Option<LocalAnchor>> {
        if self.prepared_address_inputs.is_empty() { return Ok(None); }
        let index = inventory.original_index(owner, b)?;
        let Some(SemanticOriginalNode::Expression(call)) = index.site_node(site, b)?
        else { return Ok(None) };
        let CoreExprKind::Call { arguments, .. } = &index.get_expr(call, b)?.kind
        else { return Ok(None) };
        let [argument] = arguments.as_slice() else { return Ok(None) };
        b.lookup(self.prepared_address_inputs.len(), WORK)?;
        Ok(self.prepared_address_inputs.get(&(owner, argument.id)).copied())
    }

    /// A Name used by this call still denotes its independently resolved
    /// original local/capture, even if no extra TypeUse was emitted for it.
    pub(in crate::semantic) fn call_input_anchor(
        &self, owner: SemanticDefinitionId, node: NodeId, b: &mut Budget,
    ) -> CompileResult<Option<LocalAnchor>> {
        b.lookup(self.local_reads.len(), WORK)?;
        if let Some(anchor) = self.local_reads.get(&(owner, node)) { return Ok(Some(*anchor)); }
        b.lookup(self.environment_reads.len(), WORK)?;
        Ok(self.environment_reads.get(&(owner, node)).copied().map(LocalAnchor::Environment))
    }
    /// Read only this independently checked region's original incoming rows.
    /// A caller still binds the exact original argument node to actual SSA.
    pub(in crate::semantic) fn call_region_nodes(
        &self, context: &SemanticInstance, site: SemanticSiteId, ordinal: u32,
        selected: &crate::semantic::value::Class, source: &crate::semantic::value::Class,
        inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<Vec<NodeId>> {
        let index = inventory.original_index(context.definition, b)?;
        let Some(SemanticOriginalNode::Expression(call)) = index.site_node(site, b)? else { return Ok(Vec::new()); };
        if !matches!(index.recipe(site, b)?.kind, SemanticSiteRecipeKind::OriginalCall
            | SemanticSiteRecipeKind::Call { .. } | SemanticSiteRecipeKind::CallableCall { .. }
            | SemanticSiteRecipeKind::Construct { .. }) { return Ok(Vec::new()); }
        b.lookup(self.call_uses.len(), WORK)?;
        let Some(uses) = self.call_uses.get(&(context.definition, call)) else { return Ok(Vec::new()); };
        let mut nodes = Vec::new();
        for usage in uses {
            b.spend(1, WORK)?;
            if usage.ordinal != ordinal || self.life_value(usage.target, context, inventory, b)? != *selected { continue; }
            let incoming = &usage.source;
            if let Some(life) = incoming.life {
                if self.life_value(life, context, inventory, b)? == *source {
                    let index = inventory.original_index(context.definition, b)?;
                    let mut node = incoming.node;
                    for depth in 0..=b.partial_depth {
                        Solver::push(&mut nodes, node, b)?;
                        match &index.get_expr(node, b)?.kind {
                            CoreExprKind::Group(inner) if depth < b.partial_depth => {
                                node = inner.id
                            }
                            CoreExprKind::Group(_) => {
                                return Err(resource(
                                    "grouped call input occurrence exceeds its complete depth",
                                ));
                            }
                            _ => break,
                        }
                    }
                }
            }
        }
        Ok(nodes)
    }
}
