//! Select original providers by complete declaration patterns. Matching never
//! writes an equation into the caller's original rigid type/lifetime graph.
use super::*;

impl Solver<'_, '_> {
    pub(super) fn trait_scope_frames(&mut self, b: &mut Budget) -> CompileResult<()> {
        for definition in self.inventory.definitions.values() {
            if !matches!(definition.body, SemanticDefinitionBody::Scope(_)) {
                continue;
            }
            if definition.parent.is_some() || !definition.environment.is_empty() {
                return Err(unknown(
                    "original nested trait/impl scope needs its complete checked environment",
                ));
            }
            let mut arguments = Vec::new();
            for (ordinal, formal) in definition.parameters.iter().enumerate() {
                let argument = self.rigid_formal(definition.id, ordinal, formal, &arguments, b)?;
                Self::push(&mut arguments, argument, b)?;
            }
            let mut frame = trait_frame(definition.id, arguments);
            frame.result = self.atom(Type::Unit, b)?;
            self.snapshot(&frame, b)?;
        }
        for definition in self.inventory.definitions.values() {
            let SemanticDefinitionBody::Scope(recipe) = &definition.body else {
                continue;
            };
            let frame = self
                .state
                .frames
                .remove(&definition.id)
                .ok_or_else(|| fail("original scope frame is absent"))?;
            let self_type = match recipe {
                SemanticScopeRecipe::Trait { .. } => self.term(Kind::SelfType(definition.id), b)?,
                SemanticScopeRecipe::TraitImpl { self_type, .. }
                | SemanticScopeRecipe::InherentImpl { self_type, .. }
                | SemanticScopeRecipe::BuiltinImpl { self_type, .. } => {
                    Self::push(&mut self.trait_state.selves, (definition.id, usize::MAX), b)?;
                    let result = self.recipe(&frame, self_type, b, 0);
                    self.trait_state.selves.pop();
                    result?
                }
            };
            Self::push(&mut self.trait_state.selves, (definition.id, self_type), b)?;
            self.original_trait_facts(&frame, b)?;
            self.snapshot(&frame, b)?;
        }
        Ok(())
    }

