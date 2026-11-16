//! A call through an abstract F keeps its original subject and source parameter
//! identity. Its declaration premise is not an invented function definition.
use super::*;

#[derive(Clone)]
pub(super) struct AbstractCall {
    parameter: u32,
    pub(super) subject: Term,
    receiver: Term,
    place: Option<Borrow>,
    pub(super) signature_owner: SemanticDefinitionId,
    pub(super) signature: SemanticSignatureRecipe,
    pub(super) bounds: Vec<u32>,
    protected: Vec<Term>,
    kind: CallableKind,
    parameters: Vec<Term>,
    result: Term,
}
pub(in crate::semantic) struct AbstractBoundary {
    pub(in crate::semantic) parameter: u32,
    pub(in crate::semantic) place: Option<borrow_places::Source>,
    pub(in crate::semantic) subject: crate::semantic::value::Value,
    pub(in crate::semantic) receiver: crate::semantic::value::Value,
    pub(in crate::semantic) signature_owner: SemanticDefinitionId,
    pub(in crate::semantic) signature: SemanticSignatureRecipe,
    pub(in crate::semantic) bounds: Vec<u32>,
    pub(in crate::semantic) kind: CallableKind,
    pub(in crate::semantic) parameters: Vec<crate::semantic::value::Value>,
    pub(in crate::semantic) result: crate::semantic::value::Value,
}
impl Solver<'_, '_> {
    pub(super) fn callable_premise(
        &self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        b: &mut Budget,
    ) -> CompileResult<(CallableKind, SemanticSignatureRecipe)> {
        let (kind, signature, bounds) = self.quantified_callable_premise(owner, ordinal, b)?;
        if !bounds.is_empty() {
            return Err(unknown(
                "sealed callable protocol requires its exact unquantified role",
            ));
        }
        Ok((kind, signature))
    }

    pub(super) fn quantified_callable_premise(
        &self,
        owner: SemanticDefinitionId,
        ordinal: u32,
        b: &mut Budget,
    ) -> CompileResult<(CallableKind, SemanticSignatureRecipe, Vec<u32>)> {
        let declaration = self.inventory.definition(owner, b)?;
        let Some(SemanticFormal::Type { requirements }) =
            declaration.parameters.get(ordinal as usize)
        else {
            return Err(fail(
                "abstract callable changes its original Type formal kind",
            ));
        };
        let mut selected: Option<&roles::DeclaredRole<'_>> = None;
        let mut declared = Vec::new();
        roles::declared_roles(requirements, &mut Vec::new(), &mut declared, b, 0)?;
        roles::declared_roles(
            &declaration.requirements,
            &mut Vec::new(),
            &mut declared,
            b,
            0,
        )?;
        for role in &declared {
            let SemanticTypeRecipe::Formal(reference) = role.ty else {
                continue;
            };
            let (kind, signature) = (role.kind, role.signature);
            if self.original_scope(owner, &reference.scope, b)? != owner
                || reference.ordinal != ordinal
            {
                continue;
            }
            if let Some(old) = selected {
                crate::dyn_metadata::recipe(old.signature, b)?;
                crate::dyn_metadata::recipe(signature, b)?;
                b.spend(old.bounds.len().saturating_add(role.bounds.len()), WORK)?;
                if old.kind != kind || old.signature != signature || old.bounds != role.bounds {
                    return Err(unknown(
                        "abstract callable has multiple roles requiring its complete result-role selection",
                    ));
                }
            } else {
                selected = Some(role);
            }
        }
        let role = selected.ok_or_else(|| {
            fail("abstract call lacks a declared callable premise for its actual subject")
        })?;
        crate::dyn_metadata::recipe(role.signature, b)?;
        b.spend(role.bounds.len(), WORK)?;
        Ok((role.kind, role.signature.clone(), role.bounds.clone()))
    }

