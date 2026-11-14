//! Carrier-sensitive exits and opaque call effects. Ordinary worker Ready is
//! distinct from canonical close Ready; no unobserved tag certifies retirement.
use super::*;
impl State {
    pub(super) fn call_effect(
        &mut self,
        arguments: &[Data],
        consumed: &[bool],
        b: &mut Budget,
    ) -> CompileResult<()> {
        self.invalidate(b)?;
        let exposed = arguments.iter().any(|argument| argument.exposed);
        for (role, phase) in &mut self.phases {
            b.spend(arguments.len().saturating_add(1), WORK)?;
            let mut passed = false;
            let mut retired = false;
            for (index, argument) in arguments.iter().enumerate() {
                if argument.alias == Some(*role) {
                    passed = true;
                    retired |= consumed.get(index) == Some(&true);
                }
            }
            if *phase == 0 {
                continue;
            }
            *phase = if retired {
                0
            } else if passed || exposed {
                ANY
            } else {
                // No actual typed head/borrow was passed. Other ordinary and
                // foreign code may complete or request close through previously
                // admitted raw protocol holders, but cannot replace this affine
                // original occupant or reopen its private closed admission gate.
                if *phase & OPEN != 0 {
                    ANY
                } else {
                    (*phase & CLOSED)
                        | if *phase & CLOSING != 0 {
                            CLOSING | CLOSED
                        } else {
                            0
                        }
                }
            };
        }
        Ok(())
    }
}
impl Proof<'_, '_> {
    pub(super) fn try_value(
        &self,
        state: &mut State,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Data> {
        let data = self.expression(state, value, b, depth + 1)?;
        let term = self.expression_term(value.id, b)?;
        if !matches!(self.checked.arena[term], Kind::ErrorUnion { .. }) {
            return Err(fail(
                "original protocol propagation has another complete result carrier",
            ));
        }
        if data.outcomes.is_empty() {
            let mut failure = state.copy(b)?;
            if let Some(role) = data.alias {
                failure.phases.insert(role, 0);
            }
            b.spend(failure.phases.len(), WORK)?;
            self.exit(
                &failure,
                &Data {
                    alias: None,
                    exposed: false,
                    outcomes: vec![(ProtocolExit::ErrorErr, failure.phases.clone())],
                },
                b,
            )?;
            return Ok(data);
        }
        let mut success = None::<State>;
        for (exit, phases) in &data.outcomes {
            b.spend(phases.len(), WORK)?;
            let mut branch = state.copy(b)?;
            branch.phases = phases.clone();
            match exit {
                ProtocolExit::ErrorErr => {
                    if let Some(role) = data.alias {
                        branch.phases.insert(role, 0);
                    }
                    b.spend(branch.phases.len(), WORK)?;
                    self.exit(
                        &branch,
                        &Data {
                            alias: None,
                            exposed: false,
                            outcomes: vec![(*exit, branch.phases.clone())],
                        },
                        b,
                    )?;
                }
                ProtocolExit::ErrorOk => {
                    if let Some(success) = &mut success {
                        success.join(&branch, b)?;
                    } else {
                        success = Some(branch);
                    }
                }
                _ => {
                    return Err(fail(
                        "original protocol ErrorUnion propagation carries another constructor's outcome",
                    ));
                }
            }
        }
        if let Some(success) = success {
            *state = success;
        } else {
            state.alive = false;
        }
        Ok(Data {
            alias: data.alias,
            exposed: data.exposed,
            outcomes: Vec::new(),
        })
    }
    pub(super) fn return_value(
        &self,
        state: &State,
        expression: Option<&SemanticOriginalExpr>,
        mut data: Data,
        b: &mut Budget,
    ) -> CompileResult<Data> {
        if !data.outcomes.is_empty() {
            return Ok(data);
        }
        let SemanticDefinitionBody::FunctionWithBody { signature, .. } = &self.definition.body
        else {
            return Err(fail(
                "original protocol return has no full declaration signature",
            ));
        };
        let exits = if self
            .inventory
            .protocol_poll_recipe(&signature.result, b)?
            .is_some()
        {
            vec![ProtocolExit::PollPending, ProtocolExit::PollReady]
        } else if matches!(signature.result, SemanticTypeRecipe::ErrorUnion { .. }) {
            let expression = expression.ok_or_else(|| {
                fail("original protocol ErrorUnion return omits its complete value")
            })?;
            let term = self.expression_term(expression.id, b)?;
            if matches!(self.checked.arena[term], Kind::ErrorUnion { .. }) {
                vec![ProtocolExit::ErrorOk, ProtocolExit::ErrorErr]
            } else {
                let frame = self
                    .checked
                    .frames
                    .get(&self.definition.id)
                    .ok_or_else(|| {
                        fail("original error return lacks its checked original declaration frame")
                    })?;
                let Kind::ErrorUnion { error, result } =
                    self.checked.arena[self.checked.root(frame.result, b)?]
                else {
                    return Err(fail(
                        "original error return changes its declaration constructor",
                    ));
                };
                let error = self.checked.root(error, b)?;
                let result = self.checked.root(result, b)?;
                if term == error && term != result {
                    vec![ProtocolExit::ErrorErr]
                } else {
                    let disjoint = match (&self.checked.arena[term], &self.checked.arena[error]) {
                        (Kind::Atom(a), Kind::Atom(z)) => a != z,
                        (Kind::Atom(_), Kind::Nominal { .. })
                        | (Kind::Nominal { .. }, Kind::Atom(_)) => true,
                        (
                            Kind::Nominal { definition: a, .. },
                            Kind::Nominal { definition: z, .. },
                        ) => a != z,
                        _ => false,
                    };
                    if disjoint {
                        vec![ProtocolExit::ErrorOk]
                    } else {
                        vec![ProtocolExit::ErrorOk, ProtocolExit::ErrorErr]
                    }
                }
            }
        } else {
            return Ok(data);
        };
        for exit in exits {
            b.spend(state.phases.len(), WORK)?;
            Solver::push(&mut data.outcomes, (exit, state.phases.clone()), b)?;
        }
        Ok(data)
    }
}
