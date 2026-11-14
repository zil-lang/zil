//! Conditional protocol relations are proved from original operations. No
//! runtime layout, instance specialization, or proposed Typed table is input.
use super::*;

const OPEN: u8 = 1;
const CLOSING: u8 = 2;
const CLOSED: u8 = 4;
const UNKNOWN: u8 = OPEN | CLOSING | CLOSED;

fn bit(s: ProtocolState) -> u8 {
    match s {
        ProtocolState::Open => OPEN,
        ProtocolState::Closing => CLOSING,
        ProtocolState::Closed => CLOSED,
    }
}
fn mask(states: &[ProtocolState]) -> u8 {
    states.iter().fold(0, |m, s| m | bit(*s))
}
fn failure(message: &str) -> Diagnostic {
    error("E-SEMANTIC-PROTOCOL", Span::default(), message)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Root {
    Parameter(SemanticDefinitionId, u32),
    Created(SemanticDefinitionId, NodeId),
    Capture(SemanticDefinitionId, u32),
}
#[derive(Clone, PartialEq, Eq)]
struct Outcome {
    exit: ProtocolExit,
    phases: BTreeMap<Root, u8>,
}
#[derive(Clone, PartialEq, Eq)]
enum Value {
    Plain,
    Bool(bool),
    Error {
        is_error: bool,
        payload: Box<Value>,
    },
    Boxed(Box<Value>),
    // A generic affine leaf carries its independently checked original type.
    // It is neither an initialized protocol root nor a capability receipt.
    Cleanup {
        owner: SemanticDefinitionId,
        ty: SemanticTypeRecipe,
    },
    Choice(Vec<Value>),
    Variant {
        name: String,
        fields: Vec<Value>,
    },
    Closure {
        definition: SemanticDefinitionId,
        captures: BTreeMap<String, Value>,
    },
    // Root identity, current occurrence validity, actual owning carrier.
    // Address/Desc observations retain identity but never cleanup ownership.
    Root(Root, bool, bool, Option<u8>),
    Valid {
        root: Root,
        fresh: bool,
        negated: bool,
    },
    Poll {
        fresh: bool,
        outcomes: Vec<Outcome>,
    },
    Tuple(Vec<Value>),
    Fields(BTreeMap<String, Value>),
}
impl Value {
    fn work(&self) -> usize {
        match self {
            Self::Error { payload, .. } | Self::Boxed(payload) => payload.work().saturating_add(1),
            Self::Poll { outcomes, .. } => outcomes
                .iter()
                .fold(1, |w, o| w.saturating_add(o.phases.len()).saturating_add(1)),
            Self::Tuple(v) | Self::Choice(v) => v.iter().fold(1, |w, v| w.saturating_add(v.work())),
            Self::Variant { name, fields } => {
                fields.iter().fold(name.len().saturating_add(1), |w, v| {
                    w.saturating_add(v.work())
                })
            }
            Self::Closure { captures, .. } => captures.iter().fold(1, |w, (k, v)| {
                w.saturating_add(k.len()).saturating_add(v.work())
            }),
            Self::Fields(v) => v.iter().fold(1, |w, (k, v)| {
                w.saturating_add(k.len()).saturating_add(v.work())
            }),
            _ => 1,
        }
    }
    fn roots(&self, out: &mut BTreeSet<Root>) {
        match self {
            Self::Error { payload, .. } | Self::Boxed(payload) => payload.roots(out),
            Self::Root(root, true, _, _) => {
                out.insert(*root);
            }
            Self::Tuple(values) | Self::Choice(values) | Self::Variant { fields: values, .. } => {
                for value in values {
                    value.roots(out);
                }
            }
            Self::Closure { captures, .. } => {
                for value in captures.values() {
                    value.roots(out);
                }
            }
            Self::Fields(values) => {
                for value in values.values() {
                    value.roots(out);
                }
            }
            _ => {}
        }
    }
    fn invalidate(&mut self, root: Root, replace: bool, previous: u8) {
        match self {
            Self::Error { payload, .. } | Self::Boxed(payload) => {
                payload.invalidate(root, replace, previous)
            }
            Self::Root(id, live, owning, prior) if *id == root && replace => {
                if *live && *owning {
                    *prior = Some(previous);
                }
                *live = false;
            }
            Self::Poll { fresh, outcomes }
                if outcomes.iter().any(|o| o.phases.contains_key(&root)) =>
            {
                *fresh = false
            }
            Self::Valid {
                root: id, fresh, ..
            } if *id == root => *fresh = false,
            Self::Tuple(values) | Self::Choice(values) | Self::Variant { fields: values, .. } => {
                for value in values {
                    value.invalidate(root, replace, previous);
                }
            }
            Self::Closure { captures, .. } => {
                for value in captures.values_mut() {
                    value.invalidate(root, replace, previous);
                }
            }
            Self::Fields(values) => {
                for value in values.values_mut() {
                    value.invalidate(root, replace, previous);
                }
            }
            _ => {}
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Control {
    Next,
    Return,
    Break,
    Continue,
}
#[derive(Clone, PartialEq, Eq)]
struct State {
    reachable: bool,
    abrupt: Vec<State>,
    // Stacks preserve hidden lexical binders, including their stale Polls.
    names: BTreeMap<String, Vec<Value>>,
    roots: BTreeMap<Root, u8>,
    control: Control,
    returned: Value,
    temporaries: Vec<Value>,
}
impl State {
    fn work(&self) -> usize {
        self.names.iter().fold(
            self.abrupt.iter().fold(
                self.temporaries.iter().fold(
                    self.roots
                        .len()
                        .saturating_add(self.returned.work())
                        .saturating_add(1),
                    |w, v| w.saturating_add(v.work()),
                ),
                |w, state| w.saturating_add(state.work()),
            ),
            |w, (name, values)| {
                values
                    .iter()
                    .fold(w.saturating_add(name.len()).saturating_add(1), |w, v| {
                        w.saturating_add(v.work())
                    })
            },
        )
    }
}
struct Engine<'a, 'b> {
    program: &'a SemanticModule,
    proof: &'b mut Proof,
    limits: &'b Limits,
    left: &'b mut usize,
    active: BTreeSet<SemanticDefinitionId>,
    relevant: BTreeSet<SemanticDefinitionId>,
    categories: Vec<(SemanticDefinitionId, SemanticTypeRecipe, cleanup::Category)>,
    cleanup: Vec<(SemanticDefinitionId, SemanticTypeRecipe, bool)>,
}
impl<'a> Engine<'a, '_> {
    fn category(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<cleanup::Category> {
        self.pay(self.categories.len().saturating_add(1), depth)?;
        for (scope, candidate, result) in &self.categories {
            if *scope != owner {
                continue;
            }
            crate::dyn_metadata::semantic_operand_snapshot(candidate, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?;
            if candidate == ty {
                return Ok(*result);
            }
        }
        let result =
            cleanup::category(self.program, self.proof, owner, ty, self.limits, self.left)?;
        self.categories
            .push((owner, copy_recipe(ty, self.limits, self.left)?, result));
        Ok(result)
    }
    fn can_drop(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(self.cleanup.len().saturating_add(1), depth)?;
        for (scope, candidate, result) in &self.cleanup {
            if *scope != owner {
                continue;
            }
            crate::dyn_metadata::semantic_operand_snapshot(candidate, self.limits, self.left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?;
            if candidate == ty {
                return Ok(*result);
            }
        }
        let result =
            cleanup::capability(self.program, self.proof, owner, ty, self.limits, self.left)?;
        self.cleanup
            .push((owner, copy_recipe(ty, self.limits, self.left)?, result));
        Ok(result)
    }
    fn pay(&mut self, n: usize, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original protocol work exceeded", None)
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original protocol depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn definition(&mut self, id: SemanticDefinitionId) -> R<&'a SemanticDefinition> {
        self.pay(self.program.definitions.len(), 0)?;
        self.program
            .definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| failure("protocol target has no original definition"))
    }
    fn charge_value(&mut self, value: &Value, depth: usize) -> R<()> {
        self.pay(1, depth)?;
        match value {
            Value::Error { payload, .. } | Value::Boxed(payload) => {
                self.charge_value(payload, depth + 1)?
            }
            Value::Cleanup { ty, .. } => {
                crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?
            }
            Value::Tuple(values)
            | Value::Choice(values)
            | Value::Variant { fields: values, .. } => {
                for value in values {
                    self.charge_value(value, depth + 1)?;
                }
            }
            Value::Fields(values)
            | Value::Closure {
                captures: values, ..
            } => {
                for value in values.values() {
                    self.charge_value(value, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn charge_state(&mut self, state: &State, depth: usize) -> R<()> {
        self.pay(state.abrupt.len(), depth)?;
        for exit in &state.abrupt {
            self.charge_state(exit, depth + 1)?;
        }
        for value in state
            .names
            .values()
            .flatten()
            .chain(&state.temporaries)
            .chain(std::iter::once(&state.returned))
        {
            self.charge_value(value, depth + 1)?;
        }
        Ok(())
    }
    fn copy(&mut self, state: &State, depth: usize) -> R<State> {
        self.pay(state.work(), depth)?;
        self.charge_state(state, depth + 1)?;
        Ok(state.clone())
    }
    fn value_copy(&mut self, value: &Value, depth: usize) -> R<Value> {
        self.pay(value.work(), depth)?;
        self.charge_value(value, depth + 1)?;
        Ok(value.clone())
    }
    fn root(&mut self, value: &Value, state: &State, depth: usize) -> R<Root> {
        self.pay(value.work().saturating_add(state.roots.len()), depth)?;
        match value {
            Value::Root(root, true, _, _) if state.roots.contains_key(root) => Ok(*root),
            _ => Err(failure(
                "protocol operation lacks its live original parameter or owning occurrence",
            )),
        }
    }
    fn change(
        &mut self,
        state: &mut State,
        root: Root,
        phase: u8,
        replace: bool,
        depth: usize,
    ) -> R<()> {
        self.pay(state.work(), depth)?;
        let previous = state.roots.get(&root).copied().unwrap_or(UNKNOWN);
        for values in state.names.values_mut() {
            for value in values {
                value.invalidate(root, replace, previous);
            }
        }
        state.returned.invalidate(root, replace, previous);
        for value in &mut state.temporaries {
            value.invalidate(root, replace, previous);
        }
        state.roots.insert(root, phase);
        Ok(())
    }
    fn async_head(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<bool> {
        self.pay(1, depth)?;
        match ty {
            SemanticTypeRecipe::AsyncRegistration(_) => Ok(true),
            SemanticTypeRecipe::Address { inner, .. }
            | SemanticTypeRecipe::Desc { inner, .. }
            | SemanticTypeRecipe::Managed(inner) => self.async_head(owner, inner, depth + 1),
            SemanticTypeRecipe::Qualified { storage, .. } => {
                self.async_head(owner, storage, depth + 1)
            }
            SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source,
            } => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    source,
                    self.limits,
                    self.left,
                    depth + 1,
                )?;
                let q = Query::from(source.clone());
                self.pay(q.copy_work().saturating_add(self.proof.types.len()), depth)?;
                let Some(ty) = self.proof.types.get(&(owner, q)) else {
                    return Ok(false);
                };
                let ty = copy_recipe(ty, self.limits, self.left)?;
                self.async_head(owner, &ty, depth + 1)
            }
            // Opaque bodies remain in the defining lexical domain.
            SemanticTypeRecipe::Opaque {
                definition, hidden, ..
            } => self.async_head(*definition, hidden, depth + 1),
            _ => Ok(false),
        }
    }
    fn closure_child(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        depth: usize,
    ) -> R<&'a SemanticDefinition> {
        let original = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &original.body
        else {
            return Err(failure("original closure creator has no body directory"));
        };
        self.pay(
            site_nodes
                .len()
                .saturating_mul(sites.len().saturating_add(1)),
            depth,
        )?;
        let mut selected = None;
        for row in site_nodes {
            if row.node != SemanticOriginalNode::Expression(node) {
                continue;
            }
            for site in sites {
                if site.id != row.site {
                    continue;
                }
                if let SemanticSiteRecipeKind::Closure { definition, .. } = site.kind {
                    if selected.replace(definition).is_some() {
                        return Err(failure(
                            "original closure creator selects multiple children",
                        ));
                    }
                }
            }
        }
        let child = self.definition(
            selected
                .ok_or_else(|| failure("original closure lacks its exact creator occurrence"))?,
        )?;
        if child.parent != Some(owner) {
            return Err(failure(
                "original closure creator changes its lexical parent",
            ));
        }
        Ok(child)
    }
    fn closure_creator(
        &mut self,
        child: &SemanticDefinition,
        depth: usize,
    ) -> R<(SemanticDefinitionId, NodeId)> {
        let parent = child
            .parent
            .ok_or_else(|| failure("original capture has no creator definition"))?;
        let original = self.definition(parent)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites, site_nodes, ..
        } = &original.body
        else {
            return Err(failure("original capture creator has no body directory"));
        };
        self.pay(
            sites
                .len()
                .saturating_mul(site_nodes.len().saturating_add(1)),
            depth,
        )?;
        let mut selected = None;
        for site in sites {
            if !matches!(site.kind,SemanticSiteRecipeKind::Closure{definition,..} if definition==child.id)
            {
                continue;
            }
            for row in site_nodes.iter().filter(|row| row.site == site.id) {
                let SemanticOriginalNode::Expression(node) = row.node else {
                    return Err(failure("original closure creator is not an expression"));
                };
                if selected.replace(node).is_some() {
                    return Err(failure(
                        "original closure has ambiguous creator occurrences",
                    ));
                }
            }
        }
        Ok((
            parent,
            selected
                .ok_or_else(|| failure("original closure has no complete creator occurrence"))?,
        ))
    }
    fn capture_type(
        &mut self,
        parent: SemanticDefinitionId,
        node: NodeId,
        name: &str,
        child: &SemanticDefinition,
        depth: usize,
    ) -> R<SemanticTypeRecipe> {
        self.pay(name.len(), depth)?;
        if let Some(ty) = self.query_type(
            parent,
            Query::NameAt {
                expression: true,
                node,
                name: name.to_owned(),
            },
            depth + 1,
        )? {
            return Ok(ty);
        }
        self.pay(child.environment.len(), depth)?;
        let mut selected = None;
        for (slot, formal) in child.environment.iter().enumerate() {
            if matches!(&formal.source,SemanticActualRecipe::Type(SemanticTypeRecipe::OriginalType{source:SemanticOriginalTypeSource::NameAt{at:SemanticOriginalNode::Expression(at),name:source},scope:SemanticScopeRef::Ancestor(0)}) if *at==node&&source==name)
            {
                if selected.replace(slot as u32).is_some() {
                    return Err(failure(
                        "original physical capture has ambiguous Type sources",
                    ));
                }
            }
        }
        let slot = selected.ok_or_else(|| {
            pending("original cleanup capture lacks its exact original whole-Type source")
        })?;
        self.pay(self.proof.environments.len(), depth)?;
        let (owner, ty) = self
            .proof
            .environments
            .get(&(child.id, slot))
            .ok_or_else(|| pending("original cleanup capture Type source is not sealed"))?;
        if *owner != parent {
            return Err(failure(
                "original capture Type source belongs to another lexical creator",
            ));
        }
        copy_recipe(ty, self.limits, self.left)
    }
    fn query_type(
        &mut self,
        owner: SemanticDefinitionId,
        query: Query,
        depth: usize,
    ) -> R<Option<SemanticTypeRecipe>> {
        self.pay(
            query.copy_work().saturating_add(self.proof.types.len()),
            depth,
        )?;
        self.proof
            .types
            .get(&(owner, query))
            .map(|ty| copy_recipe(ty, self.limits, self.left))
            .transpose()
    }
    fn view(&mut self, value: Value, depth: usize) -> R<Value> {
        self.pay(value.work(), depth)?;
        Ok(match value {
            Value::Error { is_error, payload } => Value::Error {
                is_error,
                payload: Box::new(self.view(*payload, depth + 1)?),
            },
            Value::Boxed(payload) => self.view(*payload, depth + 1)?,
            Value::Cleanup { .. } => Value::Plain,
            Value::Root(root, live, _, prior) => Value::Root(root, live, false, prior),
            Value::Tuple(values) => Value::Tuple(
                values
                    .into_iter()
                    .map(|value| self.view(value, depth + 1))
                    .collect::<R<_>>()?,
            ),
            Value::Choice(values) => Value::Choice(
                values
                    .into_iter()
                    .map(|value| self.view(value, depth + 1))
                    .collect::<R<_>>()?,
            ),
            Value::Variant { name, fields } => Value::Variant {
                name,
                fields: fields
                    .into_iter()
                    .map(|value| self.view(value, depth + 1))
                    .collect::<R<_>>()?,
            },
            Value::Fields(values) => Value::Fields(
                values
                    .into_iter()
                    .map(|(name, value)| Ok((name, self.view(value, depth + 1)?)))
                    .collect::<R<_>>()?,
            ),
            Value::Closure {
                definition,
                captures,
            } => Value::Closure {
                definition,
                captures: captures
                    .into_iter()
                    .map(|(name, value)| Ok((name, self.view(value, depth + 1)?)))
                    .collect::<R<_>>()?,
            },
            other => other,
        })
    }
    fn choice(&mut self, values: Vec<Value>, depth: usize) -> R<Value> {
        self.pay(values.len().saturating_add(1), depth)?;
        let mut pending = values;
        let mut out = Vec::new();
        while let Some(value) = pending.pop() {
            self.pay(value.work().saturating_add(out.len()), depth)?;
            if let Value::Choice(values) = value {
                self.pay(values.len(), depth)?;
                pending.extend(values);
                continue;
            }
            self.charge_value(&value, depth + 1)?;
            let mut duplicate = false;
            for previous in &out {
                self.charge_value(previous, depth + 1)?;
                if previous == &value {
                    duplicate = true;
                    break;
                }
            }
            if !duplicate {
                out.push(value);
            }
        }
        out.reverse();
        Ok(if out.len() == 1 {
            out.pop().unwrap()
        } else {
            Value::Choice(out)
        })
    }
    fn typed(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        value: Value,
        depth: usize,
    ) -> R<Value> {
        self.pay(value.work(), depth)?;
        match self.category(owner, ty, depth + 1)? {
            cleanup::Category::Copy | cleanup::Category::View => {
                return self.view(value, depth + 1);
            }
            cleanup::Category::Own => {}
        }
        match value {
            Value::Plain => {
                if let Some((error, result)) = cleanup::error_arms(
                    self.program,
                    self.proof,
                    owner,
                    ty,
                    self.limits,
                    self.left,
                )? {
                    let error = self.typed(owner, &error, Value::Plain, depth + 1)?;
                    let result = self.typed(owner, &result, Value::Plain, depth + 1)?;
                    return self.choice(
                        vec![
                            Value::Error {
                                is_error: true,
                                payload: Box::new(error),
                            },
                            Value::Error {
                                is_error: false,
                                payload: Box::new(result),
                            },
                        ],
                        depth + 1,
                    );
                }
                if let Some(fields) =
                    cleanup::fields(self.program, self.proof, owner, ty, self.limits, self.left)?
                {
                    self.pay(fields.len(), depth)?;
                    let mut values = BTreeMap::new();
                    for (name, ty) in fields {
                        values.insert(name, self.typed(owner, &ty, Value::Plain, depth + 1)?);
                    }
                    return Ok(Value::Fields(values));
                }
                Ok(Value::Cleanup {
                    owner,
                    ty: copy_recipe(ty, self.limits, self.left)?,
                })
            }
            Value::Choice(values) => {
                let values = values
                    .into_iter()
                    .map(|value| self.typed(owner, ty, value, depth + 1))
                    .collect::<R<Vec<_>>>()?;
                self.choice(values, depth + 1)
            }
            Value::Fields(values) => {
                if cleanup::fields(self.program, self.proof, owner, ty, self.limits, self.left)?
                    .is_some()
                {
                    Ok(Value::Fields(values))
                } else {
                    Ok(Value::Cleanup {
                        owner,
                        ty: copy_recipe(ty, self.limits, self.left)?,
                    })
                }
            }
            other => Ok(other),
        }
    }
    fn query_async(&mut self, owner: SemanticDefinitionId, query: Query, depth: usize) -> R<bool> {
        self.pay(
            query.copy_work().saturating_add(self.proof.types.len()),
            depth,
        )?;
        let Some(ty) = self.proof.types.get(&(owner, query)) else {
            return Ok(false);
        };
        let ty = copy_recipe(ty, self.limits, self.left)?;
        self.async_head(owner, &ty, depth + 1)
    }
    fn query_ownership(
        &mut self,
        owner: SemanticDefinitionId,
        query: Query,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(
            query.copy_work().saturating_add(self.proof.types.len()),
            depth,
        )?;
        let Some(ty) = self.proof.types.get(&(owner, query)) else {
            return Ok(None);
        };
        let ty = copy_recipe(ty, self.limits, self.left)?;
        self.ownership(owner, &ty, depth + 1)
    }
    fn ownership(
        &mut self,
        owner: SemanticDefinitionId,
        ty: &SemanticTypeRecipe,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(1, depth)?;
        match ty {
            SemanticTypeRecipe::Managed(_) => Ok(Some(true)),
            SemanticTypeRecipe::Address { .. } | SemanticTypeRecipe::Desc { .. } => Ok(Some(false)),
            SemanticTypeRecipe::Qualified { storage, .. } => {
                self.ownership(owner, storage, depth + 1)
            }
            SemanticTypeRecipe::Opaque {
                definition, hidden, ..
            } => self.ownership(*definition, hidden, depth + 1),
            SemanticTypeRecipe::OriginalType {
                scope: SemanticScopeRef::Ancestor(0),
                source,
            } => {
                crate::dyn_metadata::semantic_operand_snapshot(
                    source,
                    self.limits,
                    self.left,
                    depth + 1,
                )?;
                self.query_ownership(owner, Query::from(source.clone()), depth + 1)
            }
            _ => Ok(None),
        }
    }
    fn place_path(
        &mut self,
        expression: &SemanticOriginalExpr,
        depth: usize,
    ) -> R<Option<(String, Vec<String>)>> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Name(name) => {
                self.pay(name.len().saturating_add(1), depth)?;
                Ok(Some((name.clone(), Vec::new())))
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                self.place_path(value, depth + 1)
            }
            CoreExprKind::Member { object, member, .. } => {
                let Some((name, mut path)) = self.place_path(object, depth + 1)? else {
                    return Ok(None);
                };
                self.pay(
                    member.len().saturating_add(path.len()).saturating_add(1),
                    depth,
                )?;
                path.push(member.clone());
                Ok(Some((name, path)))
            }
            _ => Ok(None),
        }
    }
    fn replace_place(
        &mut self,
        expression: &SemanticOriginalExpr,
        state: &mut State,
        value: Value,
        depth: usize,
    ) -> R<Option<Value>> {
        let Some((name, path)) = self.place_path(expression, depth + 1)? else {
            return Ok(None);
        };
        self.pay(
            name.len()
                .saturating_add(state.names.len())
                .saturating_add(path.len()),
            depth,
        )?;
        let Some(mut target) = state
            .names
            .get_mut(&name)
            .and_then(|stack| stack.last_mut())
        else {
            return Ok(None);
        };
        for field in path {
            target = match target {
                Value::Fields(fields) => match fields.get_mut(&field) {
                    Some(value) => value,
                    None => {
                        return Err(failure(
                            "original cleanup place selects an absent complete product field",
                        ));
                    }
                },
                Value::Tuple(values) => {
                    let index = field.parse::<usize>().map_err(|_| {
                        failure("original cleanup tuple place has a nonordinal member")
                    })?;
                    values
                        .get_mut(index)
                        .ok_or_else(|| failure("original cleanup tuple place is out of range"))?
                }
                _ => return Ok(None),
            };
        }
        self.pay(target.work().saturating_add(value.work()), depth)?;
        Ok(Some(std::mem::replace(target, value)))
    }
    fn move_place(
        &mut self,
        expression: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
    ) -> R<()> {
        if self
            .replace_place(expression, state, Value::Plain, depth)?
            .is_none()
        {
            return Err(pending(
                "original cleanup transfer requires its exact owned product/place boundary",
            ));
        }
        Ok(())
    }
    fn observe(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        self.pay(1, depth)?;
        match &expression.kind {
            CoreExprKind::Name(name) => {
                self.pay(name.len().saturating_add(state.names.len()), depth)?;
                if let Some(value) = state.names.get(name).and_then(|stack| stack.last()) {
                    self.value_copy(value, depth + 1)
                } else {
                    Ok(Value::Plain)
                }
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                self.observe(owner, value, state, depth + 1)
            }
            CoreExprKind::Member { object, member, .. } => {
                let value = self.observe(owner, object, state, depth + 1)?;
                match value {
                    Value::Fields(mut fields) => Ok(fields.remove(member).unwrap_or(Value::Plain)),
                    Value::Tuple(mut values) => Ok(member
                        .parse::<usize>()
                        .ok()
                        .filter(|index| *index < values.len())
                        .map(|index| values.remove(index))
                        .unwrap_or(Value::Plain)),
                    Value::Cleanup { .. } | Value::Root(..) | Value::Plain => Ok(Value::Plain),
                    _ => Err(pending(
                        "original cleanup observation requires its complete projected owner relation",
                    )),
                }
            }
            _ => self.expr(owner, expression, state, depth + 1),
        }
    }
    fn drop_value(&mut self, value: &Value, state: &State, depth: usize) -> R<()> {
        self.pay(value.work(), depth)?;
        match value {
            Value::Error { payload, .. } | Value::Boxed(payload) => {
                self.drop_value(payload, state, depth + 1)?
            }
            Value::Cleanup { owner, ty } => {
                if !self.can_drop(*owner, ty, depth + 1)? {
                    self.pay(128, depth)?;
                    let mut error = error(
                        "E-ORIGINAL-CLEANUP",
                        Span::default(),
                        "live original value has no independent SyncDrop proof",
                    );
                    crate::dyn_metadata::semantic_operand_snapshot(ty, self.limits, self.left, 1)?;
                    error.notes.push(format!(
                        "original cleanup owner {:?}, complete type recipe {:?}",
                        owner, ty
                    ));
                    return Err(error);
                }
            }
            Value::Choice(values) | Value::Variant { fields: values, .. } => {
                for value in values {
                    self.drop_value(value, state, depth + 1)?;
                }
            }
            Value::Closure { captures, .. } => {
                for value in captures.values() {
                    self.drop_value(value, state, depth + 1)?;
                }
            }
            Value::Root(root, true, true, _) => {
                if state.roots.get(root).copied() != Some(CLOSED) {
                    return Err(failure(
                        "original cleanup discards a live AsyncRegistration owner without a Closed fact for that root",
                    ));
                }
            }
            Value::Root(_, false, true, prior) if *prior != Some(CLOSED) => {
                return Err(failure(
                    "original cleanup loses an earlier registration generation's owning responsibility",
                ));
            }
            Value::Tuple(values) => {
                for value in values {
                    self.drop_value(value, state, depth + 1)?;
                }
            }
            Value::Fields(values) => {
                for value in values.values() {
                    self.drop_value(value, state, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn site(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        depth: usize,
    ) -> R<Option<SemanticSiteId>> {
        let original = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &original.body else {
            return Ok(None);
        };
        self.pay(site_nodes.len(), depth)?;
        let mut matches = site_nodes
            .iter()
            .filter(|r| r.node == SemanticOriginalNode::Expression(node))
            .filter(|r| self.proof.calls.contains_key(&(owner, r.site)));
        let first = matches.next().map(|r| r.site);
        if matches.next().is_some() {
            return Err(failure(
                "original protocol call occurrence has multiple call sites",
            ));
        }
        Ok(first)
    }
    fn output(
        &mut self,
        owner: SemanticDefinitionId,
        state: &State,
        contracts: &[FunctionProtocolContract],
        inputs: &BTreeMap<u32, ProtocolState>,
        depth: usize,
    ) -> R<()> {
        self.pay(
            contracts.iter().fold(state.work(), |w, c| {
                w.saturating_add(c.exits.len().saturating_mul(4))
                    .saturating_add(1)
            }),
            depth,
        )?;
        if contracts.is_empty() {
            return Ok(());
        }
        let outcomes = match &state.returned {
            Value::Poll {
                fresh: true,
                outcomes,
            } => {
                self.pay(state.returned.work(), depth)?;
                outcomes.clone()
            }
            Value::Poll { fresh: false, .. } => {
                return Err(failure(
                    "returned Poll was invalidated by a later operation on its original root",
                ));
            }
            _ => {
                if contracts
                    .iter()
                    .any(|c| c.exits.iter().any(|r| r.exit != ProtocolExit::Return))
                {
                    return Err(pending(
                        "original conditional Error/Poll result lacks its complete tagged source relation",
                    ));
                }
                self.pay(state.roots.len().saturating_add(1), depth)?;
                vec![Outcome {
                    exit: ProtocolExit::Return,
                    phases: state.roots.clone(),
                }]
            }
        };
        for outcome in outcomes {
            for contract in contracts {
                let input = inputs
                    .get(&contract.parameter)
                    .ok_or_else(|| failure("protocol input case is absent"))?;
                let root = Root::Parameter(owner, contract.parameter);
                let actual = outcome
                    .phases
                    .get(&root)
                    .or_else(|| state.roots.get(&root))
                    .copied()
                    .ok_or_else(|| failure("protocol exit lost its original parameter root"))?;
                self.pay(contract.exits.len(), depth)?;
                let row = contract
                    .exits
                    .iter()
                    .find(|r| r.input == *input && r.exit == outcome.exit)
                    .ok_or_else(|| {
                        failure(
                            "reachable original protocol exit is absent from the declared relation",
                        )
                    })?;
                if actual == 0 || actual & !mask(&row.output) != 0 {
                    return Err(failure(
                        "original protocol exit exceeds its declared conditional phase relation",
                    ));
                }
            }
        }
        Ok(())
    }
    fn nominal_value(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        values: &[Value],
        state: &State,
        depth: usize,
    ) -> R<Option<Value>> {
        self.pay(
            self.proof.nominal_constructors.len().saturating_add(1),
            depth,
        )?;
        let Some((id, variant)) = self.proof.nominal_constructors.get(&(owner, node)).copied()
        else {
            return Ok(None);
        };
        let original = self.definition(id)?;
        let SemanticDefinitionBody::Nominal {
            declaration,
            body: SemanticNominalBody::Enum { variants },
            ..
        } = &original.body
        else {
            return Err(failure(
                "original constructor theorem names another declaration kind",
            ));
        };
        let selected = variants
            .get(variant as usize)
            .ok_or_else(|| failure("original constructor theorem selects an absent variant"))?;
        if selected.fields.len() != values.len() {
            return Err(failure(
                "original constructor cleanup changes the complete argument arity",
            ));
        }
        if *declaration == zil_ir::builtin_life::POLL
            && values.iter().all(|value| matches!(value, Value::Plain))
        {
            let exit = match variant {
                0 => ProtocolExit::PollPending,
                1 => ProtocolExit::PollReady,
                _ => {
                    return Err(failure(
                        "canonical Poll constructor changes its original variant",
                    ));
                }
            };
            self.pay(state.roots.len().saturating_add(1), depth)?;
            return Ok(Some(Value::Poll {
                fresh: true,
                outcomes: vec![Outcome {
                    exit,
                    phases: state.roots.clone(),
                }],
            }));
        }
        self.pay(selected.name.len().saturating_add(values.len()), depth)?;
        let mut fields = Vec::with_capacity(values.len());
        for value in values {
            fields.push(self.value_copy(value, depth + 1)?)
        }
        Ok(Some(Value::Variant {
            name: selected.name.clone(),
            fields,
        }))
    }
    fn call(
        &mut self,
        owner: SemanticDefinitionId,
        expression: &SemanticOriginalExpr,
        _arguments: &[SemanticOriginalExpr],
        values: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        if let Some(value) = self.nominal_value(owner, expression.id, values, state, depth + 1)? {
            return Ok(value);
        }
        let Some(site) = self.site(owner, expression.id, depth)? else {
            return self.unknown_call(owner, expression.id, values, state, depth);
        };
        self.pay(self.proof.intrinsics.len(), depth)?;
        if let Some(call) = self.proof.intrinsics.get(&(owner, site)) {
            let work = match &call.kind {
                IntrinsicKind::DynCall {
                    parameter_modes, ..
                } => parameter_modes.len().saturating_add(1),
                IntrinsicKind::RegistrationStart { providers, .. } => {
                    providers.len().saturating_add(1)
                }
                _ => 1,
            };
            self.pay(work, depth)?;
        }
        let intrinsic = self
            .proof
            .intrinsics
            .get(&(owner, site))
            .map(|c| c.kind.clone());
        if let Some(kind) = intrinsic {
            match kind {
                IntrinsicKind::OwnedDyn => {
                    let [value] = values else {
                        return Err(failure(
                            "original owned Dyn construction changes its source arity",
                        ));
                    };
                    self.drop_value(value, state, depth + 1)?;
                    let source = self.value_copy(value, depth + 1)?;
                    return self.choice(
                        vec![
                            Value::Error {
                                is_error: false,
                                payload: Box::new(Value::Boxed(Box::new(source))),
                            },
                            Value::Error {
                                is_error: true,
                                payload: Box::new(Value::Plain),
                            },
                        ],
                        depth + 1,
                    );
                }
                IntrinsicKind::RegistrationStart {
                    asynchronous: true, ..
                } => {
                    let root = Root::Created(owner, expression.id);
                    self.change(state, root, OPEN | CLOSED, true, depth)?;
                    return Ok(Value::Root(root, true, true, None));
                }
                IntrinsicKind::NominalConstructor { .. } => {
                    return Err(failure(
                        "original constructor call lacks its complete expression theorem",
                    ));
                }
                IntrinsicKind::RegistrationOperation(
                    op @ (RegistrationOp::AsyncValid
                    | RegistrationOp::AsyncRequest
                    | RegistrationOp::AsyncPollClose
                    | RegistrationOp::AsyncFinish
                    | RegistrationOp::AsyncFinishChecked),
                ) => {
                    let [value] = values else {
                        return Err(failure(
                            "sealed protocol operation changes original argument arity",
                        ));
                    };
                    let root = self.root(value, state, depth)?;
                    let phase = state.roots[&root];
                    if op == RegistrationOp::AsyncValid {
                        return Ok(Value::Valid {
                            root,
                            fresh: true,
                            negated: false,
                        });
                    }
                    if matches!(
                        op,
                        RegistrationOp::AsyncFinish | RegistrationOp::AsyncFinishChecked
                    ) {
                        if phase != CLOSED {
                            return Err(failure(
                                "original registration reclamation is not dominated by Closed for this owning root",
                            ));
                        }
                        self.change(state, root, CLOSED, true, depth)?;
                        return Ok(Value::Plain);
                    }
                    let closing = if phase & (OPEN | CLOSING) != 0 {
                        CLOSING
                    } else {
                        0
                    };
                    if op == RegistrationOp::AsyncRequest {
                        self.change(state, root, closing | (phase & CLOSED), false, depth)?;
                        return Ok(Value::Plain);
                    }
                    self.change(state, root, closing | CLOSED, false, depth)?;
                    self.pay(4, depth)?;
                    let mut outcomes = Vec::new();
                    if closing != 0 {
                        outcomes.push(Outcome {
                            exit: ProtocolExit::PollPending,
                            phases: BTreeMap::from([(root, CLOSING)]),
                        });
                    }
                    outcomes.push(Outcome {
                        exit: ProtocolExit::PollReady,
                        phases: BTreeMap::from([(root, CLOSED)]),
                    });
                    return Ok(Value::Poll {
                        fresh: true,
                        outcomes,
                    });
                }
                // Other sealed primitives do not gain protocol facts. Moving a
                // root through a Task or container requires its separate proof.
                _ => return self.unknown_call(owner, expression.id, values, state, depth),
            }
        }
        self.pay(self.proof.call_targets.len(), depth)?;
        let target = self.proof.call_targets.get(&(owner, site)).copied();
        let Some(target) = target else {
            // An abstract or environment call may reach a captured root even
            // without an explicit resource parameter. It has no neutral-state
            // theorem merely because its value signature is Copy.
            self.pay(state.roots.len(), depth)?;
            let roots: Vec<_> = state.roots.keys().copied().collect();
            for root in roots {
                self.change(state, root, UNKNOWN, false, depth)?;
            }
            return self.unknown_call(owner, expression.id, values, state, depth);
        };
        let callee = self.definition(target)?;
        let SemanticDefinitionBody::FunctionWithBody {
            protocol_contracts,
            parameters,
            ..
        } = &callee.body
        else {
            return self.unknown_call(owner, expression.id, values, state, depth);
        };
        if protocol_contracts.is_empty() {
            return self.unknown_call(owner, expression.id, values, state, depth);
        }
        self.function(target, depth + 1)?;
        if !self.proof.protocol_complete.contains(&target) {
            return Err(failure(
                "callee protocol body is not independently complete",
            ));
        }
        if parameters.len() != values.len() {
            return Err(pending(
                "protocol method call requires its full original receiver argument correspondence",
            ));
        }
        self.pay(protocol_contracts.len().saturating_add(values.len()), depth)?;
        let mut roles = Vec::new();
        let mut seen = BTreeSet::new();
        for contract in protocol_contracts {
            let argument = values
                .get(contract.parameter as usize)
                .ok_or_else(|| failure("protocol parameter is not an original call argument"))?;
            let root = self.root(argument, state, depth)?;
            if !seen.insert(root) {
                return Err(failure(
                    "independent protocol roles alias one exclusive original root",
                ));
            }
            let phase = state.roots[&root];
            if phase == 0 || phase & !mask(&contract.entry) != 0 {
                return Err(failure(
                    "caller has not proved the protocol entry state of this actual argument root",
                ));
            }
            roles.push((root, phase, contract));
        }
        self.pay(5usize.saturating_mul(roles.len()), depth)?;
        let mut outcomes = Vec::new();
        for exit in [
            ProtocolExit::Return,
            ProtocolExit::ErrorOk,
            ProtocolExit::ErrorErr,
            ProtocolExit::PollPending,
            ProtocolExit::PollReady,
        ] {
            let mut phases = BTreeMap::new();
            for (root, phase, contract) in &roles {
                let mut output = 0;
                self.pay(contract.exits.len().saturating_mul(4), depth)?;
                for row in &contract.exits {
                    if bit(row.input) & *phase != 0 && row.exit == exit {
                        output |= mask(&row.output);
                    }
                }
                if output == 0 {
                    phases.clear();
                    break;
                }
                phases.insert(*root, output);
            }
            if !phases.is_empty() {
                outcomes.push(Outcome { exit, phases });
            }
        }
        if outcomes.is_empty() {
            return Err(failure(
                "callee protocol has no reachable result for the proved entry states",
            ));
        }
        for (root, _, _) in &roles {
            let joined = outcomes
                .iter()
                .fold(0, |m, o| m | o.phases.get(root).copied().unwrap_or(0));
            self.change(state, *root, joined, false, depth)?;
        }
        if outcomes.iter().all(|o| o.exit == ProtocolExit::Return) {
            return Ok(Value::Plain);
        }
        if outcomes.iter().any(|o| {
            matches!(
                o.exit,
                ProtocolExit::ErrorOk | ProtocolExit::ErrorErr | ProtocolExit::Return
            )
        }) {
            return Err(pending(
                "original protocol call requires a complete ErrorUnion result correlation",
            ));
        }
        Ok(Value::Poll {
            fresh: true,
            outcomes,
        })
    }
    fn unknown_call(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        values: &[Value],
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        let mut roots = BTreeSet::new();
        for value in values {
            self.pay(value.work(), depth)?;
            value.roots(&mut roots);
        }
        // Missing tables cannot promise absence of a mutation.
        for root in roots {
            self.change(state, root, UNKNOWN, false, depth)?;
        }
        if self.query_async(owner, Query::Expression(node), depth)? {
            let root = Root::Created(owner, node);
            self.change(state, root, UNKNOWN, true, depth)?;
            Ok(Value::Root(root, true, true, None))
        } else {
            Ok(Value::Plain)
        }
    }
    fn items(
        &mut self,
        owner: SemanticDefinitionId,
        items: &[SemanticOriginalExpr],
        state: &mut State,
        depth: usize,
    ) -> R<Vec<Value>> {
        self.pay(items.len(), depth)?;
        let start = state.temporaries.len();
        for item in items {
            let value = self.expr(owner, item, state, depth + 1)?;
            state.temporaries.push(value);
        }
        Ok(state.temporaries.drain(start..).collect())
    }
    fn expr(
        &mut self,
        owner: SemanticDefinitionId,
        expr: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        if !state.reachable {
            return Ok(Value::Plain);
        }
        let mut value = self
            .expr_inner(owner, expr, state, depth)
            .map_err(|mut diagnostic| {
                diagnostic.notes.push(format!(
                    "original operation owner {:?}, expression {:?}, span {:?}, kind {:?}",
                    owner,
                    expr.id,
                    expr.span,
                    std::mem::discriminant(&expr.kind)
                ));
                diagnostic
            })?;
        self.pay(self.proof.injections.len(), depth)?;
        if let Some(injection) = self.proof.injections.get(&(owner, expr.id)) {
            let is_error = injection.is_error;
            let payload = copy_recipe(&injection.payload, self.limits, self.left)?;
            value = Value::Error {
                is_error,
                payload: Box::new(self.typed(owner, &payload, value, depth + 1)?),
            };
        }
        if let Value::Root(_, true, owning, _) = &mut value {
            if let Some(actual) =
                self.query_ownership(owner, Query::Expression(expr.id), depth + 1)?
            {
                *owning = actual;
            }
        }
        if let Some(ty) = self.query_type(owner, Query::Expression(expr.id), depth + 1)? {
            let category = self.category(owner, &ty, depth + 1)?;
            if category == cleanup::Category::Own
                && matches!(
                    expr.kind,
                    CoreExprKind::Name(_) | CoreExprKind::Member { .. }
                )
            {
                // A sealed constructor expression can be spelled as a static
                // member. It creates a value instead of reading a local place.
                self.pay(self.proof.nominal_constructors.len(), depth)?;
                let constructed = if self
                    .proof
                    .nominal_constructors
                    .contains_key(&(owner, expr.id))
                {
                    true
                } else if let Some(site) = self.site(owner, expr.id, depth + 1)? {
                    self.pay(self.proof.intrinsics.len(), depth)?;
                    matches!(
                        self.proof
                            .intrinsics
                            .get(&(owner, site))
                            .map(|row| &row.kind),
                        Some(IntrinsicKind::NominalConstructor { .. })
                    )
                } else {
                    false
                };
                if !constructed {
                    self.move_place(expr, state, depth + 1).map_err(|mut diagnostic| {
                        diagnostic.notes.push(format!("original transfer owner {:?}, expression {:?}, span {:?}, operation {:?}", owner, expr.id, expr.span, std::mem::discriminant(&expr.kind)));
                        diagnostic
                    })?;
                }
            }
            value = self.typed(owner, &ty, value, depth + 1)?;
        }
        Ok(value)
    }
    fn expr_inner(
        &mut self,
        owner: SemanticDefinitionId,
        expr: &SemanticOriginalExpr,
        state: &mut State,
        depth: usize,
    ) -> R<Value> {
        self.pay(1, depth)?;
        match &expr.kind {
            CoreExprKind::Name(name) => {
                self.pay(name.len().saturating_add(state.names.len()), depth)?;
                if let Some(value) = state.names.get(name).and_then(|s| s.last()) {
                    self.value_copy(value, depth)
                } else {
                    Ok(Value::Plain)
                }
            }
            CoreExprKind::Call { callee, arguments } => {
                let site = self.site(owner, expr.id, depth + 1)?;
                self.pay(
                    self.proof
                        .callable_calls
                        .len()
                        .saturating_add(self.proof.method_calls.len())
                        .saturating_add(self.proof.intrinsics.len()),
                    depth,
                )?;
                let once = site
                    .and_then(|site| self.proof.callable_calls.get(&(owner, site)))
                    .is_some_and(|call| call.kind == CallableKind::FnOnce);
                let mut receiver = None;
                if let Some(site) = site {
                    if let Some(method) = self.proof.method_calls.get(&(owner, site)) {
                        let declaration_id = method.declaration;
                        crate::dyn_metadata::semantic_operand_snapshot(
                            &method.signature,
                            self.limits,
                            self.left,
                            1,
                        )?;
                        let signature = method.signature.clone();
                        let declaration = self.definition(declaration_id)?;
                        if let SemanticDefinitionBody::FunctionWithBody {
                            receiver_parameter: Some(index),
                            ..
                        } = &declaration.body
                        {
                            let parameter=signature.parameters.get(*index as usize).ok_or_else(||failure("original method receiver ordinal is absent from its complete scheme"))?;
                            receiver = Some((
                                *index as usize,
                                parameter.mode,
                                copy_recipe(&parameter.ty, self.limits, self.left)?,
                            ));
                        }
                    }
                }
                let dynamic = site
                    .and_then(|site| self.proof.intrinsics.get(&(owner, site)))
                    .is_some_and(|call| matches!(call.kind, IntrinsicKind::DynCall { .. }));
                let mut callee_base = callee.as_ref();
                while let CoreExprKind::TypeApply { callee, .. }
                | CoreExprKind::GenericApply { callee, .. } = &callee_base.kind
                {
                    self.pay(1, depth)?;
                    callee_base = callee;
                }
                let object = if receiver.is_some() || dynamic {
                    let CoreExprKind::Member { object, .. } = &callee_base.kind else {
                        return Err(failure(
                            "original receiver call has no actual source object",
                        ));
                    };
                    Some(object.as_ref())
                } else {
                    None
                };
                let source = object.unwrap_or(callee.as_ref());
                let mut callee_value = self.observe(owner, source, state, depth + 1)?;
                let consume_receiver = if let Some((_, mode, ty)) = &receiver {
                    matches!(mode, PassingMode::Give | PassingMode::Move)
                        || *mode == PassingMode::Infer
                            && cleanup::category(
                                self.program,
                                self.proof,
                                owner,
                                ty,
                                self.limits,
                                self.left,
                            )? == cleanup::Category::Own
                } else {
                    false
                };
                let consume = once || consume_receiver;
                let temporary = self.place_path(source, depth + 1)?.is_none();
                let callee_slot = state.temporaries.len();
                if consume && !temporary {
                    self.move_place(source, state, depth + 1)?;
                }
                let receiver_value = if object.is_some() {
                    let value = self.value_copy(&callee_value, depth + 1)?;
                    Some(self.view(value, depth + 1)?)
                } else {
                    None
                };
                if !consume && !temporary {
                    callee_value = self.view(callee_value, depth + 1)?;
                }
                if consume || temporary {
                    self.pay(1, depth)?;
                    state.temporaries.push(callee_value);
                }
                let mut values = self.items(owner, arguments, state, depth + 1)?;
                if let Some(value) = receiver_value {
                    let ordinal = receiver.as_ref().map_or(0, |(index, _, _)| *index);
                    if ordinal > values.len() {
                        return Err(failure(
                            "original receiver ordinal changes complete argument arity",
                        ));
                    }
                    self.pay(values.len().saturating_add(1), depth)?;
                    values.insert(ordinal, value);
                }
                let result = if state.reachable {
                    self.call(owner, expr, arguments, &values, state, depth + 1)?
                } else {
                    Value::Plain
                };
                if consume || temporary {
                    let callee = state.temporaries.remove(callee_slot);
                    if !consume && state.reachable {
                        self.drop_value(&callee, state, depth + 1)?;
                    }
                }
                Ok(result)
            }
            CoreExprKind::Unary { op, operand } => {
                let borrowing = matches!(
                    op,
                    UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Dereference | UnaryOp::Give
                );
                let mut value = if borrowing {
                    self.observe(owner, operand, state, depth + 1)?
                } else {
                    self.expr(owner, operand, state, depth + 1)?
                };
                if *op == UnaryOp::Give {
                    self.move_place(operand, state, depth + 1)?;
                    if let Value::Root(_, _, owning, _) = &mut value {
                        *owning = true;
                    }
                    return Ok(value);
                }
                if matches!(
                    op,
                    UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Dereference
                ) {
                    if let Value::Root(_, _, owning, _) = &mut value {
                        *owning = false;
                    }
                }
                if *op == UnaryOp::Not {
                    if let Value::Valid {
                        root,
                        fresh,
                        negated,
                    } = value
                    {
                        return Ok(Value::Valid {
                            root,
                            fresh,
                            negated: !negated,
                        });
                    }
                    if let Value::Bool(value) = value {
                        return Ok(Value::Bool(!value));
                    }
                    return Ok(Value::Plain);
                }
                if matches!(
                    op,
                    UnaryOp::Borrow | UnaryOp::BorrowMut | UnaryOp::Give | UnaryOp::Dereference
                ) {
                    Ok(value)
                } else {
                    Ok(Value::Plain)
                }
            }
            CoreExprKind::Group(value) | CoreExprKind::Ascribed { value, .. } => {
                self.expr(owner, value, state, depth + 1)
            }
            CoreExprKind::TypeApply { callee, .. } | CoreExprKind::GenericApply { callee, .. } => {
                self.expr(owner, callee, state, depth + 1)
            }
            CoreExprKind::Tuple(items) | CoreExprKind::Array(items) => {
                Ok(Value::Tuple(self.items(owner, items, state, depth + 1)?))
            }
            CoreExprKind::Record { fields, .. }
            | CoreExprKind::RecordConstructor { fields, .. } => {
                self.pay(fields.len(), depth)?;
                let start = state.temporaries.len();
                for (_, expr) in fields {
                    let value = self.expr(owner, expr, state, depth + 1)?;
                    state.temporaries.push(value);
                }
                let values: Vec<_> = state.temporaries.drain(start..).collect();
                let mut out = BTreeMap::new();
                for ((name, _), value) in fields.iter().zip(values) {
                    self.pay(name.len().saturating_add(1), depth)?;
                    out.insert(name.clone(), value);
                }
                Ok(Value::Fields(out))
            }
            CoreExprKind::Member { .. } => {
                if let Some(value) = self.nominal_value(owner, expr.id, &[], state, depth + 1)? {
                    Ok(value)
                } else {
                    self.observe(owner, expr, state, depth + 1)
                }
            }
            CoreExprKind::Index { object, index } => {
                let value = self.observe(owner, object, state, depth + 1)?;
                self.expr(owner, index, state, depth + 1)?;
                let mut roots = BTreeSet::new();
                self.pay(value.work(), depth)?;
                value.roots(&mut roots);
                if !roots.is_empty() {
                    return Err(pending(
                        "protocol root projection requires its exact original constant index",
                    ));
                }
                Ok(Value::Plain)
            }
            CoreExprKind::Slice { object, start, end } => {
                // The original place rule already proves the array/slice and
                // integer endpoints. Slicing creates a view and transfers no
                // owning element or container responsibility.
                let source = self.observe(owner, object, state, depth + 1)?;
                let begin = state.temporaries.len();
                state.temporaries.push(self.view(source, depth + 1)?);
                for bound in start.iter().chain(end.iter()) {
                    let value = self.expr(owner, bound, state, depth + 1)?;
                    self.drop_value(&value, state, depth + 1)?;
                }
                state.temporaries.truncate(begin);
                Ok(Value::Plain)
            }
            CoreExprKind::Sequence { bindings, value } => {
                let mut old = Vec::new();
                let error_start = state.abrupt.len();
                self.pay(bindings.len(), depth)?;
                for (name, expr) in bindings {
                    let value = self.expr(owner, expr, state, depth + 1)?;
                    self.pay(name.len().saturating_add(1), depth)?;
                    old.push((name.clone(), state.names.get(name).map_or(0, Vec::len)));
                    state.names.entry(name.clone()).or_default().push(value);
                }
                let result = self.expr(owner, value, state, depth + 1)?;
                self.restore(state, &old, depth)?;
                let exits = state.abrupt.drain(error_start..).collect::<Vec<_>>();
                for mut exit in exits {
                    self.restore(&mut exit, &old, depth + 1)?;
                    state.abrupt.push(exit);
                }
                Ok(result)
            }
            CoreExprKind::If {
                condition,
                then_value,
                else_value,
            } => {
                let condition = self.expr(owner, condition, state, depth + 1)?;
                let mut left = self.copy(state, depth)?;
                let mut right = self.copy(state, depth)?;
                self.condition(&condition, true, &mut left, depth)?;
                self.condition(&condition, false, &mut right, depth)?;
                let a = self.expr(owner, then_value, &mut left, depth + 1)?;
                let b = self.expr(owner, else_value, &mut right, depth + 1)?;
                // Unknown result provenance cannot supply a phase refinement.
                self.pay(a.work().saturating_add(b.work()), depth)?;
                let result = if !left.reachable {
                    b
                } else if !right.reachable || a == b {
                    a
                } else {
                    self.pay(2, depth)?;
                    self.choice(vec![a, b], depth + 1)?
                };
                self.merge(state, &left, &right, depth)?;
                Ok(result)
            }
            CoreExprKind::Try(value) => {
                let value = self.expr(owner, value, state, depth + 1)?;
                let definition = self.definition(owner)?;
                if matches!(&definition.body,SemanticDefinitionBody::FunctionWithBody{protocol_contracts,..}if !protocol_contracts.is_empty())
                {
                    return Err(pending(
                        "original protocol ErrorUnion exits require their complete conditional table correlation",
                    ));
                }
                let mut payloads = vec![value];
                let mut normal = Vec::new();
                while let Some(value) = payloads.pop() {
                    self.pay(value.work().saturating_add(1), depth)?;
                    match value {
                        Value::Choice(values) => {
                            self.pay(values.len(), depth)?;
                            payloads.extend(values);
                        }
                        Value::Error {
                            is_error: true,
                            payload,
                        } => {
                            let mut exit = self.copy(state, depth + 1)?;
                            exit.abrupt.clear();
                            exit.control = Control::Return;
                            exit.returned = Value::Error {
                                is_error: true,
                                payload,
                            };
                            for value in &exit.temporaries {
                                self.drop_value(value, &exit, depth + 1)?;
                            }
                            exit.temporaries.clear();
                            self.pay(1, depth)?;
                            state.abrupt.push(exit);
                        }
                        Value::Error {
                            is_error: false,
                            payload,
                        } => normal.push(*payload),
                        Value::Plain => {
                            // The whole union is Copy; the two ordinary payload
                            // branches have no owning cleanup responsibility.
                            let mut exit = self.copy(state, depth + 1)?;
                            exit.abrupt.clear();
                            exit.control = Control::Return;
                            exit.returned = Value::Plain;
                            for value in &exit.temporaries {
                                self.drop_value(value, &exit, depth + 1)?;
                            }
                            exit.temporaries.clear();
                            state.abrupt.push(exit);
                            normal.push(Value::Plain);
                        }
                        _ => {
                            return Err(pending(
                                "original cleanup propagation lacks its exact ErrorUnion payload inventory",
                            ));
                        }
                    }
                }
                if normal.is_empty() {
                    state.reachable = false;
                    Ok(Value::Plain)
                } else {
                    self.choice(normal, depth + 1)
                }
            }
            CoreExprKind::Binary {
                op: op @ (BinaryOp::And | BinaryOp::Or),
                left,
                right,
            } => {
                let left = self.expr(owner, left, state, depth + 1)?;
                let mut skip = self.copy(state, depth + 1)?;
                let execute = *op == BinaryOp::And;
                self.condition(&left, !execute, &mut skip, depth + 1)?;
                self.condition(&left, execute, state, depth + 1)?;
                let value = if state.reachable {
                    self.expr(owner, right, state, depth + 1)?
                } else {
                    Value::Plain
                };
                let run = self.copy(state, depth + 1)?;
                self.merge(state, &run, &skip, depth + 1)?;
                self.choice(vec![value, Value::Bool(!execute)], depth + 1)
            }
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::Range {
                start: left,
                end: right,
            } => {
                self.expr(owner, left, state, depth + 1)?;
                self.expr(owner, right, state, depth + 1)?;
                Ok(Value::Plain)
            }
            CoreExprKind::NumericConvert { value, .. }
            | CoreExprKind::IntegerConvert { value, .. } => {
                self.expr(owner, value, state, depth + 1)?;
                Ok(Value::Plain)
            }
            CoreExprKind::Closure { move_capture, .. } => {
                let child = self.closure_child(owner, expr.id, depth + 1)?;
                self.pay(
                    self.proof
                        .capture_names
                        .len()
                        .saturating_add(self.proof.closure_move.len()),
                    depth,
                )?;
                if self.proof.closure_move.get(&child.id) != Some(move_capture) {
                    return Err(failure(
                        "original cleanup closure mode differs from its true creator",
                    ));
                }
                let names = self.proof.capture_names.get(&child.id).ok_or_else(|| {
                    failure("original cleanup closure lacks its complete capture directory")
                })?;
                self.pay(
                    names
                        .iter()
                        .fold(names.len(), |n, name| n.saturating_add(name.len())),
                    depth,
                )?;
                let names = self
                    .proof
                    .capture_names
                    .get(&child.id)
                    .ok_or_else(|| failure("original cleanup capture directory disappeared"))?
                    .clone();
                let mut captures = BTreeMap::new();
                for name in names {
                    let source = state
                        .names
                        .get(&name)
                        .and_then(|stack| stack.last())
                        .ok_or_else(|| {
                            failure("original cleanup capture has no live creator binder")
                        })?;
                    let mut value = self.value_copy(source, depth + 1)?;
                    let ty = self.capture_type(owner, expr.id, &name, child, depth + 1)?;
                    if *move_capture {
                        if !cleanup::copy(
                            self.program,
                            self.proof,
                            owner,
                            &ty,
                            self.limits,
                            self.left,
                        )? {
                            *state
                                .names
                                .get_mut(&name)
                                .and_then(|stack| stack.last_mut())
                                .ok_or_else(|| {
                                    failure("original cleanup capture lost its creator binder")
                                })? = Value::Plain;
                        }
                    } else {
                        value = self.view(value, depth + 1)?;
                    }
                    self.pay(name.len().saturating_add(1), depth)?;
                    captures.insert(name, value);
                }
                Ok(Value::Closure {
                    definition: child.id,
                    captures,
                })
            }
            CoreExprKind::Bool(value) => Ok(Value::Bool(*value)),
            CoreExprKind::Literal(Constant::Bool(value)) => Ok(Value::Bool(*value)),
            CoreExprKind::Literal(_)
            | CoreExprKind::Integer(_)
            | CoreExprKind::Float(_)
            | CoreExprKind::String(_)
            | CoreExprKind::Character(_)
            | CoreExprKind::Unit => Ok(Value::Plain),
            CoreExprKind::CompileTime(_)
            | CoreExprKind::CompileTimeBlock(_)
            | CoreExprKind::SyntaxQuote { .. } => Ok(Value::Plain),
            _ => Err(pending(
                "original protocol operation requires its complete source evaluation rule",
            )),
        }
    }
    fn restore(&mut self, state: &mut State, old: &[(String, usize)], depth: usize) -> R<()> {
        self.pay(
            old.iter().fold(state.names.len(), |w, (n, _)| {
                w.saturating_add(n.len()).saturating_add(1)
            }),
            depth,
        )?;
        for (name, len) in old.iter().rev() {
            if let Some(values) = state.names.get(name) {
                for value in values.iter().skip(*len) {
                    self.drop_value(value, state, depth + 1)?;
                }
            }
            if *len == 0 {
                state.names.remove(name);
            } else if let Some(values) = state.names.get_mut(name) {
                values.truncate(*len);
            }
        }
        Ok(())
    }
    fn condition(&mut self, value: &Value, truth: bool, state: &mut State, depth: usize) -> R<()> {
        self.pay(value.work().saturating_add(state.roots.len()), depth)?;
        if matches!(value,Value::Bool(actual) if *actual != truth) {
            state.reachable = false;
        }
        if let Value::Valid {
            root,
            fresh: true,
            negated,
        } = value
        {
            if truth == *negated {
                let phase = state.roots.get_mut(root).ok_or_else(|| {
                    failure("validity predicate lost its exact original owning root")
                })?;
                *phase &= CLOSED;
            }
        }
        Ok(())
    }
    fn merge(&mut self, output: &mut State, a: &State, b: &State, depth: usize) -> R<()> {
        self.pay(a.work().saturating_add(b.work()), depth)?;
        if !a.reachable {
            *output = self.copy(b, depth + 1)?;
            return Ok(());
        }
        if !b.reachable {
            *output = self.copy(a, depth + 1)?;
            return Ok(());
        }
        self.charge_state(a, depth + 1)?;
        self.charge_state(b, depth + 1)?;
        *output = a.clone();
        for exit in &b.abrupt {
            let exit = self.copy(exit, depth + 1)?;
            self.offer(&mut output.abrupt, exit, depth + 1)?;
        }
        for (root, phase) in &b.roots {
            *output.roots.entry(*root).or_default() |= *phase;
        }
        for (name, values) in &mut output.names {
            if let Some(other) = b.names.get(name) {
                if values.len() != other.len() {
                    return Err(failure("protocol lexical merge changes binder scope"));
                }
                for (value, other) in values.iter_mut().zip(other) {
                    if value != other {
                        let old = self.value_copy(value, depth + 1)?;
                        let incoming = self.value_copy(other, depth + 1)?;
                        *value = self.choice(vec![old, incoming], depth + 1)?;
                    }
                }
            } else {
                return Err(failure(
                    "original cleanup merge changes the live lexical binder directory",
                ));
            }
        }
        for (value, other) in output.temporaries.iter_mut().zip(&b.temporaries) {
            if value != other {
                let old = self.value_copy(value, depth + 1)?;
                let incoming = self.value_copy(other, depth + 1)?;
                *value = self.choice(vec![old, incoming], depth + 1)?;
            }
        }
        Ok(())
    }
    fn pattern(
        &mut self,
        pattern: &SemanticOriginalPattern,
        value: Value,
        state: &mut State,
        depth: usize,
    ) -> R<()> {
        self.pay(value.work().saturating_add(1), depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                self.pay(name.len().saturating_add(1), depth)?;
                state.names.entry(name.clone()).or_default().push(value);
            }
            CorePatternKind::Wildcard
            | CorePatternKind::Integer(_)
            | CorePatternKind::Bool(_)
            | CorePatternKind::Float(_)
            | CorePatternKind::String(_)
            | CorePatternKind::Character(_) => self.drop_value(&value, state, depth + 1)?,
            CorePatternKind::Reference { pattern, .. } => {
                let value = self.view(value, depth + 1)?;
                self.pattern(pattern, value, state, depth + 1)?;
            }
            CorePatternKind::Tuple(patterns) => {
                let values = match value {
                    Value::Tuple(values) => values,
                    Value::Fields(mut fields) => {
                        self.pay(patterns.len().saturating_mul(32), depth)?;
                        let mut values = Vec::with_capacity(patterns.len());
                        for index in 0..patterns.len() {
                            values.push(fields.remove(&index.to_string()).ok_or_else(|| {
                                failure("cleanup tuple pattern omits a complete original field")
                            })?);
                        }
                        if !fields.is_empty() {
                            return Err(failure("cleanup tuple pattern changes original arity"));
                        }
                        values
                    }
                    Value::Plain => {
                        self.pay(patterns.len(), depth)?;
                        vec![Value::Plain; patterns.len()]
                    }
                    _ => {
                        return Err(pending(
                            "original cleanup tuple pattern lacks its complete owning inventory",
                        ));
                    }
                };
                if values.len() != patterns.len() {
                    return Err(failure(
                        "original cleanup tuple pattern changes original arity",
                    ));
                }
                for (pattern, value) in patterns.iter().zip(values) {
                    self.pattern(pattern, value, state, depth + 1)?;
                }
            }
            CorePatternKind::Variant {
                variant, fields, ..
            }
            | CorePatternKind::Constructor {
                variant, fields, ..
            } => {
                let values = match value {
                    Value::Variant {
                        name,
                        fields: values,
                    } => {
                        if name != *variant {
                            return Err(failure(
                                "original cleanup binds an inactive known constructor arm",
                            ));
                        }
                        values
                    }
                    Value::Cleanup { owner, ty } => {
                        let fields = cleanup::variant(
                            self.program,
                            self.proof,
                            owner,
                            &ty,
                            variant,
                            self.limits,
                            self.left,
                        )?
                        .ok_or_else(|| {
                            pending(
                                "original cleanup variant lacks its complete source nominal scheme",
                            )
                        })?;
                        let mut values = Vec::with_capacity(fields.len());
                        for ty in fields {
                            values.push(self.typed(owner, &ty, Value::Plain, depth + 1)?);
                        }
                        values
                    }
                    Value::Plain | Value::Poll { .. } => {
                        self.pay(fields.len(), depth)?;
                        vec![Value::Plain; fields.len()]
                    }
                    _ => {
                        return Err(pending(
                            "original cleanup constructor pattern lacks its exact owning payload inventory",
                        ));
                    }
                };
                if values.len() != fields.len() {
                    return Err(failure(
                        "original cleanup constructor pattern changes source field arity",
                    ));
                }
                for (pattern, value) in fields.iter().zip(values) {
                    self.pattern(pattern, value, state, depth + 1)?;
                }
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                let mut values = match value {
                    Value::Fields(values) => values,
                    Value::Plain => BTreeMap::new(),
                    _ => {
                        return Err(pending(
                            "original cleanup record pattern lacks its complete physical fields",
                        ));
                    }
                };
                for (name, pattern) in fields {
                    self.pay(name.len().saturating_add(values.len()), depth)?;
                    self.pattern(
                        pattern,
                        values.remove(name).unwrap_or(Value::Plain),
                        state,
                        depth + 1,
                    )?;
                }
                for value in values.values() {
                    self.drop_value(value, state, depth + 1)?;
                }
            }
            CorePatternKind::ErrorArm { is_error, pattern } => {
                let payload = match value {
                    Value::Error {
                        is_error: actual,
                        payload,
                    } if *is_error == actual => *payload,
                    Value::Plain => Value::Plain,
                    _ => {
                        return Err(pending(
                            "original cleanup error pattern lacks its exact owning arm",
                        ));
                    }
                };
                self.pattern(pattern, payload, state, depth + 1)?;
            }
        }
        Ok(())
    }
    fn matches(
        &mut self,
        pattern: &SemanticOriginalPattern,
        value: &Value,
        depth: usize,
    ) -> R<Option<bool>> {
        self.pay(value.work().saturating_add(1), depth)?;
        Ok(match (&pattern.kind, value) {
            (CorePatternKind::Name(_) | CorePatternKind::Wildcard, _) => Some(true),
            (CorePatternKind::Reference { pattern, .. }, value) => {
                return self.matches(pattern, value, depth + 1);
            }
            (
                CorePatternKind::Variant { variant, .. }
                | CorePatternKind::Constructor { variant, .. },
                Value::Variant { name, .. },
            ) => Some(variant == name),
            (
                CorePatternKind::ErrorArm { is_error, .. },
                Value::Error {
                    is_error: actual, ..
                },
            ) => Some(is_error == actual),
            (CorePatternKind::Bool(expected), Value::Bool(actual)) => Some(expected == actual),
            _ => None,
        })
    }
    fn selected(
        &mut self,
        owner: SemanticDefinitionId,
        pattern: &SemanticOriginalPattern,
        value: &Value,
        state: &State,
        depth: usize,
    ) -> R<Vec<State>> {
        self.pay(value.work(), depth)?;
        if let Value::Choice(values) = value {
            let mut out = Vec::new();
            for value in values {
                out.extend(self.selected(owner, pattern, value, state, depth + 1)?);
            }
            return Ok(out);
        }
        if self.matches(pattern, value, depth + 1)? == Some(false) {
            return Ok(Vec::new());
        }
        if let Value::Poll { fresh, outcomes } = value {
            if !fresh {
                // An old Copy discriminator remains an ordinary value; it
                // supplies no state fact about the current root generation.
                let mut branch = self.copy(state, depth)?;
                self.pattern(pattern, Value::Plain, &mut branch, depth + 1)?;
                return Ok(vec![branch]);
            }
            let variant = match &pattern.kind {
                CorePatternKind::Variant { variant, .. }
                | CorePatternKind::Constructor { variant, .. } => Some(variant.as_str()),
                CorePatternKind::Name(_) | CorePatternKind::Wildcard => None,
                _ => {
                    return Err(failure(
                        "protocol Poll tag selection is not a nominal constructor pattern",
                    ));
                }
            };
            // Γ already proved the exact subject nominal and variant. These
            // spellings select its closed Poll arms; they grant no type identity.
            let wanted = match variant {
                Some("Pending") => Some(ProtocolExit::PollPending),
                Some("Ready") => Some(ProtocolExit::PollReady),
                None => None,
                _ => {
                    return Err(failure(
                        "original Poll pattern selects a nonexistent sealed arm",
                    ));
                }
            };
            let mut out = Vec::new();
            for outcome in outcomes {
                if wanted.is_some_and(|w| w != outcome.exit) {
                    continue;
                }
                let mut branch = self.copy(state, depth)?;
                for (root, phase) in &outcome.phases {
                    if !branch.roots.contains_key(root) {
                        return Err(failure("Poll refinement lost its original root"));
                    }
                    branch.roots.insert(*root, *phase);
                }
                let bound = self.value_copy(value, depth)?;
                self.pattern(pattern, bound, &mut branch, depth + 1)?;
                out.push(branch);
            }
            Ok(out)
        } else {
            let mut state = self.copy(state, depth)?;
            let value = self.value_copy(value, depth)?;
            self.pattern(pattern, value, &mut state, depth + 1)?;
            let _ = owner;
            Ok(vec![state])
        }
    }
    fn offer(&mut self, seen: &mut Vec<State>, state: State, depth: usize) -> R<bool> {
        self.charge_state(&state, depth + 1)?;
        for old in seen.iter() {
            self.charge_state(old, depth + 1)?;
        }
        for old in seen.iter() {
            self.pay(old.work().saturating_add(state.work()), depth)?;
            if old == &state {
                return Ok(false);
            }
        }
        self.pay(state.work().saturating_add(1), depth)?;
        seen.push(state);
        Ok(true)
    }
    fn loop_body(
        &mut self,
        owner: SemanticDefinitionId,
        body: &SemanticOriginalBlock,
        condition: Option<&SemanticOriginalExpr>,
        pattern: Option<&SemanticOriginalPattern>,
        may_exit: bool,
        initial: State,
        depth: usize,
    ) -> R<Vec<State>> {
        let mut seen = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        let mut exits = Vec::new();
        let saved = self.copy(&initial, depth)?;
        self.offer(&mut seen, saved, depth)?;
        queue.push_back(initial);
        while let Some(mut state) = queue.pop_front() {
            self.pay(1, depth)?;
            let scope = self.scope(&state, depth)?;
            let mut entries = Vec::new();
            if let Some(condition) = condition {
                let value = self.expr(owner, condition, &mut state, depth + 1)?;
                // Both condition outcomes are real source paths. A Poll
                // constructor pattern refines only its own matching edge.
                let mut exit = self.copy(&state, depth)?;
                self.condition(&value, false, &mut exit, depth)?;
                if exit.reachable {
                    exits.push(exit);
                }
                if let Some(pattern) = pattern {
                    entries = self.selected(owner, pattern, &value, &state, depth + 1)?;
                } else {
                    self.condition(&value, true, &mut state, depth)?;
                    if state.reachable {
                        entries.push(state);
                    }
                }
            } else {
                if may_exit {
                    exits.push(self.copy(&state, depth)?);
                }
                entries.push(state);
            }
            for entry in entries {
                for mut next in self.block(owner, body, entry, depth + 1)? {
                    self.leave(&mut next, &scope, depth)?;
                    match next.control {
                        Control::Break => {
                            next.control = Control::Next;
                            exits.push(next);
                        }
                        Control::Return => exits.push(next),
                        Control::Next | Control::Continue => {
                            next.control = Control::Next;
                            // No fresh κ, epoch counter, or SSA equality is
                            // created at a backedge. Existing stale aliases stay
                            // stale; repeated complete states are the fixed point.
                            let saved = self.copy(&next, depth)?;
                            if self.offer(&mut seen, saved, depth)? {
                                queue.push_back(next);
                            }
                        }
                    }
                }
            }
        }
        Ok(exits)
    }
    fn present(&mut self, expected: &Value, actual: &Value, depth: usize) -> R<bool> {
        self.pay(expected.work().saturating_add(actual.work()), depth)?;
        self.charge_value(expected, depth + 1)?;
        self.charge_value(actual, depth + 1)?;
        if let Value::Choice(values) = actual {
            for value in values {
                if !self.present(expected, value, depth + 1)? {
                    return Ok(false);
                }
            }
            return Ok(!values.is_empty());
        }
        Ok(match (expected, actual) {
            (Value::Plain, _) | (Value::Bool(_), _) | (Value::Root(_, _, false, _), _) => true,
            (Value::Cleanup { .. }, Value::Plain) => false,
            (Value::Cleanup { .. }, _) => true,
            (Value::Root(_, _, true, _), Value::Root(_, true, true, _)) => true,
            (Value::Root(_, _, true, _), _) => false,
            (Value::Tuple(expected), Value::Tuple(actual)) if expected.len() == actual.len() => {
                for (expected, actual) in expected.iter().zip(actual) {
                    if !self.present(expected, actual, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Fields(expected), Value::Fields(actual))
            | (
                Value::Closure {
                    captures: expected, ..
                },
                Value::Closure {
                    captures: actual, ..
                },
            ) if expected.len() == actual.len() => {
                for (name, expected) in expected {
                    let Some(actual) = actual.get(name) else {
                        return Ok(false);
                    };
                    if !self.present(expected, actual, depth + 1)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Value::Boxed(_), Value::Boxed(_)) => true,
            (Value::Error { .. }, Value::Error { .. })
            | (Value::Variant { .. }, Value::Variant { .. }) => true,
            (Value::Choice(values), _) => {
                for expected in values {
                    if self.present(expected, actual, depth + 1)? {
                        return Ok(true);
                    }
                }
                false
            }
            (Value::Valid { .. } | Value::Poll { .. }, _) => true,
            _ => false,
        })
    }
    fn pattern_query(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        pattern: &SemanticOriginalPattern,
        path: &mut Vec<u32>,
        state: &mut State,
        depth: usize,
    ) -> R<()> {
        self.pay(path.len().saturating_add(1), depth)?;
        match &pattern.kind {
            CorePatternKind::Name(name) => {
                let ty = self
                    .query_type(
                        owner,
                        Query::BindingProjection {
                            statement,
                            pattern: path.clone(),
                        },
                        depth + 1,
                    )?
                    .ok_or_else(|| {
                        pending(
                            "original cleanup iteration binding lacks its final Gamma leaf type",
                        )
                    })?;
                let value = self.typed(owner, &ty, Value::Plain, depth + 1)?;
                self.pay(name.len().saturating_add(1), depth)?;
                state.names.entry(name.clone()).or_default().push(value);
            }
            CorePatternKind::Tuple(fields)
            | CorePatternKind::Variant { fields, .. }
            | CorePatternKind::Constructor { fields, .. } => {
                for (index, field) in fields.iter().enumerate() {
                    path.push(index as u32);
                    self.pattern_query(owner, statement, field, path, state, depth + 1)?;
                    path.pop();
                }
            }
            CorePatternKind::Record { fields, .. }
            | CorePatternKind::RecordConstructor { fields, .. } => {
                for (index, (_, field)) in fields.iter().enumerate() {
                    path.push(index as u32);
                    self.pattern_query(owner, statement, field, path, state, depth + 1)?;
                    path.pop();
                }
            }
            CorePatternKind::Reference { pattern, .. }
            | CorePatternKind::ErrorArm { pattern, .. } => {
                path.push(0);
                self.pattern_query(owner, statement, pattern, path, state, depth + 1)?;
                path.pop();
            }
            _ => {}
        }
        Ok(())
    }
    fn iteration(
        &mut self,
        owner: SemanticDefinitionId,
        statement: NodeId,
        pattern: &SemanticOriginalPattern,
        body: &SemanticOriginalBlock,
        initial: State,
        depth: usize,
    ) -> R<Vec<State>> {
        let mut seen = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        let mut exits = Vec::new();
        let initial_copy = self.copy(&initial, depth + 1)?;
        self.offer(&mut seen, initial_copy, depth + 1)?;
        queue.push_back(initial);
        while let Some(mut state) = queue.pop_front() {
            self.pay(1, depth)?;
            let scope = self.scope(&state, depth + 1)?;
            exits.push(self.copy(&state, depth + 1)?);
            self.pattern_query(
                owner,
                statement,
                pattern,
                &mut Vec::new(),
                &mut state,
                depth + 1,
            )?;
            for mut next in self.block(owner, body, state, depth + 1)? {
                self.leave(&mut next, &scope, depth + 1)?;
                match next.control {
                    Control::Break => {
                        next.control = Control::Next;
                        exits.push(next);
                    }
                    Control::Return => exits.push(next),
                    Control::Next | Control::Continue => {
                        next.control = Control::Next;
                        let saved = self.copy(&next, depth + 1)?;
                        if self.offer(&mut seen, saved, depth + 1)? {
                            queue.push_back(next);
                        }
                    }
                }
            }
        }
        Ok(exits)
    }
    fn scope(&mut self, state: &State, depth: usize) -> R<Vec<(String, usize)>> {
        self.pay(
            state
                .names
                .keys()
                .fold(0usize, |w, n| w.saturating_add(n.len()).saturating_add(1)),
            depth,
        )?;
        Ok(state
            .names
            .iter()
            .map(|(name, values)| (name.clone(), values.len()))
            .collect())
    }
    fn leave(&mut self, state: &mut State, scope: &[(String, usize)], depth: usize) -> R<()> {
        self.pay(
            scope
                .iter()
                .fold(state.names.len(), |w, (name, _)| {
                    w.saturating_add(name.len()).saturating_add(1)
                })
                .saturating_mul(state.names.len().saturating_add(1)),
            depth,
        )?;
        for (name, values) in &state.names {
            let keep = scope
                .iter()
                .find(|(n, _)| n == name)
                .map_or(0, |(_, len)| *len);
            for value in values.iter().skip(keep) {
                self.drop_value(value, state, depth + 1)
                    .map_err(|mut diagnostic| {
                        diagnostic.notes.push(format!(
                            "original scope cleanup binding {:?}, retained lexical depth {}",
                            name, keep
                        ));
                        diagnostic
                    })?;
            }
        }
        state.names.retain(|name, values| {
            if let Some((_, len)) = scope.iter().find(|(n, _)| n == name) {
                values.truncate(*len);
                true
            } else {
                false
            }
        });
        Ok(())
    }
    fn hide_for_defer(
        &mut self,
        state: &mut State,
        visible: &[(String, usize)],
        depth: usize,
    ) -> R<(Vec<(String, usize)>, usize)> {
        self.pay(
            state.work().saturating_mul(visible.len().saturating_add(1)),
            depth,
        )?;
        let start = state.temporaries.len();
        let mut hidden = Vec::new();
        for (name, values) in &mut state.names {
            let keep = visible
                .iter()
                .find(|(key, _)| key == name)
                .map_or(0, |(_, keep)| *keep);
            if keep > values.len() {
                return Err(failure(
                    "defer source binding no longer has its original lexical stack",
                ));
            }
            if values.len() > keep {
                self.pay(name.len().saturating_add(values.len() - keep), depth)?;
                hidden.push((name.clone(), values.len() - keep));
                state.temporaries.extend(values.drain(keep..));
            }
        }
        state.names.retain(|_, values| !values.is_empty());
        Ok((hidden, start))
    }
    fn restore_defer(
        &mut self,
        state: &mut State,
        hidden: &[(String, usize)],
        start: usize,
        depth: usize,
    ) -> R<()> {
        self.pay(
            hidden.iter().fold(hidden.len(), |n, (name, count)| {
                n.saturating_add(name.len()).saturating_add(*count)
            }),
            depth,
        )?;
        let mut values = state
            .temporaries
            .drain(start..)
            .collect::<Vec<_>>()
            .into_iter();
        for (name, count) in hidden {
            let target = state.names.entry(name.clone()).or_default();
            for _ in 0..*count {
                target.push(values.next().ok_or_else(|| {
                    failure("defer lost a hidden original binding responsibility")
                })?);
            }
        }
        if values.next().is_some() {
            return Err(failure(
                "defer restoration changes the pending temporary inventory",
            ));
        }
        Ok(())
    }
    fn cleanup(
        &mut self,
        owner: SemanticDefinitionId,
        state: State,
        defers: &[(&SemanticOriginalBlock, Vec<(String, usize)>)],
        scope: &[(String, usize)],
        depth: usize,
    ) -> R<Vec<State>> {
        self.pay(state.roots.len(), depth)?;
        if !state.reachable || state.roots.values().any(|phase| *phase == 0) {
            return Ok(Vec::new());
        }
        let control = state.control;
        let mut states = vec![state];
        for (cleanup, visible) in defers.iter().rev() {
            let mut next = Vec::new();
            for mut state in states {
                let (hidden, start) = self.hide_for_defer(&mut state, visible, depth + 1)?;
                state.control = Control::Next;
                for mut state in self.block(owner, cleanup, state, depth + 1)? {
                    self.restore_defer(&mut state, &hidden, start, depth + 1)?;
                    if state.control != Control::Next {
                        return Err(failure(
                            "original protocol cleanup escapes its lexical exit",
                        ));
                    }
                    self.pay(1, depth)?;
                    next.push(state);
                }
            }
            states = next;
        }
        for state in &mut states {
            state.control = control;
            self.leave(state, scope, depth)?;
        }
        Ok(states)
    }
    fn block(
        &mut self,
        owner: SemanticDefinitionId,
        body: &SemanticOriginalBlock,
        state: State,
        depth: usize,
    ) -> R<Vec<State>> {
        self.pay(
            body.statements
                .len()
                .saturating_add(state.roots.len())
                .saturating_add(1),
            depth,
        )?;
        if !state.reachable || state.roots.values().any(|phase| *phase == 0) {
            return Ok(Vec::new());
        }
        let scope = self.scope(&state, depth)?;
        let mut active = vec![state];
        let mut ended = Vec::new();
        let mut defers = Vec::new();
        for statement in &body.statements {
            if active.is_empty() {
                break;
            }
            if let CoreStatementKind::Defer(cleanup) = &statement.kind {
                self.pay(1, depth)?;
                let visible = self.scope(
                    active.first().ok_or_else(|| {
                        failure("defer registration has no reaching original state")
                    })?,
                    depth + 1,
                )?;
                defers.push((cleanup, visible));
                continue;
            }
            let mut next = Vec::new();
            for state in active {
                for mut state in self.statement(owner, statement, state, depth + 1).map_err(
                    |mut diagnostic| {
                        diagnostic.notes.push(format!(
                            "original statement owner {:?}, node {:?}, span {:?}, operation {:?}",
                            owner,
                            statement.id,
                            statement.span,
                            std::mem::discriminant(&statement.kind)
                        ));
                        diagnostic
                    },
                )? {
                    self.pay(1, depth)?;
                    for exit in std::mem::take(&mut state.abrupt) {
                        ended.extend(self.cleanup(owner, exit, &defers, &scope, depth + 1)?);
                    }
                    if !state.reachable {
                        continue;
                    }
                    if state.control == Control::Next {
                        next.push(state);
                    } else {
                        ended.extend(self.cleanup(owner, state, &defers, &scope, depth + 1)?);
                    }
                }
            }
            active = next;
        }
        for state in active {
            ended.extend(self.cleanup(owner, state, &defers, &scope, depth + 1)?);
        }
        Ok(ended)
    }
    fn statement(
        &mut self,
        owner: SemanticDefinitionId,
        statement: &SemanticOriginalStatement,
        mut state: State,
        depth: usize,
    ) -> R<Vec<State>> {
        self.pay(state.roots.len().saturating_add(1), depth)?;
        if !state.reachable || state.roots.values().any(|phase| *phase == 0) {
            return Ok(Vec::new());
        }
        match &statement.kind {
            CoreStatementKind::Binding {
                name, initializer, ..
            } => {
                let value = if let Some(expr) = initializer {
                    self.expr(owner, expr, &mut state, depth + 1)?
                } else {
                    Value::Plain
                };
                let value = if initializer.is_some() {
                    let ty=self.query_type(owner,Query::Binding(statement.id),depth+1)?.ok_or_else(||failure("original cleanup initialized binding has no final solved Gamma type"))?;
                    self.typed(owner, &ty, value, depth + 1)?
                } else {
                    value
                };
                self.pay(name.len().saturating_add(1), depth)?;
                state.names.entry(name.clone()).or_default().push(value);
            }
            CoreStatementKind::TupleBinding {
                pattern,
                initializer,
                ..
            } => {
                let value = self.expr(owner, initializer, &mut state, depth + 1)?;
                self.pattern(pattern, value, &mut state, depth + 1)?;
            }
            CoreStatementKind::Return(value) => {
                state.returned = if let Some(expr) = value {
                    self.expr(owner, expr, &mut state, depth + 1)?
                } else {
                    Value::Plain
                };
                state.control = Control::Return;
            }
            CoreStatementKind::Expression(expr) => {
                let value = self.expr(owner, expr, &mut state, depth + 1)?;
                self.drop_value(&value, &state, depth + 1)?;
            }
            CoreStatementKind::Assign { target, value }
            | CoreStatementKind::CompoundAssign { target, value, .. } => {
                let value = self.expr(owner, value, &mut state, depth + 1)?;
                let inserted = self.value_copy(&value, depth + 1)?;
                if let Some(old) = self.replace_place(target, &mut state, inserted, depth + 1)? {
                    self.drop_value(&old, &state, depth + 1)?;
                } else {
                    let ty=self.query_type(owner,Query::Expression(target.id),depth+1)?.ok_or_else(||pending("original cleanup external store lacks its independently solved place type"))?;
                    let old = self.typed(owner, &ty, Value::Plain, depth + 1)?;
                    self.drop_value(&old, &state, depth + 1)?;
                    let target = self.observe(owner, target, &mut state, depth + 1)?;
                    let mut roots = BTreeSet::new();
                    target.roots(&mut roots);
                    value.roots(&mut roots);
                    if !roots.is_empty() {
                        return Err(pending(
                            "original cleanup external store lacks its exact protocol owner projection",
                        ));
                    }
                }
            }
            CoreStatementKind::Block(body) | CoreStatementKind::Unsafe(body) => {
                return self.block(owner, body, state, depth + 1);
            }
            CoreStatementKind::If {
                condition,
                then_body,
                else_body,
            } => {
                let condition = self.expr(owner, condition, &mut state, depth + 1)?;
                let mut right = self.copy(&state, depth)?;
                self.condition(&condition, true, &mut state, depth)?;
                self.condition(&condition, false, &mut right, depth)?;
                let mut out = self.block(owner, then_body, state, depth + 1)?;
                if let Some(body) = else_body {
                    out.extend(self.block(owner, body, right, depth + 1)?);
                } else {
                    out.push(right);
                }
                return Ok(out);
            }
            CoreStatementKind::IfLet {
                pattern,
                value,
                then_body,
                else_body,
            } => {
                let value = self.expr(owner, value, &mut state, depth + 1)?;
                let scope = self.scope(&state, depth)?;
                let mut out = Vec::new();
                for branch in self.selected(owner, pattern, &value, &state, depth + 1)? {
                    for mut branch in self.block(owner, then_body, branch, depth + 1)? {
                        self.leave(&mut branch, &scope, depth)?;
                        out.push(branch);
                    }
                }
                let mut pending = vec![self.value_copy(&value, depth + 1)?];
                while let Some(value) = pending.pop() {
                    if let Value::Choice(values) = value {
                        self.pay(values.len(), depth)?;
                        pending.extend(values);
                        continue;
                    }
                    if self.matches(pattern, &value, depth + 1)? == Some(true) {
                        continue;
                    }
                    let branch = self.copy(&state, depth + 1)?;
                    self.drop_value(&value, &branch, depth + 1)?;
                    if let Some(body) = else_body {
                        out.extend(self.block(owner, body, branch, depth + 1)?);
                    } else {
                        out.push(branch);
                    }
                }
                return Ok(out);
            }
            CoreStatementKind::Match { value, arms } => {
                let value = self.expr(owner, value, &mut state, depth + 1)?;
                let scope = self.scope(&state, depth)?;
                self.pay(arms.len(), depth)?;
                let mut out = Vec::new();
                for arm in arms {
                    for branch in self.selected(owner, &arm.pattern, &value, &state, depth + 1)? {
                        for mut branch in self.block(owner, &arm.body, branch, depth + 1)? {
                            self.leave(&mut branch, &scope, depth)?;
                            out.push(branch);
                        }
                    }
                }
                return Ok(out);
            }
            CoreStatementKind::Loop(body) => {
                return self.loop_body(owner, body, None, None, false, state, depth + 1);
            }
            CoreStatementKind::While { condition, body } => {
                return self.loop_body(owner, body, Some(condition), None, false, state, depth + 1);
            }
            CoreStatementKind::WhileLet {
                pattern,
                value,
                body,
            } => {
                return self.loop_body(
                    owner,
                    body,
                    Some(value),
                    Some(pattern),
                    false,
                    state,
                    depth + 1,
                );
            }
            CoreStatementKind::ForRange {
                name,
                start,
                end,
                body,
            } => {
                self.expr(owner, start, &mut state, depth + 1)?;
                self.expr(owner, end, &mut state, depth + 1)?;
                let scope = self.scope(&state, depth + 1)?;
                self.pay(name.len().saturating_add(1), depth)?;
                state
                    .names
                    .entry(name.clone())
                    .or_default()
                    .push(Value::Plain);
                let mut out = self.loop_body(owner, body, None, None, true, state, depth + 1)?;
                for state in &mut out {
                    self.leave(state, &scope, depth + 1)?;
                }
                return Ok(out);
            }
            CoreStatementKind::With { name, value, body } => {
                let value = self.expr(owner, value, &mut state, depth + 1)?;
                let scope = self.scope(&state, depth)?;
                self.pay(name.len().saturating_add(1), depth)?;
                state.names.entry(name.clone()).or_default().push(value);
                let mut out = self.block(owner, body, state, depth + 1)?;
                for state in &mut out {
                    self.leave(state, &scope, depth)?;
                }
                return Ok(out);
            }
            CoreStatementKind::Break => state.control = Control::Break,
            CoreStatementKind::Continue => state.control = Control::Continue,
            CoreStatementKind::CompileTime(_) => {}
            CoreStatementKind::ForEach {
                pattern,
                iterator,
                body,
            } => {
                let value = self.expr(owner, iterator, &mut state, depth + 1)?;
                self.drop_value(&value, &state, depth + 1)?;
                self.pay(1, depth)?;
                let slot = state.temporaries.len();
                state.temporaries.push(value);
                let mut out =
                    self.iteration(owner, statement.id, pattern, body, state, depth + 1)?;
                for state in &mut out {
                    let value = state.temporaries.remove(slot);
                    self.drop_value(&value, state, depth + 1)?;
                }
                return Ok(out);
            }
            CoreStatementKind::Yield(_) | CoreStatementKind::Unsupported(_) => {
                return Err(pending(
                    "original protocol control form requires its complete CFG relation",
                ));
            }
            CoreStatementKind::Defer(_) => unreachable!(),
        }
        Ok(vec![state])
    }
    fn function(&mut self, owner: SemanticDefinitionId, depth: usize) -> R<()> {
        self.pay(
            self.proof
                .protocol_complete
                .len()
                .saturating_add(self.active.len())
                .saturating_add(1),
            depth,
        )?;
        if self.proof.protocol_complete.contains(&owner)
            && self.proof.cleanup_complete.contains(&owner)
        {
            return Ok(());
        }
        if !self.proof.complete.contains(&owner) {
            return Err(failure(
                "protocol definition lacks a complete independent original type proof",
            ));
        }
        if !self.active.insert(owner) {
            return Err(pending(
                "recursive protocol contracts require a jointly proved conditional fixed point",
            ));
        }
        let definition = self.definition(owner)?;
        let SemanticDefinitionBody::FunctionWithBody {
            protocol_contracts,
            parameters,
            body,
            foreign,
            ..
        } = &definition.body
        else {
            self.active.remove(&owner);
            self.proof.protocol_complete.insert(owner);
            self.proof.cleanup_complete.insert(owner);
            return Ok(());
        };
        self.pay(
            parameters.len().saturating_add(protocol_contracts.len()),
            depth,
        )?;
        if let Some(declaration) = foreign {
            self.pay(
                declaration
                    .abi
                    .len()
                    .saturating_add(declaration.symbol.len())
                    .saturating_add(declaration.noescape_parameters.len())
                    .saturating_add(1),
                depth,
            )?;
            if !matches!(body, SemanticOriginalFunctionBody::External)
                || declaration.abi != "C"
                || declaration.symbol.is_empty()
            {
                return Err(failure(
                    "original foreign head is not its bodyless, concrete C declaration",
                ));
            }
            let mut previous = None;
            for parameter in &declaration.noescape_parameters {
                if *parameter as usize >= parameters.len()
                    || previous.is_some_and(|p| p >= *parameter)
                {
                    return Err(failure(
                        "original noescape promise does not name distinct ordered real parameters",
                    ));
                }
                previous = Some(*parameter);
            }
        } else if matches!(body, SemanticOriginalFunctionBody::External) {
            return Err(failure(
                "original external body has no independently produced ABI declaration",
            ));
        }
        let mut roles = BTreeSet::new();
        for contract in protocol_contracts {
            crate::registration::charge_contract(contract, |work| self.pay(work, depth))?;
            let parameter = contract.parameter as usize;
            if parameter >= parameters.len()
                || !roles.insert(contract.parameter)
                || contract.entry.is_empty()
            {
                return Err(failure(
                    "original protocol table has an absent, duplicate, or empty parameter role",
                ));
            }
            if !self.query_async(owner, Query::Parameter(contract.parameter), depth + 1)? {
                return Err(failure(
                    "original protocol role is not a genuine AsyncRegistration parameter",
                ));
            }
            let mut entry = BTreeSet::new();
            for input in &contract.entry {
                if !entry.insert(*input) {
                    return Err(failure("original protocol entry state is duplicated"));
                }
            }
            let mut exits = BTreeSet::new();
            for row in &contract.exits {
                if !entry.contains(&row.input)
                    || !exits.insert((row.input, row.exit))
                    || row.output.is_empty()
                {
                    return Err(failure(
                        "original protocol relation has an invalid input, duplicate exit, or empty output",
                    ));
                }
                let mut output = BTreeSet::new();
                for phase in &row.output {
                    if !output.insert(*phase) {
                        return Err(failure("original protocol output state is duplicated"));
                    }
                }
            }
        }
        let SemanticOriginalFunctionBody::Defined(body) = body else {
            if !protocol_contracts.is_empty() {
                return Err(pending(
                    "bodyless original protocol relation has no independent implementation proof",
                ));
            }
            self.active.remove(&owner);
            self.proof.protocol_complete.insert(owner);
            self.proof.cleanup_complete.insert(owner);
            return Ok(());
        };
        let mut cases = vec![BTreeMap::new()];
        for contract in protocol_contracts {
            let mut next = Vec::new();
            for case in cases {
                for input in &contract.entry {
                    self.pay(case.len().saturating_add(1), depth)?;
                    let mut case = case.clone();
                    case.insert(contract.parameter, *input);
                    next.push(case);
                }
            }
            cases = next;
        }
        for inputs in cases {
            let mut state = State {
                reachable: true,
                abrupt: Vec::new(),
                names: BTreeMap::new(),
                roots: BTreeMap::new(),
                control: Control::Next,
                returned: Value::Plain,
                temporaries: Vec::new(),
            };
            let mut capture_inputs = Vec::new();
            let captured_mode = self.proof.kinds.get(&owner).copied();
            if definition.kind == SemanticDefinitionKind::Closure {
                let (parent, creator) = self.closure_creator(definition, depth + 1)?;
                self.pay(
                    self.proof
                        .capture_names
                        .len()
                        .saturating_add(self.proof.closure_move.len()),
                    depth,
                )?;
                let moved =
                    *self.proof.closure_move.get(&owner).ok_or_else(|| {
                        failure("original closure cleanup lacks its creator mode")
                    })?;
                let names = self.proof.capture_names.get(&owner).ok_or_else(|| {
                    failure("original closure cleanup lacks its physical capture directory")
                })?;
                self.pay(
                    names
                        .iter()
                        .fold(names.len(), |work, name| work.saturating_add(name.len())),
                    depth,
                )?;
                let names = self
                    .proof
                    .capture_names
                    .get(&owner)
                    .ok_or_else(|| failure("original closure capture directory disappeared"))?
                    .clone();
                for (index, name) in names.into_iter().enumerate() {
                    let ty = self.capture_type(parent, creator, &name, definition, depth + 1)?;
                    let mut value = if self.async_head(parent, &ty, depth + 1)? {
                        let root = Root::Capture(owner, index as u32);
                        state.roots.insert(root, UNKNOWN);
                        Value::Root(
                            root,
                            true,
                            moved && self.ownership(parent, &ty, depth + 1)?.unwrap_or(false),
                            None,
                        )
                    } else {
                        self.typed(parent, &ty, Value::Plain, depth + 1)?
                    };
                    if !moved {
                        value = self.view(value, depth + 1)?;
                    }
                    self.pay(name.len().saturating_add(1), depth)?;
                    capture_inputs.push((name.clone(), self.value_copy(&value, depth + 1)?));
                    state.names.insert(name, vec![value]);
                }
            }
            for (index, parameter) in parameters.iter().enumerate() {
                self.pay(parameter.name.len().saturating_add(1), depth)?;
                let value = if self.query_async(owner, Query::Parameter(index as u32), depth + 1)? {
                    let root = Root::Parameter(owner, index as u32);
                    state.roots.insert(
                        root,
                        inputs
                            .get(&(index as u32))
                            .copied()
                            .map(bit)
                            .unwrap_or(UNKNOWN),
                    );
                    let owning = self
                        .query_ownership(owner, Query::Parameter(index as u32), depth + 1)?
                        .unwrap_or(false);
                    Value::Root(root, true, owning, None)
                } else {
                    Value::Plain
                };
                let ty = self
                    .query_type(owner, Query::Parameter(index as u32), depth + 1)?
                    .ok_or_else(|| {
                        failure("original cleanup parameter has no independent solved Gamma type")
                    })?;
                let value = self.typed(owner, &ty, value, depth + 1)?;
                state
                    .names
                    .entry(parameter.name.clone())
                    .or_default()
                    .push(value);
            }
            for mut state in self.block(owner, body, state, depth + 1)? {
                if matches!(state.control, Control::Break | Control::Continue) {
                    return Err(failure(
                        "original protocol loop control escaped its function",
                    ));
                }
                if matches!(captured_mode, Some(CallableKind::Fn | CallableKind::FnMut)) {
                    for (name, expected) in &capture_inputs {
                        self.pay(name.len().saturating_add(state.names.len()), depth)?;
                        let actual = state
                            .names
                            .get(name)
                            .and_then(|stack| stack.first())
                            .ok_or_else(|| {
                                failure("original reusable closure loses its captured binding")
                            })?;
                        if !self.present(expected, actual, depth + 1)? {
                            return Err(failure(
                                "original reusable closure exits without refilling its captured owning responsibility",
                            ));
                        }
                        let stack = state.names.get_mut(name).ok_or_else(|| {
                            failure("original closure handoff lost its captured binding")
                        })?;
                        self.pay(stack.len(), depth)?;
                        stack.remove(0);
                    }
                    state.names.retain(|_, values| !values.is_empty());
                }
                for (name, values) in &state.names {
                    for value in values {
                        self.pay(name.len().saturating_add(1), depth)?;
                        self.drop_value(value, &state, depth + 1).map_err(|mut diagnostic| {
                            diagnostic.notes.push(format!("original function exit owner {:?}, binding {:?}, body span {:?}", owner, name, body.span));
                            diagnostic
                        })?;
                    }
                }
                self.output(owner, &state, protocol_contracts, &inputs, depth + 1)?;
            }
        }
        self.active.remove(&owner);
        self.proof.protocol_complete.insert(owner);
        self.proof.cleanup_complete.insert(owner);
        Ok(())
    }
}

pub(super) fn check(
    program: &SemanticModule,
    proof: &mut Proof,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    // An unused foreign call still needs its real original unsafe boundary.
    // The actual marker/ABI is checked again by the actual world below.
    for ((caller, site), target) in &proof.call_targets {
        if proof.protocol_complete.contains(caller) {
            continue;
        }
        *left = left
            .checked_sub(
                program
                    .definitions
                    .len()
                    .saturating_mul(2)
                    .saturating_add(1),
            )
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "original foreign edge work exceeded", None)
            })?;
        let target = program
            .definitions
            .iter()
            .find(|d| d.id == *target)
            .ok_or_else(|| failure("original foreign edge target is absent"))?;
        if matches!(
            &target.body,
            SemanticDefinitionBody::FunctionWithBody {
                foreign: Some(_),
                ..
            }
        ) {
            let original = program
                .definitions
                .iter()
                .find(|d| d.id == *caller)
                .ok_or_else(|| failure("original foreign edge caller is absent"))?;
            if !unsafe_call(original, *site, limits, left)? {
                return Err(failure(
                    "original foreign call is outside its genuine unsafe region",
                ));
            }
        }
    }
    let mut engine = Engine {
        program,
        proof,
        limits,
        left,
        active: BTreeSet::new(),
        relevant: BTreeSet::new(),
        categories: Vec::new(),
        cleanup: Vec::new(),
    };
    engine.pay(
        program
            .definitions
            .len()
            .saturating_add(engine.proof.intrinsics.len()),
        0,
    )?;
    for definition in &program.definitions {
        if matches!(&definition.body,SemanticDefinitionBody::FunctionWithBody{protocol_contracts,..}if !protocol_contracts.is_empty())
        {
            engine.relevant.insert(definition.id);
        }
    }
    for ((owner, _), intrinsic) in &engine.proof.intrinsics {
        if matches!(
            intrinsic.kind,
            IntrinsicKind::RegistrationStart {
                asynchronous: true,
                ..
            } | IntrinsicKind::RegistrationOperation(
                RegistrationOp::AsyncValid
                    | RegistrationOp::AsyncRequest
                    | RegistrationOp::AsyncPollClose
                    | RegistrationOp::AsyncFinish
                    | RegistrationOp::AsyncFinishChecked
            )
        ) {
            engine.relevant.insert(*owner);
        }
    }
    loop {
        engine.pay(
            engine
                .proof
                .call_targets
                .len()
                .saturating_add(engine.relevant.len()),
            0,
        )?;
        let mut changed = false;
        for ((caller, site), callee) in &engine.proof.call_targets {
            if !engine.proof.compile_time_calls.contains(&(*caller, *site))
                && engine.relevant.contains(callee)
            {
                changed |= engine.relevant.insert(*caller);
            }
        }
        if !changed {
            break;
        }
    }
    for definition in &program.definitions {
        if matches!(
            definition.body,
            SemanticDefinitionBody::FunctionWithBody { .. }
        ) {
            engine
                .function(definition.id, 0)
                .map_err(|mut diagnostic| {
                    diagnostic
                        .notes
                        .push(format!("original checked function {:?}", definition.id));
                    diagnostic
                })?;
        }
    }
    Ok(())
}

/// Read-only original AST authority for a specific original Call site. An
/// actual unsafe marker, symbol spelling, or same-typed call is never input.
pub(in crate::semantics) fn unsafe_call(
    original: &SemanticDefinition,
    site: SemanticSiteId,
    limits: &Limits,
    left: &mut usize,
) -> R<bool> {
    let SemanticDefinitionBody::FunctionWithBody {
        site_nodes,
        body: SemanticOriginalFunctionBody::Defined(body),
        ..
    } = &original.body
    else {
        return Ok(false);
    };
    *left = left
        .checked_sub(site_nodes.len().saturating_add(1))
        .ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "original unsafe site work exceeded", None)
        })?;
    let mut selected = None;
    for row in site_nodes {
        if row.site == site {
            let SemanticOriginalNode::Expression(node) = row.node else {
                return Err(failure("foreign call source is not an original expression"));
            };
            if selected.replace(node).is_some() {
                return Err(failure(
                    "foreign call site has multiple original source nodes",
                ));
            }
        }
    }
    let target =
        selected.ok_or_else(|| failure("foreign call has no original source occurrence"))?;
    crate::dyn_metadata::semantic_operand_snapshot(body, limits, left, 1)?;
    let mut body = body.clone();
    let mut scan = UnsafeSource {
        target,
        scan: true,
        count: 0,
        inside: false,
        limits,
        left,
    };
    super::super::original_walk::block(&mut body, &mut scan, 0)?;
    if scan.count != 1 {
        return Err(failure(
            "original foreign source expression is absent or duplicated",
        ));
    }
    Ok(scan.inside)
}
struct UnsafeSource<'a> {
    target: NodeId,
    scan: bool,
    count: usize,
    inside: bool,
    limits: &'a Limits,
    left: &'a mut usize,
}
impl super::super::original_walk::Context for UnsafeSource<'_> {
    fn node(&mut self, depth: usize) -> R<()> {
        *self.left = self.left.checked_sub(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "original unsafe traversal work exceeded",
                None,
            )
        })?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original unsafe traversal depth exceeded",
                None,
            ));
        }
        Ok(())
    }
    fn ty(&mut self, _: &mut SemanticTypeRecipe, depth: usize) -> R<()> {
        self.node(depth)
    }
    fn descend_closure(&self) -> bool {
        false
    }
    fn expression(&mut self, id: NodeId, _: &SemanticOriginalExprKind, depth: usize) -> R<()> {
        self.node(depth)?;
        if id == self.target {
            self.count = self
                .count
                .checked_add(1)
                .ok_or_else(|| failure("original unsafe occurrence count overflows"))?;
            if !self.scan {
                self.inside = true;
            }
        }
        Ok(())
    }
    fn statement(
        &mut self,
        _: NodeId,
        kind: &SemanticOriginalStatementKind,
        depth: usize,
    ) -> R<()> {
        self.node(depth)?;
        if self.scan {
            if let CoreStatementKind::Unsafe(body) = kind {
                crate::dyn_metadata::semantic_operand_snapshot(
                    body,
                    self.limits,
                    self.left,
                    depth + 1,
                )?;
                let mut body = body.clone();
                let mut nested = UnsafeSource {
                    target: self.target,
                    scan: false,
                    count: 0,
                    inside: false,
                    limits: self.limits,
                    left: self.left,
                };
                super::super::original_walk::block(&mut body, &mut nested, depth + 1)?;
                self.inside |= nested.inside;
            }
        }
        Ok(())
    }
}
