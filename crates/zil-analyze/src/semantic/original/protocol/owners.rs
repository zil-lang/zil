//! Original owner-result identities. A complete owner type permits tracking a
//! fresh call result, never assuming that an arbitrary input is already Closed.
use super::*;
impl Proof<'_, '_> {
    pub(super) fn owned_creation(
        &self,
        state: &mut State,
        node: NodeId,
        expressions: &[SemanticOriginalExpr],
        arguments: &[Data],
        b: &mut Budget,
    ) -> CompileResult<Data> {
        let Kind::ErrorUnion { error, .. } = self.checked.arena[self.expression_term(node, b)?] else {
            return Err(fail("original owning Task constructor loses its full result carrier"));
        };
        let Kind::Nominal { definition, fields, .. } = &self.checked.arena[self.checked.root(error, b)?] else {
            return Err(fail("original owning Task constructor loses its input-return failure product"));
        };
        let declaration = self.inventory.definition(*definition, b)?;
        let SemanticDefinitionBody::Nominal { declaration: identity, .. } = declaration.body else {
            return Err(fail("original owning Task failure has no full nominal declaration"));
        };
        if ![zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD,
            zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE].contains(&identity)
            || self.inventory.protocol_task_definition(identity, b)? != *definition
            || fields.len() != 2 || expressions.len() != arguments.len() || arguments.is_empty()
        {
            return Err(fail("original owning Task changes its authenticated complete input-return constructor"));
        }
        let Kind::Tuple(inputs) = &self.checked.arena[self.checked.root(fields[1], b)?] else {
            return Err(fail("original owning Task failure inputs have no complete ordered tuple"));
        };
        if inputs.len() + 1 != arguments.len() {
            return Err(fail("original owning Task failure changes its ordered input arity"));
        }
        let before = state.copy(b)?;
        let mut consumed = Vec::new();
        for expression in expressions {
            Solver::push(&mut consumed, self.async_owner(self.expression_term(expression.id, b)?, b)?, b)?;
        }
        state.call_effect(arguments, &consumed, b)?;
        // Moved exact registration heads are private to the new frame on
        // success. Failure returns each one at its own input ordinal. Borrowed
        // heads or already opaque inputs can still expose other live owners.
        b.spend(arguments.len(), WORK)?;
        let exposed = arguments.iter().zip(&consumed).any(|(argument, moved)| {
            argument.borrow_exposure()
                && !(*moved && argument.sources.only_exact_root().is_some() && argument.outcomes.is_empty())
        });
        let (mut result, result_role) = self.fresh_owner(state, node, exposed, b)?;
        let mut failure = before.phases;
        b.lookup(failure.len(), WORK)?;
        failure.insert(result_role, 0);
        for (index, input) in arguments.iter().skip(1).enumerate() {
            input.charge_sources(b)?;
            let ordinal = u32::try_from(index).map_err(|_| resource("original returned input ordinal overflow"))?;
            let prefix = [PlaceProjection::ErrorArm(true), PlaceProjection::Field(1), PlaceProjection::Field(ordinal)];
            result.sources.append_under(&prefix, &input.sources, b)?;
        }
        b.spend(state.phases.len(), WORK)?;
        Solver::push(&mut result.outcomes, (ProtocolExit::ErrorOk, state.phases.clone()), b)?;
        b.spend(failure.len(), WORK)?;
        Solver::push(&mut result.outcomes, (ProtocolExit::ErrorErr, failure.clone()), b)?;
        for (role, phase) in failure {
            b.lookup(state.phases.len(), WORK)?;
            *state.phases.entry(role).or_insert(0) |= phase;
        }
        Ok(result)
    }

