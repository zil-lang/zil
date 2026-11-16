//! Original call-local outer regions. Source supplies occurrence coordinates;
//! this kernel independently reconstructs all incoming sources and directed
//! bounds. Neither a concrete class image nor a loan equates two lifetimes.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Role {
    Life(u32),
    Implicit(u32),
    Head(u32),
    Invocation(u32),
    Bound(u32, u32),
}
impl Role {
    fn suffix(self) -> ([u32; 4], usize) {
        match self {
            Self::Life(n) => ([u32::MAX, 0, n, 0], 3),
            Self::Implicit(n) => ([u32::MAX, 1, n, 0], 3),
            Self::Head(n) => ([u32::MAX, 2, n, 0], 3),
            Self::Invocation(n) => ([u32::MAX, 3, n, 0], 3),
            Self::Bound(d, n) => ([u32::MAX, 4, d, n], 4),
        }
    }
}
#[derive(Clone)]
pub(super) struct Source {
    pub node: NodeId,
    pub life: Option<Term>, // None is an independently checked Static source.
    pub input: bool,
    pub anchor: Option<LocalAnchor>,
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
    fn call_occurrence(
        &mut self,
        frame: &Frame,
        call: NodeId,
        role: Role,
        b: &mut Budget,
    ) -> CompileResult<Term> {
        let definition = self.inventory.definition(frame.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &definition.body
        else {
            return Err(fail("call region owner has no complete original function"));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.node != SemanticOriginalNode::Expression(call) {
                continue;
            }
            for site in sites {
                b.spend(1, WORK)?;
                if site.id == link.site && matches!(site.kind, SemanticSiteRecipeKind::OriginalCall)
                {
                    if selected.replace(site).is_some() {
                        return Err(fail("call region has competing original Call sites"));
                    }
                }
            }
        }
        let site =
            selected.ok_or_else(|| fail("call region has no real original Call occurrence"))?;
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
        let mut ordinal = None;
        for row in &definition.implicit_lifetimes {
            b.spend(row.occurrence_path.len().saturating_add(size + 1), WORK)?;
            if row.occurrence_path.len() == size
                && row.occurrence_path.starts_with(&site.occurrence_path)
                && row.occurrence_path[site.occurrence_path.len()..] == suffix[..length]
                && ordinal.replace(row.ordinal).is_some()
            {
                return Err(fail(
                    "call region repeats its full original role occurrence",
                ));
            }
        }
        let ordinal =
            ordinal.ok_or_else(|| fail("call region lacks its complete Source role occurrence"))?;
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
    fn original_input_source(
        &self,
        frame: &Frame,
        source: &Source,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let Some(life) = source.life else {
            return Ok(false);
        };
        let Some(LocalAnchor::Parameter(ordinal)) = source.anchor else {
            return Ok(false);
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
        source.input = self.original_input_source(frame, &source, b)?;
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
        Ok(Some(Input { source, ordinal, life, role }))
    }
    pub(super) fn settle_call_inputs(
        &mut self,
        frame: &mut Frame,
        call: NodeId,
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
                    common &= match (
                        input.source.life,
                        incoming.source.life,
                    ) {
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
            let uses = self.state.call_uses.entry((frame.definition, call)).or_default();
            Self::push(uses, Use { source: input.source.clone(), target, ordinal: input.ordinal }, b)?;
            self.unify(input.life, target, depth + 1, b)?;
            if let Some(source) = input.source.life {
                if !self.call_source_allows(frame, source, target, b, depth + 1)? {
                    return Err(fail(
                        "original call outer exceeds one of its complete incoming source bounds",
                    ));
                }
            }
        }
        Ok(())
    }
    pub(super) fn call_source_allows(
        &mut self,
        frame: &Frame,
        source: Term,
        target: Term,
        b: &mut Budget,
        depth: usize,
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
        self.view_upper_allows(frame, source, target, &mut BTreeSet::new(), b, depth + 1)
    }
    pub(super) fn intrinsic_call_input(
        &mut self,
        frame: &mut Frame,
        call: NodeId,
        expression: &SemanticOriginalExpr,
        actual: Term,
        role: Role,
        wanted: Option<Term>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let life = self.call_variable(role, b)?;
        if let Some(wanted) = wanted {
            self.unify(life, wanted, depth + 1, b)?;
        }
        let expected = match self.state.arena[self.type_root(actual, b)?] {
            Kind::Address { mutable, inner, .. } => self.term(
                Kind::Address {
                    mutable,
                    life,
                    inner,
                },
                b,
            )?,
            Kind::Desc { inner, .. } => self.term(Kind::Desc { life, inner }, b)?,
            _ => return Ok(actual),
        };
        b.lookup(self.call_parameters.len(), WORK)?;
        self.call_parameters.insert(expected);
        let input = self
            .collect_call_input(frame, expression, actual, expected, 0, b, depth + 1)?
            .ok_or_else(|| fail("sealed borrowed input has no complete original outer role"))?;
        self.settle_call_inputs(frame, call, &[input], b, depth + 1)?;
        Ok(expected)
    }
}

impl Checked {
    /// Read only this independently checked region's original incoming rows.
    /// A caller still binds the exact original argument node to actual SSA.
    pub(in crate::semantic) fn call_region_nodes(
        &self, context: &SemanticInstance, site: SemanticSiteId, ordinal: u32,
        selected: &crate::semantic::value::Class, source: &crate::semantic::value::Class,
        inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<Vec<NodeId>> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, sites, .. } = &definition.body else {
            return Err(fail("call adapter has no complete original function"));
        };
        let mut call = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site { continue; }
            let SemanticOriginalNode::Expression(node) = link.node else { continue; };
            for recipe in sites {
                b.spend(1, WORK)?;
                if recipe.id == site && matches!(recipe.kind,
                    SemanticSiteRecipeKind::OriginalCall | SemanticSiteRecipeKind::Call { .. } | SemanticSiteRecipeKind::CallableCall { .. }) {
                    if call.replace(node).is_some() { return Err(fail("call adapter has competing original call nodes")); }
                }
            }
        }
        let Some(call) = call else { return Ok(Vec::new()); };
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
