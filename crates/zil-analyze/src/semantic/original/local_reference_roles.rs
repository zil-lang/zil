//! A mutable shared-reference binding has one original region role. Each
//! write contributes its complete source separately; no RHS life is equated
//! with this role or with another write's life.
use super::*;

pub(super) struct Role {
    pub(super) ordinal: u32,
    value: Term,
    writes: Vec<Write>,
}
struct Write {
    statement: NodeId,
    expression: NodeId,
    value: Term,
}
pub(in crate::semantic) struct ActualRole {
    pub statement: NodeId,
    pub ordinal: u32,
    pub value: crate::semantic::value::Value,
    pub writes: Vec<ActualWrite>,
}
pub(in crate::semantic) struct ActualWrite {
    pub statement: NodeId,
    pub expression: NodeId,
    pub anchor: Option<LocalAnchor>,
    pub static_literal: bool,
    pub value: crate::semantic::value::Value,
}
impl Solver<'_, '_> {
    pub(super) fn reference_result_role(
        &self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        b.lookup(self.state.reference_roles.len(), WORK)?;
        let mut found = false;
        for (_, role) in self
            .state
            .reference_roles
            .range((owner, NodeId(0))..=(owner, NodeId(u32::MAX)))
        {
            b.spend(1, WORK)?;
            if role.ordinal == ordinal {
                if found {
                    return Err(fail(
                        "original returned region repeats its exact declaration identity",
                    ));
                }
                found = true;
            }
        }
        Ok(found)
    }
    /// Seal the value moved at a lexical creator without changing the local
    /// binding's own role. Every checked source must be the same input life.
    pub(super) fn moved_reference_capture(
        &mut self,
        frame: &Frame,
        local: &Local,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let Some(LocalAnchor::Binding(statement)) = local.anchor else {
            return Ok(local.ty);
        };
        b.lookup(self.state.reference_roles.len(), WORK)?;
        let Some(role) = self.state.reference_roles.get(&(frame.definition, statement)) else {
            return Ok(local.ty);
        };
        if !self.same_original(local.ty, role.value, b, depth + 1)? {
            return Err(fail("moved capture changes its original lexical reference role"));
        }
        let Kind::Address { mutable: false, inner, .. } =
            self.state.arena[self.type_root(role.value, b)?] else {
                return Err(fail("moved reference capture changes its original pointer mode"));
            };
        let mut common = None;
        for write in &role.writes {
            b.spend(1, WORK)?;
            let Some(LocalAnchor::Parameter(parameter)) = self.state.reference_source_anchor(
                frame.definition, write.expression, self.inventory, b, depth + 1,
            )? else {
                return Ok(local.ty);
            };
            let declared = *frame.parameters.get(parameter as usize)
                .ok_or_else(|| fail("moved capture source selects an absent original input"))?;
            if !self.same_original(write.value, declared, b, depth + 1)? {
                return Err(fail("moved capture write disagrees with its exact original input"));
            }
            let Kind::Address { mutable: false, life, inner: source_inner } =
                self.state.arena[self.type_root(declared, b)?] else {
                    return Err(fail("moved reference capture source changes its pointer mode"));
                };
            if !self.same_original(inner, source_inner, b, depth + 1)? {
                return Err(fail("moved reference capture changes its complete pointee"));
            }
            if let Some(previous) = common {
                if !self.same_original(previous, life, b, depth + 1)? {
                    return Ok(local.ty);
                }
            } else {
                common = Some(life);
            }
        }
        let life = common.ok_or_else(|| fail("moved reference capture has no original initializer"))?;
        self.term(Kind::Address { mutable: false, life, inner }, b)
    }

