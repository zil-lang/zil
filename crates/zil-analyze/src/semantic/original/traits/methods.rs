//! Resolve original member invocations from their exact receiver premises.
//! Opening a signature creates only call-local role variables, never a global
//! equality between unrelated formal lives or a concrete callback capability.
use super::*;

pub(in crate::semantic) struct TraitCall {
    pub(in crate::semantic) declaration: LifeDeclarationId,
    pub(in crate::semantic) member: u32,
    pub(in crate::semantic) method: SemanticDefinitionId,
    pub(in crate::semantic) receiver: Term,
    pub(in crate::semantic) receiver_anchor: Option<LocalAnchor>,
    pub(in crate::semantic) dynamic: Option<bool>,
    pub(in crate::semantic) arguments: Vec<Term>,
    pub(in crate::semantic) method_actuals: Vec<Term>,
    pub(in crate::semantic) parameters: Vec<Term>,
    pub(in crate::semantic) result: Term,
    pub(in crate::semantic) call_kind: CallKind,
}

pub(in crate::semantic) struct TraitBoundary {
    pub(in crate::semantic) declaration: LifeDeclarationId,
    pub(in crate::semantic) member: u32,
    pub(in crate::semantic) method: SemanticDefinitionId,
    pub(in crate::semantic) receiver: crate::semantic::value::Value,
    pub(in crate::semantic) receiver_anchor: Option<LocalAnchor>,
    pub(in crate::semantic) dynamic: Option<bool>,
    pub(in crate::semantic) arguments: Vec<crate::semantic::value::Argument>,
    pub(in crate::semantic) method_actuals: Vec<crate::semantic::value::Argument>,
    pub(in crate::semantic) parameters: Vec<crate::semantic::value::Value>,
    pub(in crate::semantic) result: crate::semantic::value::Value,
    pub(in crate::semantic) call_kind: CallKind,
}

impl Checked {
    pub(in crate::semantic) fn trait_boundary(
        &self,
        context: &SemanticInstance,
        site: SemanticSiteId,
        program: &SemanticModule,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<Option<TraitBoundary>> {
        let definition = inventory.definition(context.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("actual trait invocation has no original body"));
        };
        let mut node = None;
        for row in site_nodes {
            b.spend(1, WORK)?;
            if row.site != site {
                continue;
            }
            let SemanticOriginalNode::Expression(id) = row.node else {
                return Err(fail(
                    "actual trait invocation selects an original statement",
                ));
            };
            if node.replace(id).is_some() {
                return Err(fail(
                    "actual trait invocation has multiple original node identities",
                ));
            }
        }
        b.lookup(self.trait_calls.len(), WORK)?;
        let Some(call) = node.and_then(|node| self.trait_calls.get(&(context.definition, node)))
        else {
            return Ok(None);
        };
        let mut arguments = Vec::new();
        let mut method_actuals = Vec::new();
        let mut parameters = Vec::new();
        for argument in &call.arguments {
            Solver::push(
                &mut arguments,
                self.argument(*argument, context, program, inventory, b, 0)?,
                b,
            )?;
        }
        for argument in &call.method_actuals {
            Solver::push(
                &mut method_actuals,
                self.argument(*argument, context, program, inventory, b, 0)?,
                b,
            )?;
        }
        for parameter in call
            .parameters
            .iter()
            .skip(usize::from(call.dynamic.is_some()))
        {
            Solver::push(
                &mut parameters,
                self.value(*parameter, context, program, inventory, b, 0)?,
                b,
            )?;
        }
        Ok(Some(TraitBoundary {
            declaration: call.declaration,
            member: call.member,
            method: call.method,
            receiver: self.value(call.receiver, context, program, inventory, b, 0)?,
            receiver_anchor: call.receiver_anchor,
            dynamic: call.dynamic,
            arguments,
            method_actuals,
            parameters,
            result: self.value(call.result, context, program, inventory, b, 0)?,
            call_kind: call.call_kind,
        }))
    }
}

