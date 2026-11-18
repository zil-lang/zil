//! Callable obligations in the original declaration domain. Invocation
//! equations are local to a role and never update the global type/life graph.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ReceiverSource { Expression(SemanticDefinitionId, NodeId), Binding(SemanticDefinitionId, NodeId) }


pub(super) struct RoleFrame {
    pub(super) owner: SemanticDefinitionId,
    pub(super) lifetimes: Vec<Term>,
    pub(super) implicits: BTreeMap<u32, Term>,
    pub(super) receiver: Option<(Term, usize)>,
}

/// This is the access window of the callable subject in this signature only.
/// Parameter elision, Fixed captures and Static results keep their own roles.
pub(in crate::semantic) fn receiver_result(signature: &SemanticSignatureRecipe, kind: CallableKind) -> bool {
    matches!(kind, CallableKind::Fn | CallableKind::FnMut)
        && signature.call_kind == CallKind::Direct
        && matches!(&signature.result, SemanticTypeRecipe::Address {
            mutable: false,
            lifetime: SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::OuterOccurrence {
                operand: SemanticRecipeOperand::Receiver, path, mode: StoredBorrowMode::SharedAddress,
            }), ..
        } if path.is_empty())
}

pub(super) struct BoundFrame {
    pub(super) owner: SemanticDefinitionId,
    pub(super) lifetimes: Vec<Term>,
}

pub(super) struct DeclaredRole<'a> {
    pub(super) ty: &'a SemanticTypeRecipe,
    pub(super) kind: CallableKind,
    pub(super) signature: &'a SemanticSignatureRecipe,
    pub(super) bounds: Vec<u32>,
}