    fn reference_binding_occurrence(
        &self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        b: &mut Budget,
    ) -> CompileResult<u32> {
        let index = self.inventory.original_index(owner, b)?;
        let site_id = index.declaration_site(statement, b)?;
        if index.site_node(site_id, b)? != Some(SemanticOriginalNode::Statement(statement)) {
            return Err(fail("reference role does not select its exact original declaration"));
        }
        let recipe = index.recipe(site_id, b)?;
        if !matches!(recipe.kind, SemanticSiteRecipeKind::DeclareLocal { .. }) {
            return Err(fail("reference role has another original declaration operation"));
        }
        let ordinal = self.inventory.implicit_ordinal(owner, &recipe.occurrence_path, b)?;
        ordinal.ok_or_else(|| {
            unknown("reference binding has no original declaration lifetime occurrence")
        })
    }
    pub(super) fn reference_binding(
        &mut self,
        frame: &Frame,
        statement: NodeId,
        initializer: NodeId,
        actual: Term,
        mutable: bool,
        annotated: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        if !mutable || annotated {
            return Ok(None);
        }
        let Kind::Address {
            mutable: false,
            inner,
            ..
        } = self.state.arena[self.type_root(actual, b)?]
        else {
            return Ok(None);
        };
        let ordinal = self.reference_binding_occurrence(frame.definition, statement, b)?;
        let life = self.term(Kind::Life(Life::Implicit(frame.definition, ordinal)), b)?;
        let value = self.term(
            Kind::Address {
                mutable: false,
                life,
                inner,
            },
            b,
        )?;
        let mut writes = Vec::new();
        Self::push(
            &mut writes,
            Write {
                statement,
                expression: initializer,
                value: actual,
            },
            b,
        )?;
        b.lookup(self.state.reference_roles.len(), WORK)?;
        if self
            .state
            .reference_roles
            .insert(
                (frame.definition, statement),
                Role {
                    ordinal,
                    value,
                    writes,
                },
            )
            .is_some()
        {
            return Err(fail("original reference binding role is declared twice"));
        }
        Ok(Some(value))
    }
    /// A mutable binding's region is below each independently checked RHS.
    /// These are lower-bound dependencies, not equations with the label and
    /// not an assertion that the current physical cell still has that RHS.
    pub(super) fn reference_region_sources(
        &self, frame: &Frame, owner: SemanticDefinitionId, ordinal: u32,
        b: &mut Budget, depth: usize,
    ) -> CompileResult<Option<Vec<Term>>> {
        self.step(depth, b)?;
        let mut current = Some(frame.definition);
        let mut in_scope = false;
        for _ in depth..=b.partial_depth {
            let Some(scope) = current else { break; };
            if scope == owner { in_scope = true; break; }
            current = self.inventory.definition(scope, b)?.parent;
        }
        if !in_scope { return Ok(None); }
        let mut selected = None;
        b.lookup(self.state.reference_roles.len(), WORK)?;
        for ((scope, _), role) in self.state.reference_roles
            .range((owner, NodeId(0))..=(owner, NodeId(u32::MAX))) {
            b.spend(1, WORK)?;
            if role.ordinal != ordinal { continue; }
            if selected.is_some() { return Err(fail("reference region has competing original declarations")); }
            let Kind::Address { mutable: false, life, inner } =
                self.state.arena[self.type_root(role.value, b)?] else {
                return Err(fail("reference region changes its complete shared value constructor"));
            };
            if !matches!(self.state.arena[self.root(life, b)?], Kind::Life(Life::Implicit(at, index)) if at == owner && index == ordinal) {
                return Err(fail("reference region leaves its exact original binding occurrence"));
            }
            if role.writes.is_empty() { return Err(fail("reference region has no genuine original initializer")); }
            let mut sources = Vec::new();
            for write in &role.writes {
                b.spend(1, WORK)?;
                if self.reference_static_source(*scope, write.expression, b, depth + 1)? { continue; }
                let Kind::Address { mutable: false, life: source, inner: child } =
                    self.state.arena[self.type_root(write.value, b)?] else {
                    return Err(fail("reference region source changes its full original constructor"));
                };
                if !self.same_original(inner, child, b, depth + 1)? {
                    return Err(fail("reference region source changes its invariant whole pointee domain"));
                }
                // A self-copy supplies no new bound. In particular it cannot
                // bootstrap an otherwise unsupported or cyclic source.
                if !self.same_original(source, life, b, depth + 1)? {
                    Self::push(&mut sources, source, b)?;
                }
            }
            selected = Some(sources);
        }
        Ok(selected)
    }
    pub(super) fn reference_assignment_target(
        &self,
        frame: &Frame,
        local: usize,
        b: &mut Budget,
    ) -> CompileResult<Option<NodeId>> {
        let Some(LocalAnchor::Binding(statement)) = frame.locals[local].anchor else {
            return Ok(None);
        };
        b.lookup(self.state.reference_roles.len(), WORK)?;
        Ok(self
            .state
            .reference_roles
            .contains_key(&(frame.definition, statement))
            .then_some(statement))
    }
    pub(super) fn reference_assignment(
        &mut self,
        frame: &Frame,
        binding: NodeId,
        statement: NodeId,
        expression: NodeId,
        actual: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        b.lookup(self.state.reference_roles.len(), WORK)?;
        let value = self
            .state
            .reference_roles
            .get(&(frame.definition, binding))
            .ok_or_else(|| fail("reference assignment lost its exact lexical role"))?
            .value;
        let expected = self.type_root(value, b)?;
        let actual_root = self.type_root(actual, b)?;
        let (
            Kind::Address {
                mutable: false,
                inner: expected,
                ..
            },
            Kind::Address {
                mutable: false,
                inner: actual_inner,
                ..
            },
        ) = (&self.state.arena[expected], &self.state.arena[actual_root])
        else {
            return Err(fail(
                "reference assignment changes its shared-address category",
            ));
        };
        // The entire pointee domain, including phantom classes, is invariant.
        self.unify(*expected, *actual_inner, depth + 1, b)?;
        let role = self
            .state
            .reference_roles
            .get_mut(&(frame.definition, binding))
            .ok_or_else(|| fail("reference assignment role disappeared"))?;
        b.spend(role.writes.len(), WORK)?;
        if role.writes.iter().any(|write| write.statement == statement) {
            return Err(fail(
                "reference role repeats an original assignment identity",
            ));
        }
        Self::push(
            &mut role.writes,
            Write {
                statement,
                expression,
                value: actual,
            },
            b,
        )
    }
}
impl Solver<'_, '_> {
    fn reference_static_source(
        &self,
        owner: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        b.lookup(self.state.string_literals.len(), WORK)?;
        if self.state.string_literals.contains_key(&(owner, node)) {
            return Ok(true);
        }
        let index = self.inventory.original_index(owner, b)?;
        match &index.get_expr(node, b)?.kind {
            CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                self.reference_static_source(owner, inner.id, b, depth + 1)
            }
            _ => Ok(false),
        }
    }
    // A declaration label does not extend the lifetime of any of its writes.
    // Prove escaping labels from all original RHSs, including unused bodies.
    fn reference_escape_life(
        &self,
        owner: SemanticDefinitionId,
        life: Term,
        active: &mut BTreeSet<(SemanticDefinitionId, u32)>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let Kind::Life(life) = self.state.arena[self.root(life, b)?] else {
            return Err(unknown(
                "escaping reference source has no resolved original lifetime",
            ));
        };
        b.lookup(self.state.frames.len(), WORK)?;
        let frame = self
            .state
            .frames
            .get(&owner)
            .ok_or_else(|| fail("escaping reference source has no original owner frame"))?;
        for parameter in &frame.parameters {
            b.spend(1, WORK)?;
            if let Kind::Address { life: input, .. } | Kind::Desc { life: input, .. } =
                self.state.arena[self.type_root(*parameter, b)?]
                && matches!(self.state.arena[self.root(input, b)?], Kind::Life(input) if input == life)
            {
                return Ok(());
            }
        }
        let Life::Implicit(scope, ordinal) = life else {
            return Err(unknown(
                "escaping reference source needs its exact original input occurrence",
            ));
        };
        b.lookup(active.len(), WORK)?;
        if !active.insert((scope, ordinal)) {
            return Err(fail(
                "escaping reference roles have a cyclic source equation",
            ));
        }
        b.lookup(self.state.call_regions.len(), WORK)?;
        if let Some(region) = self.state.call_regions.get(&(scope, ordinal)) {
            if region.sources.is_empty() { return Err(fail("escaping call region has no original sources")); }
            for source in &region.sources {
                b.spend(1, WORK)?;
                if let Some(life) = source.life {
                    self.reference_escape_life(scope, life, active, b, depth + 1)?;
                }
            }
            active.remove(&(scope, ordinal));
            return Ok(());
        }
        let mut found = false;
        b.lookup(self.state.reference_roles.len(), WORK)?;
        for ((role_owner, _), role) in self
            .state
            .reference_roles
            .range((scope, NodeId(0))..=(scope, NodeId(u32::MAX)))
        {
            b.spend(1, WORK)?;
            if role.ordinal != ordinal {
                continue;
            }
            if found {
                return Err(fail(
                    "original reference region labels multiple declarations",
                ));
            }
            found = true;
            if role.writes.is_empty() {
                return Err(fail("escaping reference role has no original input source"));
            }
            for write in &role.writes {
                if self.reference_static_source(*role_owner, write.expression, b, depth + 1)? {
                    continue;
                }
                let Kind::Address {
                    mutable: false,
                    life: source,
                    ..
                } = self.state.arena[self.type_root(write.value, b)?]
                else {
                    return Err(fail(
                        "escaping reference write changes its original category",
                    ));
                };
                // A self-copy adds no new source. The real initializer is a
                // separate row and cannot refer to this unbound declaration.
                if matches!(self.state.arena[self.root(source, b)?], Kind::Life(source) if source == life)
                {
                    continue;
                }
                self.reference_escape_life(*role_owner, source, active, b, depth + 1)?;
            }
        }
        if !found {
            b.lookup(self.state.view_uppers.len(), WORK)?;
            let uppers = self
                .state
                .view_uppers
                .get(&(scope, ordinal))
                .filter(|uppers| !uppers.is_empty())
                .ok_or_else(|| {
                    fail("escaping reference role retains a local or unproved source")
                })?;
            // These are directed bounds from real borrow/description places;
            // all alternatives must independently reach an external input.
            for upper in uppers {
                self.reference_escape_life(owner, *upper, active, b, depth + 1)?;
            }
        }
        active.remove(&(scope, ordinal));
        Ok(())
    }
    pub(super) fn reference_role_escapes(&self, b: &mut Budget) -> CompileResult<()> {
        let mut owners = BTreeSet::new();
        for (owner, _) in self.state.reference_roles.keys() {
            b.lookup(owners.len(), WORK)?;
            owners.insert(*owner);
        }
        for (owner, _) in self.state.call_regions.keys() {
            b.lookup(owners.len(), WORK)?;
            owners.insert(*owner);
        }
        for owner in owners {
            b.lookup(self.state.frames.len(), WORK)?;
            let frame = self
                .state
                .frames
                .get(&owner)
                .ok_or_else(|| fail("reference role owner has no complete original body"))?;
            let mut pending = Vec::new();
            Self::push(&mut pending, (frame.result, 0usize), b)?;
            let mut seen = BTreeSet::new();
            while let Some((term, depth)) = pending.pop() {
                self.step(depth, b)?;
                let term = self.root(term, b)?;
                b.lookup(seen.len(), WORK)?;
                if !seen.insert(term) {
                    continue;
                }
                let mut child = |value| Self::push(&mut pending, (value, depth + 1), b);
                match &self.state.arena[term] {
                    Kind::Life(Life::Implicit(scope, ordinal)) if *scope == owner => {
                        b.lookup(self.state.call_regions.len(), WORK)?;
                        if self.state.call_regions.contains_key(&(*scope, *ordinal)) {
                            self.reference_escape_life(owner, term, &mut BTreeSet::new(), b, 0)?;
                            continue;
                        }
                        b.lookup(self.state.reference_roles.len(), WORK)?;
                        for ((_, _), role) in self
                            .state
                            .reference_roles
                            .range((owner, NodeId(0))..=(owner, NodeId(u32::MAX)))
                        {
                            b.spend(1, WORK)?;
                            if role.ordinal == *ordinal {
                                self.reference_escape_life(
                                    owner,
                                    term,
                                    &mut BTreeSet::new(),
                                    b,
                                    0,
                                )?;
                                break;
                            }
                        }
                    }
                    Kind::Address { life, inner, .. } | Kind::Desc { life, inner } => {
                        child(*life)?;
                        child(*inner)?;
                    }
                    Kind::MutexGuard { payload, life } => {
                        child(*payload)?;
                        child(*life)?;
                    }
                    Kind::Slice(value)
                    | Kind::Wrapped(_, value)
                    | Kind::CallResult { hidden: value, .. }
                    | Kind::ConstRigid { carrier: value, .. } => child(*value)?,
                    Kind::Array { element, length } => {
                        child(*element)?;
                        child(*length)?;
                    }
                    Kind::ErrorUnion { error, result } => {
                        child(*error)?;
                        child(*result)?;
                    }
                    Kind::Tuple(values) => {
                        for value in values {
                            child(*value)?;
                        }
                    }
                    Kind::Nominal {
                        arguments, fields, ..
                    } => {
                        for value in arguments.iter().chain(fields) {
                            child(*value)?;
                        }
                    }
                    Kind::Associated {
                        receiver,
                        arguments,
                        ..
                    } => {
                        child(*receiver)?;
                        for value in arguments {
                            child(*value)?;
                        }
                    }
                    Kind::Closure {
                        parameters,
                        result,
                        environment,
                        ..
                    } => {
                        for value in parameters
                            .iter()
                            .chain(environment)
                            .chain(std::iter::once(result))
                        {
                            child(*value)?;
                        }
                    }
                    Kind::Code {
                        parameters, result, ..
                    } => {
                        for value in parameters.iter().chain(std::iter::once(result)) {
                            child(*value)?;
                        }
                    }
                    Kind::Task(task) => {
                        for value in task.terms() {
                            child(value)?;
                        }
                    }
                    Kind::Dyn(value) => {
                        for value in value.terms() {
                            child(value)?;
                        }
                    }
                    Kind::Infer(Some(value)) | Kind::LifeVariable(Some(value)) => child(*value)?,
                    Kind::Infer(None) | Kind::LifeVariable(None) => {
                        return Err(unknown(
                            "escaping reference domain retains an unresolved original term",
                        ));
                    }
                    Kind::Atom(_)
                    | Kind::Rigid(..)
                    | Kind::SelfType(_)
                    | Kind::ConstValue(_)
                    | Kind::Life(_) => {}
                }
            }
        }
        Ok(())
    }
}
impl Checked {
    pub(in crate::semantic) fn reference_bindings(
        &self,
        owner: SemanticDefinitionId,
        b: &mut Budget,
    ) -> CompileResult<Vec<NodeId>> {
        b.lookup(self.reference_roles.len(), WORK)?;
        let mut result = Vec::new();
        for ((_, statement), _) in self
            .reference_roles
            .range((owner, NodeId(0))..=(owner, NodeId(u32::MAX)))
        {
            Solver::push(&mut result, *statement, b)?;
        }
        Ok(result)
    }
    fn reference_source_anchor(
        &self,
        owner: SemanticDefinitionId,
        node: NodeId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<LocalAnchor>> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource("reference source syntax exceeds its finite depth"));
        }
        b.lookup(self.local_reads.len(), WORK)?;
        if let Some(anchor) = self.local_reads.get(&(owner, node)) {
            return Ok(Some(*anchor));
        }
        b.lookup(self.environment_reads.len(), WORK)?;
        if let Some(ordinal) = self.environment_reads.get(&(owner, node)) {
            return Ok(Some(LocalAnchor::Environment(*ordinal)));
        }
        let index = inventory.original_index(owner, b)?;
        match &index.get_expr(node, b)?.kind {
            CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                self.reference_source_anchor(owner, inner.id, inventory, b, depth + 1)
            }
            _ => Ok(None),
        }
    }
    pub(in crate::semantic) fn immutable_reference_alias(
        &self,
        owner: SemanticDefinitionId,
        binding: NodeId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<(NodeId, LocalAnchor)>> {
        let index = inventory.original_index(owner, b)?;
        let CoreStatementKind::Binding { mutable: false, initializer: Some(initializer), .. } =
            &index.get_stmt(binding, b)?.kind else { return Ok(None); };
        let anchor = self.reference_source_anchor(owner, initializer.id, inventory, b, 0)?;
        Ok(anchor.map(|anchor| (initializer.id, anchor)))
    }
    fn reference_static_literal(
        &self,
        owner: SemanticDefinitionId,
        node: NodeId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        b.spend(1, WORK)?;
        if depth > b.partial_depth {
            return Err(resource(
                "reference static source syntax exceeds finite depth",
            ));
        }
        b.lookup(self.string_literals.len(), WORK)?;
        if self.string_literals.contains_key(&(owner, node)) {
            return Ok(true);
        }
        let index = inventory.original_index(owner, b)?;
        match &index.get_expr(node, b)?.kind {
            CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } => {
                self.reference_static_literal(owner, inner.id, inventory, b, depth + 1)
            }
            _ => Ok(false),
        }
    }
    pub(in crate::semantic) fn reference_role(
        &self,
        context: &SemanticInstance,
        statement: NodeId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<ActualRole>> {
        b.lookup(self.reference_roles.len(), WORK)?;
        let Some(role) = self.reference_roles.get(&(context.definition, statement)) else {
            return Ok(None);
        };
        let mut writes = Vec::new();
        for write in &role.writes {
            let anchor = self.reference_source_anchor(
                context.definition,
                write.expression,
                inventory,
                b,
                0,
            )?;
            Solver::push(
                &mut writes,
                ActualWrite {
                    statement: write.statement,
                    expression: write.expression,
                    anchor,
                    static_literal: self.reference_static_literal(
                        context.definition,
                        write.expression,
                        inventory,
                        b,
                        0,
                    )?,
                    value: self.value(write.value, context, program, inventory, b, 0)?,
                },
                b,
            )?;
        }
        Ok(Some(ActualRole {
            statement,
            ordinal: role.ordinal,
            value: self.value(role.value, context, program, inventory, b, 0)?,
            writes,
        }))
    }
}