impl Solver<'_, '_> {
    fn static_trait_receiver(
        &mut self,
        frame: &Frame,
        object: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        let Some(path) = sealed::path(object, b, depth + 1)? else {
            return Ok(None);
        };
        if self.local(frame, path[0], b)?.is_some() {
            return Ok(None);
        }
        let definition = self.inventory.definition(frame.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { names, .. } = &definition.body else {
            return Err(fail("original static method lacks its lexical namespace"));
        };
        let mut selected = None;
        for binding in names {
            b.spend(
                binding
                    .path
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(path.iter().map(|s| s.len()).sum::<usize>() + 1),
                WORK,
            )?;
            if binding.namespace == SemanticOriginalNamespace::Value
                && binding.path.len() == 1
                && binding.path[0] == path[0]
            {
                return Ok(None);
            }
            if binding.namespace == SemanticOriginalNamespace::Type
                && binding.path.len() == path.len()
                && binding.path.iter().zip(&path).all(|(a, b)| a == b)
                && selected.replace(&binding.value).is_some()
            {
                return Err(fail("original static method has competing type owners"));
            }
        }
        match selected {
            Some(SemanticOriginalNameValue::Recipe(SemanticActualRecipe::Type(recipe))) => {
                self.recipe(frame, recipe, b, depth + 1).map(Some)
            }
            // Concrete inherent and generic nominal applications require their
            // own original provider selection, not a formal-trait premise.
            _ => Ok(None),
        }
    }

    fn receiver_traits(
        &mut self,
        frame: &Frame,
        receiver: Term,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Vec<(LifeDeclarationId, Vec<Term>)>> {
        self.step(depth, b)?;
        let receiver = self.type_root(receiver, b)?;
        if let Kind::Dyn(ty) = &self.state.arena[receiver] {
            b.spend(ty.work(), WORK)?;
            return Ok(vec![(ty.declaration, ty.arguments.clone())]);
        }
        let (owner, ordinal) = match self.state.arena[receiver] {
            Kind::Rigid(owner, ordinal) => (owner, Some(ordinal)),
            Kind::SelfType(owner) => (owner, None),
            _ => return Ok(Vec::new()),
        };
        let declaration = self.inventory.definition(owner, b)?;
        let mut requirements = Vec::new();
        if let Some(ordinal) = ordinal {
            let Some(SemanticFormal::Type { requirements: rows }) =
                declaration.parameters.get(ordinal as usize)
            else {
                return Err(fail(
                    "original method receiver changes its Type formal kind",
                ));
            };
            for row in rows {
                Self::push(&mut requirements, row, b)?;
            }
        }
        for row in &declaration.requirements {
            Self::push(&mut requirements, row, b)?;
        }
        let source = if owner == frame.definition {
            frame
        } else {
            self.state
                .frames
                .get(&owner)
                .ok_or_else(|| fail("original receiver premise has no lexical scope"))?
        };
        b.spend(source.formals.len(), WORK)?;
        let source = trait_frame(owner, source.formals.clone());
        let mut applications = Vec::new();
        if let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) =
            declaration.body
        {
            b.spend(source.formals.len(), WORK)?;
            Self::push(&mut applications, (declaration, source.formals.clone()), b)?;
        }
        for requirement in requirements {
            let SemanticRequirementRecipe::Trait { ty, application } = requirement else {
                continue;
            };
            let subject = self.recipe(&source, ty, b, depth + 1)?;
            if !self.same_original(receiver, subject, b, depth + 1)? {
                continue;
            }
            let arguments = self.trait_arguments(&source, application, b, depth + 1)?;
            Self::push(&mut applications, (application.declaration, arguments), b)?;
        }
        Ok(applications)
    }

    #[allow(clippy::too_many_arguments)]
    fn original_method_scheme(
        &mut self,
        caller: &Frame,
        receiver: Term,
        declaration: LifeDeclarationId,
        arguments: &[Term],
        method: SemanticDefinitionId,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(Vec<Term>, Vec<Term>, Term)> {
        self.step(depth, b)?;
        let original = self.inventory.definition(method, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            signature,
            receiver_parameter,
            ..
        } = &original.body
        else {
            return Err(fail(
                "original trait method has no complete signature and body identity",
            ));
        };
        if receiver_parameter.is_some_and(|ordinal| ordinal != 0)
            || (*receiver_parameter == Some(0) && signature.parameters.is_empty())
            || signature.bound_lifetimes != 0
        {
            return Err(unknown(
                "original member needs its complete receiver or quantified invocation relation",
            ));
        }
        if !written.is_empty() && written.len() != original.parameters.len() {
            return Err(fail("original method actuals change its full mixed arity"));
        }
        let mut actuals = Vec::new();
        for (ordinal, formal) in original.parameters.iter().enumerate() {
            let actual = match (formal, written.get(ordinal)) {
                (SemanticFormal::Type { .. }, Some(CoreGenericArgument::Type(ty))) => {
                    self.recipe(caller, ty, b, depth + 1)?
                }
                (SemanticFormal::Type { .. }, None) => self.var(b)?,
                (SemanticFormal::Lifetime, Some(CoreGenericArgument::Lifetime(name))) => {
                    self.named_life(caller, name, b, depth + 1)?
                }
                (SemanticFormal::Lifetime, None) => self.term(Kind::LifeVariable(None), b)?,
                _ => {
                    return Err(unknown(
                        "original generic method Const actual needs its exact substituted carrier proof",
                    ));
                }
            };
            Self::push(&mut actuals, actual, b)?;
        }
        let scope = self.trait_scope(declaration, b)?;
        if original.parent != Some(scope) {
            return Err(fail(
                "original method signature is not a member of this actual trait application",
            ));
        }
        b.spend(arguments.len().saturating_add(actuals.len()), WORK)?;
        let context = trait_frame(method, actuals.clone());
        let saved = self
            .state
            .frames
            .remove(&scope)
            .ok_or_else(|| fail("original trait method has no checked parent scope frame"))?;
        self.state
            .frames
            .insert(scope, trait_frame(scope, arguments.to_vec()));
        Self::push(&mut self.trait_state.selves, (scope, receiver), b)?;
        let mut implicits = BTreeMap::new();
        for row in &original.implicit_lifetimes {
            let life = self.term(Kind::LifeVariable(None), b)?;
            b.lookup(implicits.len(), WORK)?;
            if implicits.insert(row.ordinal, life).is_some() {
                return Err(fail(
                    "original method repeats an implicit lifetime occurrence",
                ));
            }
        }
        let lifetimes = self.invocation_actuals(signature.invocation_lifetimes, b)?;
        Self::push(
            &mut self.role_frames,
            roles::RoleFrame {
                owner: method,
                lifetimes,
                implicits,
            },
            b,
        )?;
        let result: CompileResult<(Vec<Term>, Term)> = (|| {
            let mut parameters = Vec::new();
            for parameter in &signature.parameters {
                if !matches!(
                    parameter.mode,
                    PassingMode::Infer
                        | PassingMode::Copy
                        | PassingMode::Borrow
                        | PassingMode::BorrowMut
                ) {
                    return Err(unknown(
                        "original method argument requires its complete owning resource mode theorem",
                    ));
                }
                let term = self.recipe(&context, &parameter.ty, b, depth + 1)?;
                Self::push(&mut parameters, term, b)?;
            }
            for formal in &original.parameters {
                if let SemanticFormal::Type { requirements } = formal {
                    self.premise_needs(&context, requirements, b)?;
                }
            }
            self.premise_needs(&context, &original.requirements, b)?;
            let result = self.recipe(&context, &signature.result, b, depth + 1)?;
            Ok((parameters, result))
        })();
        self.role_frames.pop();
        self.trait_state.selves.pop();
        self.state.frames.insert(scope, saved);
        let (parameters, result) = result?;
        Ok((actuals, parameters, result))
    }

    pub(in crate::semantic::original) fn original_trait_call(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        callee: &SemanticOriginalExpr,
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        let (callee, written) = match &callee.kind {
            CoreExprKind::GenericApply { callee, arguments } => {
                (callee.as_ref(), arguments.as_slice())
            }
            _ => (callee, &[][..]),
        };
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &callee.kind
        else {
            return Ok(None);
        };
        let place = self.place(frame, object, false, b, depth + 1)?;
        let (receiver, actual_receiver, anchor, dynamic) = if let Some(place) = &place {
            if !place.path.is_empty() {
                return Err(unknown(
                    "projected original trait receiver needs its complete physical head relation",
                ));
            }
            let actual = self.type_root(place.selected, b)?;
            let (inner, address, dynamic) = match &self.state.arena[actual] {
                Kind::Address { inner, .. } => {
                    let child = self.type_root(*inner, b)?;
                    (
                        *inner,
                        Some(actual),
                        if matches!(self.state.arena[child], Kind::Dyn(_)) {
                            Some(false)
                        } else {
                            None
                        },
                    )
                }
                Kind::Dyn(ty) if ty.owned => {
                    b.spend(ty.work(), WORK)?;
                    let mut borrowed = ty.clone();
                    borrowed.owned = false;
                    (self.term(Kind::Dyn(borrowed), b)?, None, Some(true))
                }
                _ => return Ok(None),
            };
            let anchor = place.anchor.ok_or_else(|| {
                unknown("original trait receiver has no exact original place anchor")
            })?;
            (inner, address, Some(anchor), dynamic)
        } else {
            let Some(receiver) = self.static_trait_receiver(frame, object, b, depth + 1)? else {
                return Ok(None);
            };
            (receiver, None, None, None)
        };
        if let Some(owner) = owner {
            let expected = self.recipe(frame, owner, b, depth + 1)?;
            self.unify(receiver, expected, depth + 1, b)?;
        }
        let applications = self.receiver_traits(frame, receiver, b, depth + 1)?;
        let mut selected = None;
        for (declaration, actuals) in applications {
            let raw = self.inventory.original_dyn(declaration, b)?;
            for (ordinal, row) in raw.members.iter().enumerate() {
                b.spend(row.name.len().saturating_add(member.len() + 1), WORK)?;
                if row.name != *member || !matches!(row.kind, DynMemberKind::Method(_)) {
                    continue;
                }
                if selected
                    .replace((declaration, actuals.clone(), ordinal as u32))
                    .is_some()
                {
                    return Err(fail(
                        "original method name has several receiver trait/member identities",
                    ));
                }
            }
        }
        let Some((declaration, trait_actuals, member)) = selected else {
            return Ok(None);
        };
        if place.is_some() {
            self.place(frame, object, true, b, depth + 1)?;
        }
        let method =
            trait_members::member_scope(self.program, declaration, member, self.inventory, b)?;
        let (method_actuals, parameters, result) = self.original_method_scheme(
            frame,
            receiver,
            declaration,
            &trait_actuals,
            method,
            written,
            b,
            depth + 1,
        )?;
        let definition = self.inventory.definition(method, b)?;
        let SemanticDefinitionBody::FunctionWithBody {
            receiver_parameter,
            signature,
            ..
        } = &definition.body
        else {
            return Err(fail("original trait method lost its signature"));
        };
        if receiver_parameter.is_some() != anchor.is_some()
            || parameters.len()
                != arguments
                    .len()
                    .saturating_add(usize::from(anchor.is_some()))
        {
            return Err(fail(
                "original method argument count differs from its complete signature",
            ));
        }
        if dynamic.is_some() {
            let Kind::Address {
                mutable: false,
                inner,
                ..
            } = self.state.arena[self.type_root(parameters[0], b)?]
            else {
                return Err(unknown(
                    "original dynamic receiver needs its complete mutable or owning mode theorem",
                ));
            };
            if !self.same_original(inner, receiver, b, depth + 1)? {
                return Err(fail("original dynamic method changes its erased Self type"));
            }
        }
        // Only the fresh call-local outer life is solved by this boundary.
        // Pointee identities are ordinary full type equations and stay rigid.
        if let Some(actual_receiver) = actual_receiver {
            let adapted = match (
                &self.state.arena[self.type_root(parameters[0], b)?],
                &self.state.arena[actual_receiver],
            ) {
                (
                    Kind::Address { mutable: false, .. },
                    Kind::Address {
                        mutable: true,
                        life,
                        inner,
                    },
                ) => {
                    let (life, inner) = (*life, *inner);
                    self.term(
                        Kind::Address {
                            mutable: false,
                            life,
                            inner,
                        },
                        b,
                    )?
                }
                _ => actual_receiver,
            };
            self.unify(parameters[0], adapted, depth + 1, b)?;
        }
        for (argument, expected) in arguments
            .iter()
            .zip(parameters.iter().skip(usize::from(anchor.is_some())))
        {
            self.call_argument(frame, argument, *expected, b, depth + 1)?;
        }
        b.spend(
            trait_actuals
                .len()
                .saturating_add(method_actuals.len() + parameters.len()),
            WORK,
        )?;
        b.lookup(self.state.trait_calls.len(), WORK)?;
        if self
            .state
            .trait_calls
            .insert(
                (frame.definition, node),
                TraitCall {
                    declaration,
                    member,
                    method,
                    receiver,
                    receiver_anchor: anchor,
                    dynamic,
                    arguments: trait_actuals,
                    method_actuals,
                    parameters,
                    result,
                    call_kind: signature.call_kind,
                },
            )
            .is_some()
        {
            return Err(fail("original trait call has competing member resolutions"));
        }
        Ok(Some(result))
    }
}
