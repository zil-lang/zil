//! Immutable original callable-use modes select an emitted invocation. This
//! directory supplies no actual operand, signature, borrow or cleanup proof.
use super::*;

pub(super) struct Directory {
    modes: BTreeMap<(Origin, NodeId), Option<CallableKind>>,
    owners: BTreeMap<Origin, Option<SemanticDefinitionId>>,
    entries: BTreeMap<Origin, Option<CallableKind>>,
}
/// A constraint view of one real source closure body. It creates neither an
/// actual entry nor capture permission; later emission still binds both.
#[derive(Clone)]
pub(crate) struct CallableBodyModes {
    directory: std::rc::Rc<Directory>,
    origin: Origin,
    entry_kind: CallableKind,
}
impl CallableBodyModes {
    fn at(directory: std::rc::Rc<Directory>, origin: Origin, left: &mut usize, span: Span) -> R<Option<Self>> {
        pay(left, lookup_cost(directory.owners.len()), span)?;
        match directory.owners.get(&origin) {
            None => Ok(None),
            Some(None) => Err(invalid(span, "callable body has ambiguous original lexical ownership")),
            Some(Some(_)) => {
                pay(left, lookup_cost(directory.entries.len()), span)?;
                let kind = directory.entries.get(&origin).copied().flatten()
                    .ok_or_else(|| invalid(span, "callable body has no complete original entry kind"))?;
                Ok(Some(Self { directory, origin, entry_kind: kind }))
            },
        }
    }
    pub(crate) fn entry_kind(&self) -> CallableKind { self.entry_kind }
    pub(crate) fn nested(&self, expression: NodeId, left: &mut usize, span: Span) -> R<Option<Self>> {
        Self::at(self.directory.clone(), Origin { closure: Some(expression), ..self.origin }, left, span)
    }
    pub(crate) fn kind(&self, call: NodeId, left: &mut usize, span: Span) -> R<Option<CallableKind>> {
        pay(left, lookup_cost(self.directory.modes.len()), span)?;
        match self.directory.modes.get(&(self.origin, call)) {
            Some(None) => Err(invalid(span, "one exact callable use changes its original invocation mode")),
            Some(Some(kind)) => Ok(Some(*kind)),
            None => Ok(None),
        }
    }
}

fn index(
    program: &SemanticModule,
    proof: &super::super::original::Proof,
    limits: &Limits,
    left: &mut usize,
    span: Span,
) -> R<Directory> {
    pay(left, program.instances.len(), span)?;
    let mut instances = BTreeMap::new();
    let mut owners = BTreeMap::new();
    let mut entries = BTreeMap::new();
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
        if instances.insert(row.id, (origin, row.definition)).is_some() {
            return Err(invalid(span, "duplicate original callable instance identity"));
        }
        pay(left, lookup_cost(proof.kinds.len()).saturating_add(lookup_cost(proof.complete.len())).saturating_add(lookup_cost(entries.len())).saturating_add(2), span)?;
        let kind = proof.complete.contains(&row.definition)
            .then(|| proof.kinds.get(&row.definition).copied()).flatten();
        let entry = entries.entry(origin).or_insert(kind);
        if *entry != kind { *entry = None; }
        let owner = owners.entry(origin).or_insert(Some(row.definition));
        if *owner != Some(row.definition) { *owner = None; }
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
        if !seen.insert((site.instance, expression)) {
            return Err(invalid(span, "duplicate original callable use occurrence"));
        }
        let mode = modes.entry((origin, expression)).or_insert(Some(call.kind));
        if *mode != Some(call.kind) { *mode = None; }
    }
    Ok(Directory { modes, owners, entries })
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
        let Some(directory) = self.callable_mode_directory(limits, left, span)? else { return Ok(None); };
        let Some(body) = CallableBodyModes::at(directory, frame.origin, left, span)? else { return Ok(None); };
        body.kind(call, left, span)
    }
    fn callable_mode_directory(&self, limits: &Limits, left: &mut usize, span: Span) -> R<Option<std::rc::Rc<Directory>>> {
        let program = self.original.context().map(|context| &context.semantic)
            .or_else(|| self.frozen.as_deref().map(|frozen| &frozen.semantic));
        let Some(program) = program else { return Ok(None); };
        let missing = self.original_callable_modes.borrow().is_none();
        if missing {
            let view = self.original.borrow();
            let proof = view.as_ref().or_else(|| self.frozen.as_deref().map(|frozen| &frozen.original))
                .ok_or_else(|| invalid(span, "callable mode query has no immutable original proof"))?;
            let directory = index(program, proof, limits, left, span)?;
            *self.original_callable_modes.borrow_mut() = Some(std::rc::Rc::new(directory));
        }
        Ok(self.original_callable_modes.borrow().clone())
    }
    pub(crate) fn original_closure_modes(&self, creator: FunctionId, expression: NodeId, limits: &Limits, left: &mut usize, span: Span) -> R<Option<CallableBodyModes>> {
        pay(left, lookup_cost(self.frames.len()), span)?;
        let Some(parent) = self.frames.get(&creator) else { return Ok(None); };
        if parent.stack.last() != Some(&expression) {
            return Err(invalid(span, "closure constraint modes require their actual active constructor"));
        }
        let Some(directory) = self.callable_mode_directory(limits, left, span)? else { return Ok(None); };
        CallableBodyModes::at(directory, Origin {
            module: parent.origin.module, creator: parent.origin.creator, closure: Some(expression),
        }, left, span)
    }
}
