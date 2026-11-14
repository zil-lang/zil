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
        let place = self.borrow_place(frame, object, b, depth + 1)?;
        if place.index.is_some() {
            return Err(unknown(
                "original range borrow needs its complete indexed backing occurrence",
            ));
        }
        let input = place.inner;
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
                if place.indirect || !place.path.is_empty() {
                    return Err(unknown(
                        "original range needs its exact nested backing-reference load",
                    ));
                }
                (true, element)
            }
            Kind::Array { element, .. } => {
                if mutable && !place.mutable {
                    return Err(fail(
                        "original mutable array range borrow needs a mutable place",
                    ));
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
        b.lookup(self.state.borrows.len(), WORK)?;
        if self
            .state
            .borrows
            .insert(
                (frame.definition, node),
                Borrow {
                    anchor: place.anchor,
                    expression: place.expression,
                    index: place.index,
                    root: place.root,
                    path: place.path,
                    indirect: place.indirect,
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