    fn invocation_scheme(
        &mut self,
        context: &Frame,
        signature: &SemanticSignatureRecipe,
        bounds: &[u32],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(Vec<Term>, Term, Vec<Term>)> {
        self.step(depth, b)?;
        let checkpoint = self.bound_frames.len();
        for count in bounds {
            self.push_bound_frame(context.definition, *count, true, b)?;
        }
        self.push_bound_frame(context.definition, signature.bound_lifetimes, true, b)?;
        let mut protected = Vec::new();
        let mut bound_origins = Vec::new();
        for (depth, bound) in self.bound_frames[checkpoint..].iter().rev().enumerate() {
            for (index, life) in bound.lifetimes.iter().enumerate() {
                Self::push(&mut bound_origins, (*life, depth as u32, index as u32), b)?;
            }
        }
        for (life, depth, index) in bound_origins {
            self.mark_call_variable(life, call_regions::Role::Bound(depth, index), b)?;
        }
        for bound in &self.bound_frames[checkpoint..] {
            for life in &bound.lifetimes {
                Self::push(&mut protected, *life, b)?;
            }
        }
        let mut lifetimes = Vec::new();
        b.spend(signature.invocation_lifetimes as usize, WORK)?;
        for index in 0..signature.invocation_lifetimes {
            // These variables exist only for this real original invocation.
            // Solving them never mutates a rigid original or a retained class.
            let life = self.call_variable(call_regions::Role::Invocation(index), b)?;
            Self::push(&mut lifetimes, life, b)?;
        }
        let mut implicits = BTreeMap::new();
        for parameter in &signature.parameters {
            b.spend(1, WORK)?;
            let lifetime = match &parameter.ty {
                SemanticTypeRecipe::Address { lifetime, .. } | SemanticTypeRecipe::Desc { lifetime, .. } => lifetime,
                _ => continue,
            };
            if let SemanticOuterLifeRecipe::Class(SemanticLifeRecipe::Implicit { scope: SemanticScopeRef::Ancestor(0), ordinal }) = lifetime {
                b.lookup(implicits.len(), WORK)?;
                if !implicits.contains_key(ordinal) {
                    let life = self.call_variable(call_regions::Role::Implicit(*ordinal), b)?;
                    implicits.insert(*ordinal, life);
                }
            }
        }
        Self::push(
            &mut self.role_frames,
            roles::RoleFrame {
                owner: context.definition,
                lifetimes,
                implicits,
            },
            b,
        )?;
        let result: CompileResult<(Vec<Term>, Term)> = (|| {
            let mut parameters = Vec::new();
            for parameter in &signature.parameters {
                if !matches!(parameter.mode, PassingMode::Infer | PassingMode::Copy) {
                    return Err(unknown(
                        "abstract call needs its complete resource argument mode",
                    ));
                }
                let term = self.recipe(context, &parameter.ty, b, depth + 1)?;
                self.mark_call_parameter(term, &parameter.ty, b)?;
                Self::push(&mut parameters, term, b)?;
            }
            let result = self.recipe(context, &signature.result, b, depth + 1)?;
            Ok((parameters, result))
        })();
        self.role_frames
            .pop()
            .ok_or_else(|| fail("abstract invocation lost its own lifetime frame"))?;
        self.bound_frames.truncate(checkpoint);
        result.map(|(parameters, result)| (parameters, result, protected))
    }

    pub(super) fn finish_abstract_call(
        &mut self,
        owner: SemanticDefinitionId,
        node: NodeId,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.lookup(self.state.abstract_calls.len(), WORK)?;
        let Some(call) = self.state.abstract_calls.get(&(owner, node)) else {
            return Ok(());
        };
        let (result, receiver) = (call.result, call.receiver);
        if let Kind::Address { life, .. } = self.state.arena[self.root(result, b)?] {
            if matches!(
                self.state.arena[self.root(life, b)?],
                Kind::LifeVariable(None)
            ) {
                for protected in &call.protected {
                    if self.root(*protected, b)? == self.root(life, b)? {
                        return Err(fail(
                            "quantified callable result has no input instantiation for its protected lifetime",
                        ));
                    }
                }
                let Kind::Address { life: head, .. } = self.state.arena[self.root(receiver, b)?]
                else {
                    return Err(fail(
                        "short callable result has no actual original receiver head",
                    ));
                };
                // Only an otherwise-unbound outer result invocation receives
                // this virtual input. Inner/phantom classes are never grounded
                // by a head and retained Fixed identities are never retagged.
                self.unify(life, head, 0, b)?;
            }
        }
        Ok(())
    }

    pub(super) fn abstract_scheme(
        &mut self,
        frame: &mut Frame,
        callee: &SemanticOriginalExpr,
        call: NodeId,
        local: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<(Vec<Term>, Term)>> {
        self.step(depth, b)?;
        let root = self.type_root(local, b)?;
        if let Kind::Rigid(owner, ordinal) = self.state.arena[root] {
            let (kind, signature, bounds) = self.quantified_callable_premise(owner, ordinal, b)?;
            let place = self.borrow_place(frame, callee, b, depth + 1)?;
            match kind {
                CallableKind::FnOnce if place.indirect => {
                    return Err(fail("abstract FnOnce cannot move through a borrowed place"));
                }
                CallableKind::FnMut if !place.mutable => {
                    return Err(fail("abstract FnMut needs its original mutable place"));
                }
                CallableKind::FunctionPointer => return Ok(None),
                _ => {}
            }
            if kind == CallableKind::FnOnce {
                self.consume_place(frame, &place, b)?;
            }
            let context = self.premise_frame(frame, owner, b)?;
            let (parameters, result, protected) =
                self.invocation_scheme(&context, &signature, &bounds, b, depth + 1)?;
            b.spend(parameters.len().saturating_add(place.path.len() + 1), WORK)?;
            let saved = AbstractCall {
                parameter: 0,
                subject: root,
                receiver: local,
                place: Some(Borrow {
                    role: None,
                    anchor: place.anchor,
                    expression: place.expression,
                    index: place.index,
                    root: place.root,
                    path: place.path,
                    indirect: place.indirect,
                    inner: place.inner,
                    slice: None,
                }),
                signature_owner: owner,
                signature,
                bounds,
                protected,
                kind,
                parameters: parameters.clone(),
                result,
            };
            b.lookup(self.state.abstract_calls.len(), WORK)?;
            if self
                .state
                .abstract_calls
                .insert((frame.definition, call), saved)
                .is_some()
            {
                return Err(fail(
                    "abstract place call has competing original resolutions",
                ));
            }
            return Ok(Some((parameters, result)));
        }
        let Kind::Address { mutable, inner, .. } = self.state.arena[root] else {
            return Ok(None);
        };
        let subject = self.root(inner, b)?;
        let Kind::Rigid(owner, ordinal) = self.state.arena[subject] else {
            return Ok(None);
        };
        let CoreExprKind::Name(name) = &callee.kind else {
            return Err(unknown(
                "abstract receiver needs its complete original place anchor",
            ));
        };
        let mut position = None;
        for (index, local) in frame.locals.iter().enumerate().rev() {
            b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
            if local.name == *name {
                if !local.initialized {
                    return Err(fail(
                        "abstract receiver is not initialized in the original context",
                    ));
                }
                if local.used {
                    Self::push(
                        &mut self.needs,
                        Need {
                            term: local.ty,
                            predicate: Predicate::Copy,
                        },
                        b,
                    )?;
                }
                position = Some(index);
                break;
            }
        }
        let position =
            position.ok_or_else(|| fail("abstract receiver has no original lexical value"))?;
        // This borrowed receiver is observed by this invocation path, not
        // by expression() or borrow_place(). Keep its exact binder receipt
        // alongside the single expression registration below.
        self.record_environment_read(frame, position, callee.id, b)?;
        let parameter = position
            .checked_sub(frame.inherited)
            .filter(|position| *position < frame.parameters.len())
            .ok_or_else(|| {
                unknown("abstract receiver alias needs its exact original Binding anchor")
            })?;
        let parameter = u32::try_from(parameter)
            .map_err(|_| fail("abstract receiver parameter ordinal overflow"))?;
        let (kind, signature, bounds) = self.quantified_callable_premise(owner, ordinal, b)?;
        match kind {
            CallableKind::Fn => {}
            CallableKind::FnMut if mutable => {}
            CallableKind::FnMut => {
                return Err(fail(
                    "abstract FnMut invocation requires an exclusive original receiver",
                ));
            }
            CallableKind::FnOnce => {
                return Err(fail("abstract FnOnce cannot consume a borrowed receiver"));
            }
            CallableKind::FunctionPointer => {
                return Err(unknown(
                    "abstract code pointer needs its exact original pointer invocation",
                ));
            }
        }
        let (parameters, result, protected) = if owner == frame.definition {
            self.invocation_scheme(frame, &signature, &bounds, b, depth + 1)?
        } else {
            let parent =
                self.state.frames.remove(&owner).ok_or_else(|| {
                    fail("abstract callable premise has no checked lexical owner")
                })?;
            let answer = self.invocation_scheme(&parent, &signature, &bounds, b, depth + 1);
            b.lookup(self.state.frames.len(), WORK)?;
            self.state.frames.insert(owner, parent);
            answer?
        };
        b.spend(parameters.len().saturating_add(1), WORK)?;
        let saved = AbstractCall {
            parameter,
            subject,
            receiver: local,
            place: None,
            signature_owner: owner,
            signature,
            bounds,
            protected,
            kind,
            parameters: parameters.clone(),
            result,
        };
        b.lookup(self.state.abstract_calls.len(), WORK)?;
        if self
            .state
            .abstract_calls
            .insert((frame.definition, call), saved)
            .is_some()
        {
            return Err(fail("abstract call has competing original resolutions"));
        }
        b.lookup(frame.expressions.len(), WORK)?;
        if frame.expressions.insert(callee.id, local).is_some() {
            return Err(fail("abstract receiver expression identity is reused"));
        }
        Ok(Some((parameters, result)))
    }
}

impl Checked {
    pub(in crate::semantic) fn abstract_boundary(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<AbstractBoundary>> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail(
                "abstract actual call omits its complete original body",
            ));
        };
        let mut node = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site == site {
                let SemanticOriginalNode::Expression(id) = link.node else {
                    return Err(fail("abstract actual call selects an original statement"));
                };
                if node.replace(id).is_some() {
                    return Err(fail("abstract actual call has competing original nodes"));
                }
            }
        }
        let node = node.ok_or_else(|| fail("abstract actual call has no original node"))?;
        b.lookup(self.abstract_calls.len(), WORK)?;
        let Some(call) = self.abstract_calls.get(&(context.definition, node)) else {
            return Ok(None);
        };
        let mut parameters = Vec::new();
        for parameter in &call.parameters {
            let value = self.value(*parameter, context, program, inventory, b, 0)?;
            Solver::push(&mut parameters, value, b)?;
        }
        crate::dyn_metadata::recipe(&call.signature, b)?;
        b.spend(call.bounds.len(), WORK)?;
        Ok(Some(AbstractBoundary {
            parameter: call.parameter,
            place: call
                .place
                .as_ref()
                .map(|source| -> CompileResult<borrow_places::Source> {
                    b.spend(source.path.len(), WORK)?;
                    Ok(borrow_places::Source {
                        role: None,
                        anchor: source.anchor,
                        expression: source.expression,
                        index: source.index,
                        root: self.value(source.root, context, program, inventory, b, 0)?,
                        path: source.path.clone(),
                        inner: self.value(source.inner, context, program, inventory, b, 0)?,
                        indirect: source.indirect,
                        slice: source.slice,
                    })
                })
                .transpose()?,
            subject: self.value(call.subject, context, program, inventory, b, 0)?,
            receiver: self.value(call.receiver, context, program, inventory, b, 0)?,
            signature_owner: call.signature_owner,
            signature: call.signature.clone(),
            bounds: call.bounds.clone(),
            kind: call.kind,
            parameters,
            result: self.value(call.result, context, program, inventory, b, 0)?,
        }))
    }
}