    pub(super) fn projected_field(&self, node: NodeId, member: &str, b: &mut Budget) -> CompileResult<u32> {
        let term = self.expression_term(node, b)?;
        if let Kind::Tuple(fields) = &self.checked.arena[term] {
            b.spend(member.len().saturating_add(fields.len() + 1), WORK)?;
            if member.is_empty() || !member.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(fail("original projected tuple owner selects no numeric field"));
            }
            let ordinal = member.parse::<u32>()
                .map_err(|_| resource("original projected tuple owner ordinal overflow"))?;
            if ordinal as usize >= fields.len() {
                return Err(fail("original projected tuple owner exceeds its complete arity"));
            }
            return Ok(ordinal);
        }
        let Kind::Nominal { definition, fields, .. } = &self.checked.arena[term] else {
            return Err(unknown("original projected owner needs its exact complete product field"));
        };
        let SemanticDefinitionBody::Nominal {
            body: SemanticNominalBody::Product { fields: recipes }, ..
        } = &self.inventory.definition(*definition, b)?.body else {
            return Err(fail("original projected owner selects another nominal family"));
        };
        if fields.len() != recipes.len() {
            return Err(fail("original projected owner product omits an original field"));
        }
        let mut selected = None;
        for (index, field) in recipes.iter().enumerate() {
            b.spend(member.len().saturating_add(field.name.len() + 1), WORK)?;
            if field.name == member && selected.replace(index).is_some() {
                return Err(fail("original projected owner product has duplicate fields"));
            }
        }
        u32::try_from(selected.ok_or_else(|| fail("original projected owner selects no declared field"))?)
            .map_err(|_| resource("original projected owner field ordinal overflow"))
    }

    pub(super) fn async_owner(&self, mut term: Term, b: &mut Budget) -> CompileResult<bool> {
        for _ in 0..=b.partial_depth {
            term = self.checked.root(term, b)?;
            if let Kind::CallResult { hidden, .. } = self.checked.arena[term] {
                term = hidden;
                continue;
            }
            if let Kind::ErrorUnion { result, .. } = self.checked.arena[term] {
                term = result;
                continue;
            }
            let Kind::Wrapped(Wrapper::Managed, payload) = self.checked.arena[term] else {
                return Ok(false);
            };
            return Ok(matches!(
                self.checked.arena[self.checked.root(payload, b)?],
                Kind::Wrapped(Wrapper::AsyncRegistration, _) | Kind::Task(_)
            ));
        }
        Err(resource(
            "original owner-result identity exceeds its finite type depth",
        ))
    }
    pub(super) fn fresh_owner(
        &self,
        state: &mut State,
        node: NodeId,
        exposed: bool,
        b: &mut Budget,
    ) -> CompileResult<(Data, u32)> {
        let SemanticDefinitionBody::FunctionWithBody { parameters, .. } = &self.root.body
        else {
            return Err(fail(
                "original owner result has no complete function domain",
            ));
        };
        let frame = self
            .checked
            .frames
            .get(&self.root.id)
            .ok_or_else(|| fail("original owner construction has no checked lexical frame"))?;
        // Formal parameters, original inherited roots and construction sites
        // occupy disjoint namespaces. Equal node/slot numbers confer nothing.
        let base = u32::try_from(
            parameters
                .len()
                .checked_add(frame.inherited)
                .ok_or_else(|| resource("original owner role domain overflow"))?,
        )
        .map_err(|_| resource("original owner role ordinal overflow"))?;
        let mut births = self.births.borrow_mut();
        b.lookup(births.len(), WORK)?;
        let ordinal = u32::try_from(births.len()).map_err(|_| resource("original owner site count overflow"))?;
        let role = *births.entry((self.definition.id, node)).or_insert(base.checked_add(ordinal)
            .ok_or_else(|| resource("original owner role domain overflow"))?);
        drop(births);
        b.lookup(state.phases.len(), WORK)?;
        if state.phases.get(&role).is_some_and(|phase| *phase != 0) {
            return Err(unknown(
                "original repeated owner construction needs separate live-instance responsibility",
            ));
        }
        state.phases.insert(role, ANY);
        Ok((Data {
            sources: OwnerSources::exact(role).with_opaque(exposed),
            outcomes: Vec::new(),
        }, role))
    }
}