    fn provider_pattern(
        &self,
        pattern: Term,
        actual: Term,
        owner: SemanticDefinitionId,
        bindings: &mut [Option<Term>],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        let pattern = self.type_root(pattern, b)?;
        let actual = self.type_root(actual, b)?;
        let formal = match self.state.arena[pattern] {
            Kind::Rigid(scope, ordinal) if scope == owner => Some((ordinal, 0)),
            Kind::Life(Life::Formal(scope, ordinal)) if scope == owner => Some((ordinal, 1)),
            Kind::ConstRigid {
                owner: scope,
                ordinal,
                ..
            } if scope == owner => Some((ordinal, 2)),
            _ => None,
        };
        if let Some((ordinal, kind)) = formal {
            let valid = match (&self.state.arena[actual], kind) {
                (Kind::Life(_) | Kind::LifeVariable(_), 1) => true,
                (Kind::ConstRigid { .. } | Kind::ConstValue(_), 2) => true,
                (
                    Kind::Life(_)
                    | Kind::LifeVariable(_)
                    | Kind::ConstRigid { .. }
                    | Kind::ConstValue(_),
                    _,
                ) => false,
                (_, 0) => true,
                _ => false,
            };
            if !valid {
                return Ok(false);
            }
            let slot = bindings
                .get_mut(ordinal as usize)
                .ok_or_else(|| fail("provider pattern references another mixed formal"))?;
            if let Some(old) = *slot {
                return self.same_original(old, actual, b, depth + 1);
            }
            *slot = Some(actual);
            return Ok(true);
        }
        match (&self.state.arena[pattern], &self.state.arena[actual]) {
            (
                Kind::Nominal {
                    definition: a,
                    arguments: aa,
                    ..
                },
                Kind::Nominal {
                    definition: z,
                    arguments: za,
                    ..
                },
            ) if a == z && aa.len() == za.len() => {
                for (a, z) in aa.iter().zip(za) {
                    if !self.provider_pattern(*a, *z, owner, bindings, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (
                Kind::Address {
                    mutable: a,
                    life: al,
                    inner: ai,
                },
                Kind::Address {
                    mutable: z,
                    life: zl,
                    inner: zi,
                },
            ) if a == z => Ok(
                self.provider_pattern(*al, *zl, owner, bindings, b, depth + 1)?
                    && self.provider_pattern(*ai, *zi, owner, bindings, b, depth + 1)?,
            ),
            (
                Kind::Desc {
                    life: al,
                    inner: ai,
                },
                Kind::Desc {
                    life: zl,
                    inner: zi,
                },
            ) => Ok(
                self.provider_pattern(*al, *zl, owner, bindings, b, depth + 1)?
                    && self.provider_pattern(*ai, *zi, owner, bindings, b, depth + 1)?,
            ),
            (Kind::Wrapped(a, ai), Kind::Wrapped(z, zi)) if a == z => {
                self.provider_pattern(*ai, *zi, owner, bindings, b, depth + 1)
            }
            (Kind::Slice(a), Kind::Slice(z)) => {
                self.provider_pattern(*a, *z, owner, bindings, b, depth + 1)
            }
            (
                Kind::Array {
                    element: a,
                    length: al,
                },
                Kind::Array {
                    element: z,
                    length: zl,
                },
            ) => Ok(
                self.provider_pattern(*a, *z, owner, bindings, b, depth + 1)?
                    && self.provider_pattern(*al, *zl, owner, bindings, b, depth + 1)?,
            ),
            (Kind::Tuple(a), Kind::Tuple(z)) if a.len() == z.len() => {
                for (a, z) in a.iter().zip(z) {
                    if !self.provider_pattern(*a, *z, owner, bindings, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => self.same_original(pattern, actual, b, depth + 1),
        }
    }

    pub(super) fn original_provider(
        &mut self,
        _caller: &Frame,
        receiver: Term,
        declaration: LifeDeclarationId,
        arguments: &[Term],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<(SemanticDefinitionId, Vec<Term>)> {
        self.step(depth, b)?;
        for (old, id, actuals) in &self.trait_state.proving_traits {
            if *id == declaration
                && self.same_original(*old, receiver, b, depth + 1)?
                && self.same_original_rows(actuals, arguments, b, depth + 1)?
            {
                return Err(fail(
                    "original trait provider requirements form a cyclic full receiver/application proof",
                ));
            }
        }
        let mut selected = None;
        for scope in self.inventory.definitions.values() {
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                trait_application,
                ..
            }) = &scope.body
            else {
                continue;
            };
            if trait_application.declaration != declaration {
                continue;
            }
            let self_type = self
                .trait_state
                .selves
                .iter()
                .find(|(owner, _)| *owner == scope.id)
                .map(|(_, term)| *term)
                .ok_or_else(|| fail("original provider lacks its checked Self scope"))?;
            b.spend(scope.parameters.len(), WORK)?;
            let mut bindings = vec![None; scope.parameters.len()];
            if !self.provider_pattern(self_type, receiver, scope.id, &mut bindings, b, depth + 1)? {
                continue;
            }
            let original = self
                .state
                .frames
                .get(&scope.id)
                .ok_or_else(|| fail("original provider has no full mixed frame"))?;
            b.spend(original.formals.len(), WORK)?;
            let original = trait_frame(scope.id, original.formals.clone());
            let application = self.trait_arguments(&original, trait_application, b, depth + 1)?;
            if application.len() != arguments.len() {
                return Err(fail("original provider changes its trait mixed arity"));
            }
            let mut matches = true;
            for (pattern, actual) in application.iter().zip(arguments) {
                matches &= self.provider_pattern(
                    *pattern,
                    *actual,
                    scope.id,
                    &mut bindings,
                    b,
                    depth + 1,
                )?;
            }
            if !matches {
                continue;
            }
            let mut actuals = Vec::new();
            for actual in bindings {
                Self::push(&mut actuals, actual.ok_or_else(|| fail("original provider has an unanchored formal not determined by Self and trait application"))?, b)?;
            }
            if selected.replace((scope.id, actuals)).is_some() {
                return Err(fail(
                    "original receiver trait application has several matching providers",
                ));
            }
        }
        let (owner, actuals) = selected.ok_or_else(|| {
            fail("original receiver trait application has no complete matching provider")
        })?;
        b.spend(actuals.len(), WORK)?;
        let frame = trait_frame(owner, actuals.clone());
        let scope = self.inventory.definition(owner, b)?;
        b.spend(arguments.len(), WORK)?;
        Self::push(
            &mut self.trait_state.proving_traits,
            (receiver, declaration, arguments.to_vec()),
            b,
        )?;
        Self::push(&mut self.trait_state.selves, (owner, receiver), b)?;
        let result: CompileResult<()> = (|| {
            for formal in &scope.parameters {
                if let SemanticFormal::Type { requirements } = formal {
                    self.premise_needs(&frame, requirements, b)?;
                }
            }
            self.premise_needs(&frame, &scope.requirements, b)
        })();
        self.trait_state.selves.pop();
        self.trait_state.proving_traits.pop();
        result?;
        let trait_owner = self.trait_scope(declaration, b)?;
        let trait_definition = self.inventory.definition(trait_owner, b)?;
        b.spend(arguments.len(), WORK)?;
        let trait_frame = trait_frame(trait_owner, arguments.to_vec());
        Self::push(
            &mut self.trait_state.proving_traits,
            (receiver, declaration, arguments.to_vec()),
            b,
        )?;
        Self::push(&mut self.trait_state.selves, (trait_owner, receiver), b)?;
        let result: CompileResult<()> = (|| {
            for formal in &trait_definition.parameters {
                if let SemanticFormal::Type { requirements } = formal {
                    self.premise_needs(&trait_frame, requirements, b)?;
                }
            }
            self.premise_needs(&trait_frame, &trait_definition.requirements, b)
        })();
        self.trait_state.selves.pop();
        self.trait_state.proving_traits.pop();
        result?;
        Ok((owner, actuals))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn provider_associated(
        &mut self,
        caller: &Frame,
        receiver: Term,
        declaration: LifeDeclarationId,
        arguments: Vec<Term>,
        member: u32,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        for (old_receiver, old_decl, old_arguments, old_member) in &self.trait_state.active {
            if *old_decl == declaration
                && *old_member == member
                && self.same_original(*old_receiver, receiver, b, depth + 1)?
                && self.same_original_rows(old_arguments, &arguments, b, depth + 1)?
            {
                return Err(fail(
                    "original associated query has a cyclic full receiver/application/member dependency",
                ));
            }
        }
        b.spend(arguments.len(), WORK)?;
        Self::push(
            &mut self.trait_state.active,
            (receiver, declaration, arguments.clone(), member),
            b,
        )?;
        let result: CompileResult<Term> = (|| {
            let trait_raw = self.inventory.original_dyn(declaration, b)?;
            let member_row = trait_raw
                .members
                .get(member as usize)
                .ok_or_else(|| fail("original associated query selects an absent trait member"))?;
            let (provider, actuals) =
                self.original_provider(caller, receiver, declaration, &arguments, b, depth + 1)?;
            let source = self.inventory.definition(provider, b)?;
            let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                declaration: implementation,
                ..
            }) = &source.body
            else {
                return Err(fail("original provider is not an implementation scope"));
            };
            let raw = self.inventory.original_dyn(*implementation, b)?;
            let mut implementation_member = None;
            for (ordinal, row) in raw.members.iter().enumerate() {
                b.spend(
                    row.name.len().saturating_add(member_row.name.len() + 1),
                    WORK,
                )?;
                if row.name == member_row.name {
                    if row.kind != member_row.kind
                        || implementation_member.replace(ordinal as u32).is_some()
                    {
                        return Err(fail(
                            "original provider associated override changes its exact trait member identity",
                        ));
                    }
                }
            }
            let override_row = implementation_member.and_then(|member| {
                raw.associated_definitions
                    .as_ref()
                    .and_then(|rows| rows.iter().find(|row| row.member == member))
            });
            let override_value = override_row.and_then(|row| row.value.as_ref());
            let default = trait_raw
                .associated_definitions
                .as_ref()
                .and_then(|rows| rows.iter().find(|row| row.member == member))
                .ok_or_else(|| fail("original trait omits its associated definition row"))?;
            let trait_scope = self.trait_scope(declaration, b)?;
            let (scope, actuals, value) = if let Some(value) = override_value {
                (provider, actuals, value)
            } else {
                (
                    trait_scope,
                    arguments.clone(),
                    default.value.as_ref().ok_or_else(|| {
                        fail("original provider omits a required associated definition")
                    })?,
                )
            };
            let DynAssociatedValueRecipe::Type(DynTypePattern::Semantic {
                scope: original,
                ty,
            }) = value
            else {
                return Err(unknown(
                    "associated RHS requires its complete original scoped type expression",
                ));
            };
            if *original != scope {
                return Err(fail(
                    "associated default or override is interpreted in another original scope",
                ));
            }
            let context = trait_frame(scope, actuals);
            Self::push(&mut self.trait_state.selves, (scope, receiver), b)?;
            let result = self.recipe(&context, ty, b, depth + 1);
            self.trait_state.selves.pop();
            let result = result?;
            // A value definition must discharge every declared member bound.
            // Install only this active query's already evaluated RHS while
            // checking Self.Item subjects, so those subjects do not recurse
            // into the same query or become unproved assumptions.
            b.spend(arguments.len(), WORK)?;
            Self::push(
                &mut self.trait_state.associated_values,
                (receiver, declaration, arguments.clone(), member, result),
                b,
            )?;
            let bound_frame = trait_frame(trait_scope, arguments);
            Self::push(&mut self.trait_state.selves, (trait_scope, receiver), b)?;
            let bound_result =
                self.member_bound_needs(&bound_frame, &default.requirements, b, depth + 1);
            self.trait_state.selves.pop();
            let bound_result = bound_result.and_then(|()| {
                if let Some(row) = override_row {
                    Self::push(&mut self.trait_state.selves, (scope, receiver), b)?;
                    let result = self.member_bound_needs(&context, &row.requirements, b, depth + 1);
                    self.trait_state.selves.pop();
                    result
                } else {
                    Ok(())
                }
            });
            self.trait_state.associated_values.pop();
            bound_result?;
            Ok(result)
        })();
        self.trait_state.active.pop();
        result
    }

    fn member_bound_needs(
        &mut self,
        frame: &Frame,
        requirements: &[DynObligation],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        for requirement in requirements {
            match requirement {
                DynObligation::Structural {
                    receiver: DynTypePattern::Semantic { scope, ty },
                    bound,
                } => {
                    if *scope != frame.definition {
                        return Err(fail(
                            "associated member constraint changes its original scope",
                        ));
                    }
                    let term = self.recipe(frame, ty, b, depth + 1)?;
                    let predicate = match bound {
                        DynStructuralBound::Copy => Predicate::Copy,
                        DynStructuralBound::Value => Predicate::Value,
                        DynStructuralBound::SyncDrop => Predicate::SyncDrop,
                        DynStructuralBound::StoreElement => Predicate::StoreElement,
                    };
                    Self::push(&mut self.needs, Need { term, predicate }, b)?;
                }
                _ => {
                    return Err(unknown(
                        "associated member constraints need complete original trait/callable/equality interpretation",
                    ));
                }
            }
        }
        Ok(())
    }
}
