//! Callable obligations in the original declaration domain. Invocation
//! equations are local to a role and never update the global type/life graph.
use super::*;

pub(super) struct RoleFrame {
    pub(super) owner: SemanticDefinitionId,
    pub(super) lifetimes: Vec<Term>,
    pub(super) implicits: BTreeMap<u32, Term>,
}
impl Solver<'_, '_> {
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
        if signature.bound_lifetimes != 0 {
            return Err(unknown(
                "original callable needs its complete quantified role proof",
            ));
        }
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
        Self::push(
            &mut self.role_frames,
            RoleFrame {
                owner: frame.definition,
                lifetimes,
                implicits: BTreeMap::new(),
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
            let result = self.recipe(frame, &signature.result, b, depth + 1)?;
            Ok(CallableNeed {
                term,
                kind,
                parameters,
                result,
                call_kind: signature.call_kind,
                parameter_modes: modes,
                result_mode: signature.result_mode,
            })
        })();
        self.role_frames
            .pop()
            .ok_or_else(|| fail("original callable role lost its lexical frame"))?;
        let need = result?;
        Self::push(&mut self.callable_needs, need, b)
    }

    pub(super) fn role_premises_wf(
        &mut self,
        frame: &Frame,
        requirements: &[SemanticRequirementRecipe],
        b: &mut Budget,
    ) -> CompileResult<()> {
        for requirement in requirements {
            b.spend(1, WORK)?;
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
            let expected_life = match self.state.arena[self.root(life, b)?] {
                Kind::Life(life) => life,
                _ => return Err(unknown("original callable role has an unresolved lifetime")),
            };
            let actual_life = match self.state.arena[self.root(actual_life, b)?] {
                Kind::Life(life) => life,
                _ => return Err(unknown("actual original callable lifetime is unresolved")),
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
        let pending = std::mem::take(&mut self.callable_needs);
        for need in pending {
            let term = self.root(need.term, b)?;
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
            if signature.call_kind != need.call_kind
                || signature.result_mode != need.result_mode
                || parameters.len() != need.parameters.len()
                || originals.len() != parameters.len()
                || signature.parameters.len() != parameters.len()
                || (need.kind == CallableKind::Fn && self.mutating_closures.contains(&definition))
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
            self.role_match(
                need.result,
                result,
                &input_lives,
                &mut forward,
                &mut backward,
                true,
                b,
                0,
            )?;
        }
        Ok(())
    }
}