impl Proof<'_, '_> {
    pub(super) fn captured_heads(
        &self,
        phases: &mut BTreeMap<u32, u8>,
        locals: &mut Vec<(String, Data)>,
        b: &mut Budget,
    ) -> CompileResult<()> {
        let definition = self.inventory.definition(self.definition.id, b)?;
        let SemanticDefinitionBody::FunctionWithBody { parameters, .. } = &definition.body else {
            return Err(fail(
                "original protocol capture has no full function declaration",
            ));
        };
        let frame = self
            .checked
            .frames
            .get(&self.definition.id)
            .ok_or_else(|| {
                fail("original protocol captures have no independently checked lexical frame")
            })?;
        if frame.inherited > frame.locals.len() {
            return Err(fail(
                "original protocol capture prefix exceeds its complete lexical frame",
            ));
        }
        for (index, local) in frame.locals.iter().take(frame.inherited).enumerate() {
            b.lookup(frame.captured.len(), WORK)?;
            let Some(term) = frame.captured.get(&index) else {
                continue;
            };
            let mut root = self.checked.root(*term, b)?;
            for _ in 0..=b.partial_depth {
                if let Kind::CallResult { hidden, .. } = self.checked.arena[root] {
                    root = self.checked.root(hidden, b)?;
                } else {
                    break;
                }
            }
            let head = match self.checked.arena[root] {
                Kind::Address {
                    mutable: true,
                    inner,
                    ..
                } => self.async_owner(inner, b)?,
                _ => self.async_owner(root, b)?,
            };
            let alias = if head {
                let role = u32::try_from(
                    parameters
                        .len()
                        .checked_add(index)
                        .ok_or_else(|| resource("original captured owner role overflow"))?,
                )
                .map_err(|_| resource("original captured owner role ordinal overflow"))?;
                b.lookup(phases.len(), WORK)?;
                if phases.insert(role, ANY).is_some() {
                    return Err(fail(
                        "original captured owner role aliases another declaration namespace",
                    ));
                }
                Some(role)
            } else {
                None
            };
            b.spend(local.name.len(), WORK)?;
            Solver::push(
                locals,
                (
                    local.name.clone(),
                    Data {
                        sources: (alias.map(OwnerSources::exact).unwrap_or_default()).with_opaque(!head && self.captured_exposure(root, b, 0)?),
                        outcomes: Vec::new(),
                    },
                ),
                b,
            )?;
        }
        Ok(())
    }
}

impl Proof<'_, '_> {
    fn captured_exposure(&self, term: Term, b: &mut Budget, depth: usize) -> CompileResult<bool> {
        self.step(b, depth)?;
        let term = self.checked.root(term, b)?;
        match &self.checked.arena[term] {
            Kind::Atom(_)
            | Kind::ConstValue(_)
            | Kind::ConstRigid { .. }
            | Kind::Life(_)
            | Kind::LifeVariable(_) => Ok(false),
            Kind::CallResult { hidden, .. } => self.captured_exposure(*hidden, b, depth + 1),
            Kind::Code {
                kind: SemanticCodeKind::FunctionPointer | SemanticCodeKind::ForeignCallback,
                ..
            } => Ok(false),
            Kind::Tuple(fields) | Kind::Nominal { fields, .. } => {
                for field in fields {
                    if self.captured_exposure(*field, b, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Kind::Array { element, .. } | Kind::Slice(element) => {
                self.captured_exposure(*element, b, depth + 1)
            }
            Kind::ErrorUnion { error, result } => {
                Ok(self.captured_exposure(*error, b, depth + 1)?
                    || self.captured_exposure(*result, b, depth + 1)?)
            }
            Kind::Wrapped(_, inner) => self.captured_exposure(*inner, b, depth + 1),
            // An address, opaque callable or unknown generic domain can hide a
            // mutable alias to another tracked head. Its invocation/cleanup is
            // therefore never treated as a no-head call merely from layout.
            _ => Ok(true),
        }
    }
}
