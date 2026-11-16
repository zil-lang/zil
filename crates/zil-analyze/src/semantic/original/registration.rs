//! Original registration operations use the same complete source head, mode
//! and callback environment. Actual opcodes and runtime debt are checked again.
use super::*;
impl Solver<'_, '_> {
    fn state_argument(
        &mut self,
        frame: &mut Frame,
        value: &SemanticOriginalExpr,
        borrowed: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        if borrowed
            && let CoreExprKind::Group(inner) | CoreExprKind::Ascribed { value: inner, .. } =
                &value.kind
        {
            let term = self.state_argument(frame, inner, true, b, depth + 1)?;
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, term).is_some() {
                return Err(fail("original borrowed argument wrapper node is reused"));
            }
            return Ok(term);
        }
        if borrowed && let CoreExprKind::Name(name) = &value.kind {
            let local = frame
                .locals
                .iter()
                .rev()
                .find(|local| local.name == *name)
                .ok_or_else(|| fail("original registration head has no lexical value binder"))?;
            b.spend(
                frame
                    .locals
                    .iter()
                    .map(|local| local.name.len().saturating_add(name.len() + 1))
                    .sum::<usize>(),
                WORK,
            )?;
            if !local.initialized {
                return Err(fail("original registration head is uninitialized"));
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
            let term = local.ty;
            b.spend(frame.locals.len(), WORK)?;
            if let Some(index) = frame.locals.iter().rposition(|local| local.name == *name)
                && index < frame.inherited
            {
                b.lookup(frame.captured.len(), WORK)?;
                frame.captured.insert(index, term);
            }
            b.lookup(frame.expressions.len(), WORK)?;
            if frame.expressions.insert(value.id, term).is_some() {
                return Err(fail("original registration argument node is reused"));
            }
            return Ok(term);
        }
        self.expression(frame, value, b, depth + 1)
    }
    pub(super) fn registration_anchor(
        &self,
        frame: &Frame,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<LocalAnchor> {
        self.step(depth, b)?;
        match &value.kind {
            CoreExprKind::Group(inner)
            | CoreExprKind::Ascribed { value: inner, .. }
            | CoreExprKind::Unary {
                op: UnaryOp::Give,
                operand: inner,
            } => self.registration_anchor(frame, inner, b, depth + 1),
            CoreExprKind::Name(name) => {
                for local in frame.locals.iter().rev() {
                    b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
                    if local.name == *name {
                        return match local.anchor {
                            Some(anchor) => Ok(anchor),
                            None => self.environment_anchor(frame, name, b),
                        };
                    }
                }
                Err(fail("original registration operation has no lexical head"))
            }
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::BorrowMut,
                ..
            } => {
                b.lookup(self.state.borrows.len(), WORK)?;
                self.state
                    .borrows
                    .get(&(frame.definition, value.id))
                    .and_then(|borrow| borrow.anchor)
                    .ok_or_else(|| {
                        fail(
                            "original registration borrow loses its exact initialized source place",
                        )
                    })
            }
            _ => Err(unknown(
                "original registration operand needs its complete possible-head place relation",
            )),
        }
    }
    pub(super) fn registration_call(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        intrinsic: sealed::Intrinsic,
        written: &[CoreGenericArgument<SemanticTypeRecipe>],
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        if !written.is_empty() || arguments.len() != 1 {
            return Err(fail(
                "original registration operation changes its complete source arity",
            ));
        }
        let observing = matches!(
            intrinsic,
            sealed::Intrinsic::AsyncValid | sealed::Intrinsic::RegistrationValid
        );
        let borrowed = observing
            || matches!(
                intrinsic,
                sealed::Intrinsic::AsyncRequest | sealed::Intrinsic::AsyncPoll
            );
        let input = self.state_argument(frame, &arguments[0], borrowed, b, depth + 1)?;
        let payload = if observing {
            match self.state.arena[self.type_root(input, b)?] {
                Kind::Desc { inner, .. } | Kind::Wrapped(Wrapper::Managed, inner) => inner,
                Kind::Address { inner, .. } => {
                    let Kind::Wrapped(Wrapper::Managed, payload) =
                        self.state.arena[self.type_root(inner, b)?]
                    else {
                        return Err(fail(
                            "original registration observation borrows an actual owning head, not a description slot",
                        ));
                    };
                    payload
                }
                _ => {
                    return Err(fail(
                        "original registration observation has no actual owner or descriptor",
                    ));
                }
            }
        } else {
            let owned = if borrowed {
                let Kind::Address {
                    mutable: true,
                    inner,
                    ..
                } = self.state.arena[self.type_root(input, b)?]
                else {
                    return Err(fail(
                        "original registration close requires a mutable address of its complete owning head",
                    ));
                };
                inner
            } else {
                input
            };
            let Kind::Wrapped(Wrapper::Managed, payload) =
                self.state.arena[self.type_root(owned, b)?]
            else {
                return Err(fail(
                    "original registration operation selects a description or another non-owning carrier",
                ));
            };
            payload
        };
        let synchronous = matches!(
            intrinsic,
            sealed::Intrinsic::RegistrationValid | sealed::Intrinsic::RegistrationClose
        );
        if !matches!(self.state.arena[self.type_root(payload, b)?], Kind::Wrapped(wrapper, _) if wrapper == if synchronous { Wrapper::Registration } else { Wrapper::AsyncRegistration })
        {
            return Err(fail(
                "original registration operation selects another payload family",
            ));
        }
        if observing {
            let selected = self.type_root(input, b)?;
            let source = match self.state.arena[selected] {
                Kind::Address { inner, .. } => self.type_root(inner, b)?,
                _ => selected,
            };
            if matches!(self.state.arena[source], Kind::Wrapped(Wrapper::Managed, _)) {
                let place = self
                    .place(frame, &arguments[0], false, b, depth + 1)?
                    .ok_or_else(|| {
                        unknown("registration observation requires its exact original owning place")
                    })?;
                self.describe_argument(frame, &arguments[0], &place, source, b, depth + 1)?;
            }
        }
        let result = if intrinsic == sealed::Intrinsic::AsyncPoll {
            let mut definition = None;
            for candidate in self.inventory.definitions.values() {
                b.spend(1, WORK)?;
                if matches!(candidate.body,SemanticDefinitionBody::Nominal {declaration,kind:LifeNominalKind::Enum,..} if declaration==zil_ir::builtin_life::POLL)
                {
                    if definition.replace(candidate.id).is_some() {
                        return Err(fail(
                            "original registration Poll has competing compiler declarations",
                        ));
                    }
                    if candidate.parameters.len() != 1
                        || !matches!(candidate.parameters[0], SemanticFormal::Type { .. })
                    {
                        return Err(fail(
                            "original registration Poll loses its original type formal",
                        ));
                    }
                }
            }
            let definition = definition.ok_or_else(|| {
                fail("original registration poll lacks its real canonical Poll declaration")
            })?;
            let unit = self.atom(Type::Unit, b)?;
            self.application(definition, vec![unit], b, depth + 1)?
        } else {
            self.atom(if observing { Type::Bool } else { Type::Unit }, b)?
        };
        let anchor = self.registration_anchor(frame, &arguments[0], b, depth + 1)?;
        b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
        if self
            .state
            .intrinsic_anchors
            .insert((frame.definition, node), anchor)
            .is_some()
        {
            return Err(fail(
                "original registration operation has competing head anchors",
            ));
        }
        b.lookup(self.state.intrinsics.len(), WORK)?;
        if self
            .state
            .intrinsics
            .insert((frame.definition, node), intrinsic)
            .is_some()
        {
            return Err(fail(
                "original registration operation has competing schemes",
            ));
        }
        b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
        self.state
            .intrinsic_inputs
            .insert((frame.definition, node), vec![input]);
        Ok(result)
    }
}