/// Preserve the nonempty de Bruijn frames surrounding each actual premise.
/// Zero-quantifier syntax remains in the original inventory but adds no frame.
pub(super) fn declared_roles<'a>(
    requirements: &'a [SemanticRequirementRecipe],
    bounds: &mut Vec<u32>,
    output: &mut Vec<DeclaredRole<'a>>,
    b: &mut Budget,
    depth: usize,
) -> CompileResult<()> {
    if depth > b.partial_depth {
        return Err(unknown(
            "original callable premises exceed their finite depth",
        ));
    }
    for requirement in requirements {
        b.spend(1, WORK)?;
        match requirement {
            SemanticRequirementRecipe::ForAll {
                lifetimes,
                requirements,
            } => {
                if *lifetimes != 0 {
                    Solver::push(bounds, *lifetimes, b)?;
                }
                let result = declared_roles(requirements, bounds, output, b, depth + 1);
                if *lifetimes != 0 {
                    bounds.pop();
                }
                result?;
            }
            SemanticRequirementRecipe::Callable {
                ty,
                kind,
                signature,
            } => {
                b.spend(bounds.len(), WORK)?;
                Solver::push(
                    output,
                    DeclaredRole {
                        ty,
                        kind: *kind,
                        signature,
                        bounds: bounds.clone(),
                    },
                    b,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

impl Solver<'_, '_> {
    pub(super) fn push_bound_frame(
        &mut self,
        owner: SemanticDefinitionId,
        count: u32,
        invocation: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if count == 0 {
            return Ok(());
        }
        let serial = self.next_role;
        self.next_role = serial
            .checked_add(1)
            .ok_or_else(|| resource("original quantified role namespace overflow"))?;
        let mut lifetimes = Vec::new();
        for index in 0..count {
            let life = self.term(
                if invocation {
                    Kind::LifeVariable(None)
                } else {
                    Kind::Life(Life::Role(owner, serial, index))
                },
                b,
            )?;
            Self::push(&mut lifetimes, life, b)?;
        }
        Self::push(&mut self.bound_frames, BoundFrame { owner, lifetimes }, b)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn role_need(
        &mut self,
        frame: &Frame,
        term: Term,
        kind: CallableKind,
        signature: &SemanticSignatureRecipe,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let checkpoint = self.bound_frames.len();
        self.push_bound_frame(frame.definition, signature.bound_lifetimes, false, b)?;
        let serial = self.next_role;
        self.next_role = serial
            .checked_add(1)
            .ok_or_else(|| resource("original role namespace overflow"))?;
        let mut lifetimes = Vec::new();
        b.spend(signature.invocation_lifetimes as usize, WORK)?;
        for index in 0..signature.invocation_lifetimes {
            let life = self.term(Kind::Life(Life::Role(frame.definition, serial, index)), b)?;
            Self::push(&mut lifetimes, life, b)?;
        }
        // A distinct position beyond the signature's Invocation vector.
        // Only the exact root Receiver recipe can read it through this frame.
        let receiver = if receiver_result(signature, kind) {
            Some(self.term(Kind::Life(Life::Role(frame.definition, serial,
                signature.invocation_lifetimes)), b)?)
        } else { None };
        Self::push(
            &mut self.role_frames,
            RoleFrame {
                owner: frame.definition,
                lifetimes,
                implicits: BTreeMap::new(),
                receiver: None,
            },
            b,
        )?;
        let result: CompileResult<CallableNeed> = (|| {
            let mut parameters = Vec::new();
            let mut modes = Vec::new();
            for parameter in &signature.parameters {
                if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy) {
                    return Err(unknown(
                        "original callable role needs its complete resource passing relation",
                    ));
                }
                let ty = self.recipe(frame, &parameter.ty, b, depth + 1)?;
                if parameter.mode == PassingMode::Copy {
                    Self::push(
                        &mut self.needs,
                        Need {
                            term: ty,
                            predicate: Predicate::Copy,
                        },
                        b,
                    )?;
                }
                Self::push(&mut parameters, ty, b)?;
                Self::push(&mut modes, parameter.mode, b)?;
            }
            if let Some(role) = self.role_frames.last_mut() {
                role.receiver = receiver.map(|term| (term, depth + 2));
            }
            let result = self.recipe(frame, &signature.result, b, depth + 1)?;
            Ok(CallableNeed {
                // Signature instantiations also use a callee-named Frame.
                // Only an active original body owns declaration constraints;
                // a caller's pending actuals must keep their later check.
                declaration: self.active.contains(&frame.definition).then_some(frame.definition),
                term,
                kind,
                parameters,
                result,
                call_kind: signature.call_kind,
                parameter_modes: modes,
                result_mode: signature.result_mode,
                receiver,
            })
        })();
        self.role_frames
            .pop()
            .ok_or_else(|| fail("original callable role lost its lexical frame"))?;
        self.bound_frames.truncate(checkpoint);
        let need = result?;
        Self::push(&mut self.callable_needs, need, b)
    }

    pub(super) fn role_premises_wf(
        &mut self,
        frame: &Frame,
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.role_premises_wf_at(frame, requirements, b, 0)
    }
    fn role_premises_wf_at(
        &mut self,
        frame: &Frame,
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        for requirement in requirements {
            b.spend(1, WORK)?;
            if let SemanticRequirementRecipe::ForAll {
                lifetimes,
                requirements,
            } = requirement
            {
                let checkpoint = self.bound_frames.len();
                self.push_bound_frame(frame.definition, *lifetimes, false, b)?;
                let result = self.role_premises_wf_at(frame, requirements, b, depth + 1);
                self.bound_frames.truncate(checkpoint);
                result?;
                continue;
            }
            if let SemanticRequirementRecipe::Callable {
                kind, signature, ..
            } = requirement
            {
                let before = self.callable_needs.len();
                self.role_need(frame, frame.result, *kind, signature, b, 0)?;
                if self.callable_needs.len() != before + 1 {
                    return Err(fail(
                        "original callable premise emitted competing role obligations",
                    ));
                }
                self.callable_needs
                    .pop()
                    .ok_or_else(|| fail("original callable premise lost its signature"))?;
            }
        }
        Ok(())
    }

    // Preserve a published opaque FnOnce operation contract before revealing
    // its concrete body. A Direct entry does not overwrite a Suspend operation
    // bound, nor does the caller's own ABI manufacture such a bound.
    pub(super) fn known_call_operation(&self, term: Term,
        definition: SemanticDefinitionId, b: &mut Budget) -> CompileResult<(bool, CallKind)> {
        let mut term = self.root(term, b)?;
        for _ in 0..=b.partial_depth {
            b.spend(1, WORK)?;
            let Kind::CallResult { caller, node, hidden } = self.state.arena[term] else {
                let SemanticDefinitionBody::FunctionWithBody { signature, .. } =
                    &self.inventory.definition(definition, b)?.body else {
                        return Err(fail("known callable has no complete original declaration"));
                    };
                return Ok((false, signature.call_kind));
            };
            b.lookup(self.state.calls.len(), WORK)?;
            let call = self.state.calls.get(&(caller, node))
                .ok_or_else(|| fail("opaque callable operation has no exact original factory source"))?;
            let factory = self.inventory.definition(call.target, b)?;
            let mut roles = Vec::new();
            declared_roles(&factory.requirements, &mut Vec::new(), &mut roles, b, 0)?;
            let mut upper = None;
            for role in roles {
                if role.kind != CallableKind::FnOnce
                    || !matches!(role.ty, SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result }) {
                    continue;
                }
                // All requirements were independently checked in the original
                // factory domain; their execution uppers intersect here.
                if upper.is_none() || role.signature.call_kind == CallKind::Direct {
                    upper = Some(role.signature.call_kind);
                }
            }
            if let Some(upper) = upper { return Ok((true, upper)); }
            term = self.root(hidden, b)?;
        }
        Err(resource("known callable operation provenance exceeds its finite depth"))
    }

    fn parameter_lifetime(
        &self,
        owner: SemanticDefinitionId,
        recipe: &SemanticTypeRecipe,
    ) -> Option<Life> {
        let SemanticTypeRecipe::Address {
            lifetime: SemanticOuterLifeRecipe::Class(life),
            ..
        } = recipe
        else {
            return None;
        };
        match life {
            SemanticLifeRecipe::Implicit {
                scope: SemanticScopeRef::Ancestor(0),
                ordinal,
            } => Some(Life::Implicit(owner, *ordinal)),
            SemanticLifeRecipe::Invocation(index) => Some(Life::Invocation(owner, *index)),
            _ => None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn role_match(
        &mut self,
        expected: Term,
        actual: Term,
        input_lives: &BTreeSet<Life>,
        forward: &mut BTreeMap<Life, Life>,
        backward: &mut BTreeMap<Life, Life>,
        outer: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        let expected = self.root(expected, b)?;
        let actual = self.root(actual, b)?;
        if expected == actual {
            return Ok(());
        }
        b.spend(2, WORK)?;
        let expected_address = match self.state.arena[expected] {
            Kind::Address {
                mutable,
                life,
                inner,
            } => Some((mutable, life, inner)),
            _ => None,
        };
        let actual_address = match self.state.arena[actual] {
            Kind::Address {
                mutable,
                life,
                inner,
            } => Some((mutable, life, inner)),
            _ => None,
        };
        if let Some((mutable, life, inner)) = expected_address {
            let Some((actual_mutable, actual_life, actual_inner)) = actual_address else {
                return Err(unknown(
                    "original callable reference needs a grounded original parameter occurrence",
                ));
            };
            if mutable != actual_mutable {
                return Err(fail("original callable role changes its reference mode"));
            }
            let actual_term = self.root(actual_life, b)?;
            let actual_life = match self.state.arena[actual_term] {
                Kind::Life(life) => life,
                _ => return Err(unknown("actual original callable lifetime is unresolved")),
            };
            let expected_term = self.root(life, b)?;
            b.lookup(self.call_variables.len(), WORK)?;
            if outer && matches!(self.state.arena[expected_term], Kind::LifeVariable(None))
                && matches!(self.call_variables.get(&expected_term), Some(call_regions::Role::Life(_)))
                && matches!(actual_life, Life::Formal(..))
            {
                // A call's omitted named actual can be grounded by this
                // complete callable's fixed input signature.
                self.unify(expected_term, actual_term, depth + 1, b)?;
            }
            let expected_life = match self.state.arena[self.root(expected_term, b)?] {
                Kind::Life(life) => life,
                _ => return Err(unknown("original callable role has an unresolved lifetime")),
            };
            if expected_life != actual_life {
                if !outer || !input_lives.contains(&actual_life) {
                    return Err(unknown(
                        "original callable role cannot rename a retained or inner lifetime",
                    ));
                }
                b.spend(forward.len().saturating_add(backward.len() + 1), WORK)?;
                if forward
                    .get(&expected_life)
                    .is_some_and(|old| *old != actual_life)
                    || backward
                        .get(&actual_life)
                        .is_some_and(|old| *old != expected_life)
                {
                    return Err(fail(
                        "original callable role changes an independent input/result lifetime relation",
                    ));
                }
                forward.insert(expected_life, actual_life);
                backward.insert(actual_life, expected_life);
            }
            // Invariant inner types and intrinsic nominal N are compared in
            // the original graph; the local role equation never reaches them.
            return self.role_match(
                inner,
                actual_inner,
                input_lives,
                forward,
                backward,
                false,
                b,
                depth + 1,
            );
        }
        if let (Kind::Slice(expected), Kind::Slice(actual)) =
            (&self.state.arena[expected], &self.state.arena[actual])
        {
            let (expected, actual) = (*expected, *actual);
            return self.unify(expected, actual, depth + 1, b);
        }
        if matches!(self.state.arena[expected], Kind::Slice(_))
            || matches!(self.state.arena[actual], Kind::Slice(_))
        {
            return Err(fail(
                "original callable role changes its Slice element constructor",
            ));
        }
        self.unify(expected, actual, depth + 1, b)
    }

    pub(super) fn check_roles(&mut self, b: &mut Budget) -> CompileResult<()> {
        let mut pending = std::mem::take(&mut self.callable_needs);
        while !pending.is_empty() {
            for need in pending {
                let term = self.type_root(need.term, b)?;
                if self.rigid_role(term, &need, b)? {
                    continue;
                }
                if let Kind::Code {
                    kind: SemanticCodeKind::FunctionPointer,
                    parameters,
                    result,
                    modes,
                    result_mode,
                    call_kind,
                } = &self.state.arena[term]
                {
                    if need.receiver.is_some()
                        || *call_kind != need.call_kind
                        || *result_mode != need.result_mode
                        || *modes != need.parameter_modes
                        || parameters.len() != need.parameters.len()
                    {
                        return Err(fail(
                            "original code value changes its requested callable signature",
                        ));
                    }
                    b.spend(parameters.len().saturating_add(modes.len() + 1), WORK)?;
                    let parameters = parameters.clone();
                    let result = *result;
                    let mut forward = BTreeMap::new();
                    let mut backward = BTreeMap::new();
                    for (actual, expected) in parameters.into_iter().zip(&need.parameters) {
                        self.role_match(
                            *expected,
                            actual,
                            &BTreeSet::new(),
                            &mut forward,
                            &mut backward,
                            true,
                            b,
                            0,
                        )?;
                    }
                    self.role_match(
                        need.result,
                        result,
                        &BTreeSet::new(),
                        &mut forward,
                        &mut backward,
                        true,
                        b,
                        0,
                    )?;
                    continue;
                }
                b.spend(
                    match &self.state.arena[term] {
                        Kind::Closure {
                            parameters,
                            environment,
                            ..
                        } => parameters.len().saturating_add(environment.len() + 1),
                        _ => 1,
                    },
                    WORK,
                )?;
                let Kind::Closure {
                    definition,
                    parameters,
                    result,
                    ..
                } = self.state.arena[term].clone()
                else {
                    return Err(unknown(
                        "original abstract callable needs its complete declared premise resolution",
                    ));
                };
                let declaration = self.inventory.definition(definition, b)?;
                if need.kind == CallableKind::FnOnce {
                    let result = self.type_root(result, b)?;
                    if let Kind::Address { life, .. } = self.state.arena[result]
                        && matches!(self.state.arena[self.root(life, b)?],
                            Kind::Life(Life::ClosureReceiver(owner)) if owner == definition) {
                        return Err(fail("consuming callable cannot expose a receiver-bounded result"));
                    }
                }
                self.capture_role(definition, need.kind, need.call_kind, b)?;
                let SemanticDefinitionBody::FunctionWithBody {
                    signature,
                    parameters: originals,
                    ..
                } = &declaration.body
                else {
                    return Err(fail(
                        "original callable omits its complete declared signature",
                    ));
                };
                if !callable_entry_matches(need.kind, signature.call_kind, need.call_kind)
                    || signature.result_mode != need.result_mode
                    || parameters.len() != need.parameters.len()
                    || originals.len() != parameters.len()
                    || signature.parameters.len() != parameters.len()
                    || (need.kind == CallableKind::Fn
                        && self.mutating_closures.contains(&definition))
                    || need.kind == CallableKind::FunctionPointer
                {
                    return Err(fail(
                        "original value does not satisfy its declared callable mode, ABI or arity",
                    ));
                }
                let mut input_lives = BTreeSet::new();
                for (index, original) in originals.iter().enumerate() {
                    b.spend(1, WORK)?;
                    if signature.parameters[index].mode != need.parameter_modes[index] {
                        return Err(unknown(
                            "original callable input modes need their complete adaptation proof",
                        ));
                    }
                    let life = original
                        .ty
                        .as_ref()
                        .and_then(|ty| self.parameter_lifetime(definition, ty))
                        .or_else(|| {
                            self.parameter_lifetime(definition, &signature.parameters[index].ty)
                        });
                    if let Some(life) = life {
                        b.lookup(input_lives.len(), WORK)?;
                        input_lives.insert(life);
                    }
                }
                let mut forward = BTreeMap::new();
                let mut backward = BTreeMap::new();
                for (actual, expected) in parameters.into_iter().zip(need.parameters) {
                    self.role_match(
                        expected,
                        actual,
                        &input_lives,
                        &mut forward,
                        &mut backward,
                        true,
                        b,
                        0,
                    )?;
                }
                if need.receiver.is_some() {
                    // The actual source keeps its own sigma/Fixed identity.
                    // This proves an upper bound from every genuine captured
                    // return, rather than equating it with the role window.
                    self.state.receiver_sources(definition, self.inventory, b)?
                        .ok_or_else(|| fail("callable Receiver result has no complete captured return proof"))?;
                    if self.static_byte_role(result, need.result, b, 0)? { continue; }
                    let expected = self.type_root(need.result, b)?;
                    let actual = self.type_root(result, b)?;
                    let (Kind::Address { mutable: false, inner: wanted, .. },
                         Kind::Address { mutable: false, inner: returned, .. }) =
                        (&self.state.arena[expected], &self.state.arena[actual]) else {
                        return Err(fail("callable Receiver result changes its shared outer constructor"));
                    };
                    self.unify(*wanted, *returned, 0, b)?;
                } else {
                    self.role_match(need.result, result, &input_lives,
                        &mut forward, &mut backward, true, b, 0)?;
                }
            }
            pending = std::mem::take(&mut self.callable_needs);
        }
        Ok(())
    }

    fn rigid_role(
        &mut self,
        term: Term,
        need: &CallableNeed,
        b: &mut Budget,
    ) -> CompileResult<bool> {
        let Kind::Rigid(owner, ordinal) = self.state.arena[term] else {
            return Ok(false);
        };
        let declaration = self.inventory.definition(owner, b)?;
        let Some(SemanticFormal::Type { requirements }) =
            declaration.parameters.get(ordinal as usize)
        else {
            return Err(fail("original callable premise changes its formal kind"));
        };
        let frame = self.state.frames.remove(&owner).ok_or_else(|| {
            unknown("original callable premise needs its complete declaration frame")
        })?;
        let checked = (|| {
            let mut selected = None;
            let mut roles = Vec::new();
            declared_roles(requirements, &mut Vec::new(), &mut roles, b, 0)?;
            declared_roles(&declaration.requirements, &mut Vec::new(), &mut roles, b, 0)?;
            for role in &roles {
                let (ty, kind, signature) = (role.ty, role.kind, role.signature);
                let supports = kind == need.kind
                    || (kind == CallableKind::Fn
                        && matches!(need.kind, CallableKind::FnMut | CallableKind::FnOnce))
                    || (kind == CallableKind::FnMut && need.kind == CallableKind::FnOnce);
                if !supports
                    || (signature.call_kind != need.call_kind
                        // A declared FnOnce Direct premise already covers the
                        // complete operation. A reusable Fn/FnMut premise only
                        // describes its entry and cannot supply this theorem.
                        && !(kind == CallableKind::FnOnce && need.kind == CallableKind::FnOnce
                            && signature.call_kind == CallKind::Direct && need.call_kind == CallKind::Suspend))
                    || signature.parameters.len() != need.parameters.len()
                    || signature.result_mode != need.result_mode
                {
                    continue;
                }
                let subject = self.recipe(&frame, ty, b, 0)?;
                if !self.same_original(subject, term, b, 0)? {
                    continue;
                }
                if signature
                    .parameters
                    .iter()
                    .zip(&need.parameter_modes)
                    .any(|(parameter, mode)| parameter.mode != *mode)
                {
                    continue;
                }
                if selected.replace(role).is_some() {
                    return Err(unknown(
                        "multiple original callable premises require an exact overload selection",
                    ));
                }
            }
            let Some(role) = selected else {
                return Err(unknown(
                    "original rigid callable lacks its complete declared role premise",
                ));
            };
            let before = self.callable_needs.len();
            let checkpoint = self.bound_frames.len();
            for count in &role.bounds {
                self.push_bound_frame(owner, *count, false, b)?;
            }
            let result = self.role_need(&frame, term, need.kind, role.signature, b, 0);
            self.bound_frames.truncate(checkpoint);
            result?;
            if self.callable_needs.len() != before + 1 {
                return Err(fail(
                    "original callable premise creates competing role obligations",
                ));
            }
            let available = self
                .callable_needs
                .pop()
                .ok_or_else(|| fail("original callable premise loses its scoped role"))?;
            let mut input_lives = BTreeSet::new();
            for parameter in &available.parameters {
                if let Kind::Address { life, .. } =
                    &self.state.arena[self.type_root(*parameter, b)?]
                {
                    if let Kind::Life(life @ Life::Role(..)) =
                        self.state.arena[self.root(*life, b)?]
                    {
                        b.lookup(input_lives.len(), WORK)?;
                        input_lives.insert(life);
                    }
                }
            }
            if let Some(receiver) = available.receiver {
                if need.receiver.is_none() { return Err(fail("callable Receiver premise changes its result source role")); }
                let Kind::Life(life) = self.state.arena[self.root(receiver, b)?] else {
                    return Err(fail("callable Receiver premise loses its scoped life"));
                };
                b.lookup(input_lives.len(), WORK)?;
                input_lives.insert(life);
            } else if need.receiver.is_some() {
                return Err(fail("callable premise supplies no signature-local Receiver window"));
            }
            let mut forward = BTreeMap::new();
            let mut backward = BTreeMap::new();
            for (actual, expected) in available.parameters.into_iter().zip(&need.parameters) {
                self.role_match(
                    *expected,
                    actual,
                    &input_lives,
                    &mut forward,
                    &mut backward,
                    true,
                    b,
                    0,
                )?;
            }
            self.role_match(
                need.result,
                available.result,
                &input_lives,
                &mut forward,
                &mut backward,
                true,
                b,
                0,
            )?;
            Ok(true)
        })();
        b.lookup(self.state.frames.len(), WORK)?;
        self.state.frames.insert(owner, frame);
        checked
    }
}

impl Checked {
    /// Re-read every original return against its independently checked borrow
    /// or captured-address read. A header, environment type or candidate class
    /// is never a source premise for this theorem.
    pub(in crate::semantic) fn receiver_sources(
        &self, owner: SemanticDefinitionId, inventory: &Inventory<'_>, b: &mut Budget,
    ) -> CompileResult<Option<BTreeSet<u32>>> {
        let declaration = inventory.definition(owner, b)?;
        if declaration.kind != SemanticDefinitionKind::Closure { return Ok(None); }
        let SemanticDefinitionBody::FunctionWithBody {
            body: SemanticOriginalFunctionBody::Defined(body), signature, ..
        } = &declaration.body else { return Ok(None); };
        if signature.call_kind != CallKind::Direct { return Ok(None); }
        b.lookup(self.frames.len(), WORK)?;
        let frame = self.frames.get(&owner)
            .ok_or_else(|| fail("Receiver source has no independently completed original body"))?;
        let result = self.receiver_value_root(frame.result, b)?;
        if !matches!(&self.arena[result], Kind::Address { mutable: false, .. })
            && !matches!(&self.arena[result], Kind::Atom(ty) if strings::is_byte_type(ty)) {
            return Ok(None);
        }
        let directory = inventory.original_index(owner, b)?;
        let returns = directory.active_returns(b)?;
        let mut sources = BTreeSet::new();
        if returns.is_empty() {
            // A genuine expression body must already have produced this exact
            // checked result term. A discarded final expression/fallthrough
            // cannot acquire a return by being scanned as syntax here.
            let Some(last) = body.statements.last() else { return Ok(None); };
            let CoreStatementKind::Expression(value) = &last.kind else { return Ok(None); };
            if directory.erased(last.id, b)? || directory.erased(value.id, b)? { return Ok(None); }
            b.lookup(frame.expressions.len(), WORK)?;
            let Some(term) = frame.expressions.get(&value.id) else { return Ok(None); };
            if self.root(*term, b)? != self.root(frame.result, b)?
                || !self.receiver_expression_source(owner, value.id, inventory, &mut sources, b)? {
                return Ok(None);
            }
        } else {
            for value in returns {
                b.spend(1, WORK)?;
                let Some(value) = value else { return Ok(None); };
                if !self.receiver_expression_source(owner, value.id, inventory, &mut sources, b)? { return Ok(None); }
            }
        }
        // A literal-only result has no capture root. Its separate genuine
        // Static source proof is sufficient; zero captures alone prove nothing.
        Ok(Some(sources))
    }

    pub(super) fn receiver_value_root(&self, term: Term, b: &mut Budget) -> CompileResult<Term> {
        let mut root = self.root(term, b)?;
        for _ in 0..=b.partial_depth {
            b.spend(1, WORK)?;
            let Kind::CallResult { hidden, .. } = self.arena[root] else { return Ok(root); };
            root = self.root(hidden, b)?;
        }
        Err(resource("Receiver source value exceeds its finite call ancestry"))
    }

    /// Follow the already checked original source directory. Syntax only
    /// transports projections/aliases; actual Borrow, call-input and sealed
    /// manager receipts establish roots. Equal lifetime labels never select a
    /// captured owner, and helper spellings carry no authority.
    fn receiver_expression_source(
        &self, owner: SemanticDefinitionId, node: NodeId, inventory: &Inventory<'_>,
        sources: &mut BTreeSet<u32>, b: &mut Budget,
    ) -> CompileResult<bool> {
        let root = ReceiverSource::Expression(owner, node);
        let mut pending = vec![root];
        let mut graph: BTreeMap<ReceiverSource, Vec<ReceiverSource>> = BTreeMap::new();
        let mut users: BTreeMap<ReceiverSource, Vec<ReceiverSource>> = BTreeMap::new();
        let mut grounded = BTreeSet::new();
        while let Some(query) = pending.pop() {
            b.lookup(graph.len(), WORK)?;
            if graph.contains_key(&query) { continue; }
            if graph.len() >= b.helper_nodes { return Err(resource("Receiver source directory exceeds finite node budget")); }
            let mut edges = Vec::new();
            let mut seed = false;
            match query {
                ReceiverSource::Binding(owner, binding) => {
                    let directory = inventory.original_index(owner, b)?;
                    let statement = directory.get_stmt(binding, b)?;
                    let CoreStatementKind::Binding { initializer: Some(initializer), .. } = &statement.kind
                        else { return Ok(false); };
                    Solver::push(&mut edges, ReceiverSource::Expression(owner, initializer.id), b)?;
                    for ((scope, _), write) in &self.cleanup_writes {
                        b.spend(write.path.len().saturating_add(1), WORK)?;
                        if *scope == owner && write.anchor == LocalAnchor::Binding(binding) {
                            if !write.path.is_empty() { return Ok(false); }
                            Solver::push(&mut edges, ReceiverSource::Expression(owner, write.value), b)?;
                        }
                    }
                }
                ReceiverSource::Expression(owner, node) => {
                    b.lookup(self.string_literals.len(), WORK)?;
                    if self.string_literals.contains_key(&(owner, node)) { seed = true; }
                    else {
                        let directory = inventory.original_index(owner, b)?;
                        let value = directory.get_expr(node, b)?;
                        b.lookup(self.borrows.len(), WORK)?;
                        if let Some(source) = self.borrows.get(&(owner, node)) {
                            if let Some(LocalAnchor::Environment(ordinal)) = source.anchor {
                                b.spend(source.path.len().saturating_add(1), WORK)?;
                                b.lookup(sources.len(), WORK)?; sources.insert(ordinal); seed = true;
                            } else {
                                if !source.indirect && !source.slice.is_some_and(|slice| slice.address) { return Ok(false); }
                                if let Some(expression) = source.expression {
                                    Solver::push(&mut edges, ReceiverSource::Expression(owner, expression), b)?;
                                } else if let Some(LocalAnchor::Binding(binding)) = source.anchor {
                                    Solver::push(&mut edges, ReceiverSource::Binding(owner, binding), b)?;
                                } else { return Ok(false); }
                            }
                        } else {
                            b.lookup(self.environment_reads.len(), WORK)?;
                            if let Some(ordinal) = self.environment_reads.get(&(owner, node)) {
                                b.lookup(sources.len(), WORK)?; sources.insert(*ordinal); seed = true;
                            } else {
                                b.lookup(self.intrinsics.len().saturating_add(self.intrinsic_anchors.len()), WORK)?;
                                if self.intrinsics.get(&(owner, node)) == Some(&sealed::Intrinsic::BytesView { mutable: false }) {
                                    let Some(LocalAnchor::Environment(ordinal)) = self.intrinsic_anchors.get(&(owner, node))
                                        else { return Ok(false); };
                                    b.lookup(sources.len(), WORK)?; sources.insert(*ordinal); seed = true;
                                } else {
                                    match &value.kind {
                                        CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. }
                                        | CoreExprKind::Sequence { value: inner, .. }
                                        | CoreExprKind::Member { object: inner, .. }
                                        | CoreExprKind::Index { object: inner, .. }
                                        | CoreExprKind::Unary { op: UnaryOp::Dereference, operand: inner } =>
                                            Solver::push(&mut edges, ReceiverSource::Expression(owner, inner.id), b)?,
                                        CoreExprKind::If { then_value, else_value, .. } => {
                                            Solver::push(&mut edges, ReceiverSource::Expression(owner, then_value.id), b)?;
                                            Solver::push(&mut edges, ReceiverSource::Expression(owner, else_value.id), b)?;
                                        }
                                        CoreExprKind::Name(_) => {
                                            b.lookup(self.local_reads.len(), WORK)?;
                                            let Some(LocalAnchor::Binding(binding)) = self.local_reads.get(&(owner, node))
                                                else { return Ok(false); };
                                            Solver::push(&mut edges, ReceiverSource::Binding(owner, *binding), b)?;
                                        }
                                        _ => {
                                            b.lookup(self.frames.len(), WORK)?;
                                            let frame = self.frames.get(&owner).ok_or_else(|| fail("Receiver helper source frame absent"))?;
                                            b.lookup(frame.expressions.len(), WORK)?;
                                            let Some(term) = frame.expressions.get(&node) else { return Ok(false); };
                                            let root = self.receiver_value_root(*term, b)?;
                                            let Kind::Address { mutable: false, life, .. } = self.arena[root] else { return Ok(false); };
                                            let life = self.root(life, b)?;
                                            b.lookup(self.call_uses.len(), WORK)?;
                                            if let Some(uses) = self.call_uses.get(&(owner, node)) {
                                                for input in uses {
                                                    b.spend(1, WORK)?;
                                                    if self.root(input.target, b)? == life {
                                                        Solver::push(&mut edges, ReceiverSource::Expression(owner, input.source.node), b)?;
                                                    }
                                                }
                                            }
                                            if edges.is_empty() {
                                                if !matches!(self.arena[life], Kind::Life(Life::Receiver(scope, call)) if scope == owner && call == node)
                                                    { return Ok(false); }
                                                let CoreExprKind::Call { callee, .. } = &value.kind else { return Ok(false); };
                                                Solver::push(&mut edges, ReceiverSource::Expression(owner, callee.id), b)?;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            for edge in &edges {
                b.lookup(users.len(), WORK)?;
                Solver::push(users.entry(*edge).or_default(), query, b)?;
                Solver::push(&mut pending, *edge, b)?;
            }
            if seed { b.lookup(grounded.len(), WORK)?; grounded.insert(query); }
            b.lookup(graph.len(), WORK)?;
            graph.insert(query, edges);
        }
        // A finite alias/loop component is admissible only when every source
        // dependency is initialized from a genuine capture or Static producer.
        // All writes were expanded before this closure, including writes in an
        // otherwise grounded cycle; cycles alone never establish a source.
        let mut work = Vec::new();
        for query in &grounded { Solver::push(&mut work, *query, b)?; }
        while let Some(query) = work.pop() {
            b.lookup(users.len(), WORK)?;
            if let Some(parents) = users.get(&query) {
                for parent in parents {
                    b.lookup(grounded.len(), WORK)?;
                    if grounded.insert(*parent) { Solver::push(&mut work, *parent, b)?; }
                }
            }
        }
        for query in graph.keys() {
            b.lookup(grounded.len(), WORK)?;
            if !grounded.contains(query) { return Ok(false); }
        }
        Ok(true)
    }

}

impl Solver<'_, '_> {
    pub(super) fn inferred_receiver_body(
        &self, owner: SemanticDefinitionId, b: &mut Budget,
    ) -> CompileResult<bool> {
        let declaration = self.inventory.definition(owner, b)?;
        Ok(declaration.kind == SemanticDefinitionKind::Closure
            && matches!(&declaration.body, SemanticDefinitionBody::FunctionWithBody { signature, .. }
                if signature.call_kind == CallKind::Direct
                    && matches!(signature.result, SemanticTypeRecipe::OriginalType {
                        scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result })))
    }

    pub(super) fn inferred_receiver_return(
        &mut self, frame: &mut Frame, actual: Term, b: &mut Budget, depth: usize,
    ) -> CompileResult<bool> {
        if !self.inferred_receiver_body(frame.definition, b)? { return Ok(false); }
        let actual = self.type_root(actual, b)?;
        let Kind::Address { mutable: false, inner, .. } = self.state.arena[actual] else { return Ok(false); };
        let expected = self.type_root(frame.result, b)?;
        match self.state.arena[expected] {
            Kind::Infer(None) => {
                let life = self.term(Kind::Life(Life::ClosureReceiver(frame.definition)), b)?;
                let result = self.term(Kind::Address { mutable: false, life, inner }, b)?;
                self.unify(frame.result, result, depth + 1, b)?;
            }
            Kind::Address { mutable: false, life, inner: expected_inner }
                if matches!(self.state.arena[self.root(life, b)?], Kind::Life(Life::ClosureReceiver(owner)) if owner == frame.definition) => {
                self.unify(inner, expected_inner, depth + 1, b)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn finish_inferred_receiver(
        &mut self, frame: &mut Frame, b: &mut Budget,
    ) -> CompileResult<()> {
        let result = self.type_root(frame.result, b)?;
        let Kind::Address { life, .. } = self.state.arena[result] else { return Ok(()); };
        if !matches!(self.state.arena[self.root(life, b)?], Kind::Life(Life::ClosureReceiver(owner)) if owner == frame.definition) { return Ok(()); }
        let directory = self.inventory.original_index(frame.definition, b)?;
        let returns = directory.active_returns(b)?;
        if returns.is_empty() { return Err(fail("inferred closure Receiver has no complete actual return inventory")); }
        let mut common = None;
        let mut same = true;
        for value in returns {
            let value = value.ok_or_else(|| fail("shared closure result has a Unit return"))?;
            b.lookup(frame.expressions.len(), WORK)?;
            let term = frame.expressions.get(&value.id).copied()
                .ok_or_else(|| fail("inferred closure return has no checked original expression"))?;
            let term = self.type_root(term, b)?;
            let Kind::Address { mutable: false, life, .. } = self.state.arena[term] else {
                return Err(fail("inferred closure changes its complete shared result mode"));
            };
            if let Some((_, first)) = common {
                same &= self.same_original(first, life, b, 0)?;
            } else { common = Some((term, life)); }
        }
        let (common_term, common_life) = common.ok_or_else(|| fail("closure return inventory is empty"))?;
        let local_window = matches!(self.state.arena[self.root(common_life, b)?],
            Kind::Life(Life::Implicit(owner, _)) | Kind::Life(Life::Receiver(owner, _))
                | Kind::Life(Life::CallResult(owner, _, _, _)) if owner == frame.definition);
        if same && !local_window {
            // A genuinely common Fixed/input source stays intact. A body-local
            // borrow/call occurrence is not such an external strong source.
            frame.result = common_term;
        } else {
            self.snapshot(frame, b)?;
            self.capture_role(frame.definition,
                if self.mutating_closures.contains(&frame.definition) { CallableKind::FnMut } else { CallableKind::Fn },
                CallKind::Direct, b)?;
            self.state.receiver_sources(frame.definition, self.inventory, b)?
                .ok_or_else(|| fail("closure return alternatives have no independent common Receiver upper"))?;
        }
        Ok(())
    }

    pub(super) fn known_receiver_output(
        &mut self, owner: SemanticDefinitionId, node: NodeId, subject: Term,
        definition: SemanticDefinitionId, once: bool, result: Term, b: &mut Budget,
    ) -> CompileResult<Term> {
        let result_root = self.type_root(result, b)?;
        let Kind::Address { mutable: false, life, inner } = self.state.arena[result_root] else { return Ok(result); };
        if once { return Ok(result); }
        let mut source = self.root(subject, b)?;
        let mut required = false;
        for _ in 0..=b.partial_depth {
            b.spend(1, WORK)?;
            let Kind::CallResult { caller, node, hidden } = self.state.arena[source] else { break; };
            b.lookup(self.state.calls.len(), WORK)?;
            let call = self.state.calls.get(&(caller, node))
                .ok_or_else(|| fail("Receiver role has no checked original factory call"))?;
            let factory = self.inventory.definition(call.target, b)?;
            let mut roles = Vec::new();
            declared_roles(&factory.requirements, &mut Vec::new(), &mut roles, b, 0)?;
            b.spend(roles.len(), WORK)?;
            required |= roles.iter().any(|role| receiver_result(role.signature, role.kind)
                && matches!(role.ty, SemanticTypeRecipe::OriginalType {
                    scope: SemanticScopeRef::Ancestor(0), source: SemanticOriginalTypeSource::Result }));
            source = self.root(hidden, b)?;
        }
        if matches!(self.state.arena[source], Kind::CallResult { .. }) {
            return Err(resource("Receiver callable provenance exceeds its finite depth"));
        }
        let mut local_view = matches!(self.state.arena[self.root(life, b)?],
            Kind::Life(Life::Implicit(actual, _)) | Kind::Life(Life::ClosureReceiver(actual)) if actual == definition);
        // Invocation opening may have substituted the local occurrence. Its
        // completed declaration still identifies the original upper bound;
        // the resulting lifetime label cannot establish that fact itself.
        b.lookup(self.state.frames.len(), WORK)?;
        if let Some(frame) = self.state.frames.get(&definition) {
            let result = self.type_root(frame.result, b)?;
            if let Kind::Address { mutable: false, life, .. } = self.state.arena[result] {
                local_view |= matches!(self.state.arena[self.root(life, b)?],
                    Kind::Life(Life::Implicit(actual, _)) | Kind::Life(Life::ClosureReceiver(actual)) if actual == definition);
            }
        }
        if !required && !local_view { return Ok(result); }
        self.state.receiver_sources(definition, self.inventory, b)?
            .ok_or_else(|| fail("Receiver invocation lacks a complete original captured source proof"))?;
        let receiver = self.term(Kind::Life(Life::Receiver(owner, node)), b)?;
        // An upper-bound result is a new view of this call, while the entry's
        // own borrow occurrence and every retained inner class stay intact.
        self.term(Kind::Address { mutable: false, life: receiver, inner }, b)
    }
}
