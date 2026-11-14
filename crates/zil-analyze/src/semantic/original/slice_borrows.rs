//! Explicit range borrows retain the original backing-place identity. A range
//! projection is not itself a fresh borrow or a source of exclusive authority.
use super::*;

#[derive(Clone, Copy)]
pub(in crate::semantic) struct SliceProjection {
    pub(in crate::semantic) address: bool,
    pub(in crate::semantic) start: bool,
    pub(in crate::semantic) end: bool,
}

impl Solver<'_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn slice_borrow(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        operand: &SemanticOriginalExpr,
        mutable: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let CoreExprKind::Slice { object, start, end } = &operand.kind else {
            return Err(fail("original range borrow has no original slice place"));
        };
        let CoreExprKind::Name(name) = &object.kind else {
            return Err(unknown(
                "original range borrow needs its complete projected backing-place anchor",
            ));
        };
        let input = self
            .local(frame, name, b)?
            .ok_or_else(|| fail("original range borrow has no lexical backing place"))?;
        let (anchor, local_mutable, used, initialized) = {
            let mut selected = None;
            for local in frame.locals.iter().rev() {
                b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
                if local.name == *name {
                    selected = Some((local.anchor, local.mutable, local.used, local.initialized));
                    break;
                }
            }
            selected.ok_or_else(|| fail("original range borrow loses its backing binding"))?
        };
        if !initialized {
            return Err(fail(
                "original range borrow uses an uninitialized backing place",
            ));
        }
        if used {
            Self::push(
                &mut self.needs,
                Need {
                    term: input,
                    predicate: Predicate::Copy,
                },
                b,
            )?;
        }
        let (address, element) = match self.state.arena[self.type_root(input, b)?] {
            Kind::Address {
                mutable: source_mutable,
                inner,
                ..
            } => {
                if mutable && !source_mutable {
                    return Err(fail(
                        "original mutable range borrow strengthens a shared backing address",
                    ));
                }
                let element = match self.state.arena[self.type_root(inner, b)?] {
                    Kind::Slice(element) | Kind::Array { element, .. } => element,
                    _ => {
                        return Err(fail(
                            "original range borrow address has another complete pointee family",
                        ));
                    }
                };
                self.borrowed_argument(frame, object, b, depth + 1)?;
                (true, element)
            }
            Kind::Array { element, .. } => {
                if mutable && !local_mutable {
                    return Err(fail(
                        "original mutable array range borrow needs a mutable place",
                    ));
                }
                b.lookup(frame.expressions.len(), WORK)?;
                if frame.expressions.insert(object.id, input).is_some() {
                    return Err(fail("original range backing node is reused"));
                }
                (false, element)
            }
            _ => {
                return Err(fail(
                    "original range borrow has no array or slice backing place",
                ));
            }
        };
        Self::push(
            &mut self.needs,
            Need {
                term: element,
                predicate: Predicate::Copy,
            },
            b,
        )?;
        for bound in [start, end].into_iter().flatten() {
            let term = self.expression(frame, bound, b, depth + 1)?;
            Self::push(&mut self.integers, term, b)?;
            Self::push(&mut self.numeric, term, b)?;
        }
        let inner = self.term(Kind::Slice(element), b)?;
        b.lookup(frame.expressions.len(), WORK)?;
        if frame.expressions.insert(operand.id, inner).is_some() {
            return Err(fail("original range place node is reused"));
        }
        let anchor = anchor.ok_or_else(|| {
            unknown("original captured range needs its exact environment backing-place anchor")
        })?;
        b.lookup(self.state.borrows.len(), WORK)?;
        if self
            .state
            .borrows
            .insert(
                (frame.definition, node),
                Borrow {
                    anchor,
                    inner: input,
                    slice: Some(SliceProjection {
                        address,
                        start: start.is_some(),
                        end: end.is_some(),
                    }),
                },
            )
            .is_some()
        {
            return Err(fail("original range borrow has competing backing places"));
        }
        let ordinal = self.borrow_occurrence(frame.definition, node, b)?;
        let life = self.term(Kind::Life(Life::Implicit(frame.definition, ordinal)), b)?;
        self.term(
            Kind::Address {
                mutable,
                life,
                inner,
            },
            b,
        )
    }
}
