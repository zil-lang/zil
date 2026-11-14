//! Borrowed sequence places preserve the complete element type and reference
//! occurrence. Reading an element does not consume its exclusive slice head.
use super::*;
impl Solver<'_, '_> {
    fn sequence_place(
        &mut self,
        frame: &mut Frame,
        object: &SemanticOriginalExpr,
        write: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        self.step(depth, b)?;
        let term = self.borrowed_argument(frame, object, b, depth + 1)?;
        let root = self.type_root(term, b)?;
        match self.state.arena[root] {
            Kind::Address { mutable, inner, .. } => {
                if write && !mutable {
                    return Err(fail(
                        "original sequence write requires an exclusive address",
                    ));
                }
                match self.state.arena[self.type_root(inner, b)?] {
                    Kind::Slice(element) | Kind::Array { element, .. } => Ok(element),
                    _ => Err(fail(
                        "original sequence address has another complete pointee type",
                    )),
                }
            }
            Kind::Array { element, .. } if !write => {
                Self::push(
                    &mut self.needs,
                    Need {
                        term,
                        predicate: Predicate::Copy,
                    },
                    b,
                )?;
                Ok(element)
            }
            _ => Err(unknown(
                "original indexed place needs its complete array or borrowed-slice category",
            )),
        }
    }
    fn index_operand(
        &mut self,
        frame: &mut Frame,
        index: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        let term = self.expression(frame, index, b, depth + 1)?;
        Self::push(&mut self.integers, term, b)?;
        Self::push(&mut self.numeric, term, b)
    }
    pub(super) fn index_read(
        &mut self,
        frame: &mut Frame,
        object: &SemanticOriginalExpr,
        index: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Term> {
        let element = self.sequence_place(frame, object, false, b, depth + 1)?;
        self.index_operand(frame, index, b, depth + 1)?;
        Self::push(
            &mut self.needs,
            Need {
                term: element,
                predicate: Predicate::Copy,
            },
            b,
        )?;
        Ok(element)
    }
    pub(super) fn index_compound(
        &mut self,
        frame: &mut Frame,
        target: &SemanticOriginalExpr,
        op: BinaryOp,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        let CoreExprKind::Index { object, index } = &target.kind else {
            return Err(fail(
                "original sequence assignment loses its projected place",
            ));
        };
        let element = self.sequence_place(frame, object, true, b, depth + 1)?;
        self.index_operand(frame, index, b, depth + 1)?;
        let actual = self.expression(frame, value, b, depth + 1)?;
        self.unify(element, actual, depth + 1, b)?;
        Self::push(&mut self.numeric, element, b)?;
        if matches!(
            op,
            BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual
        ) {
            return Err(fail(
                "original projected compound assignment has a non-assignment operator",
            ));
        }
        if matches!(
            op,
            BinaryOp::AddWrapping
                | BinaryOp::SubtractWrapping
                | BinaryOp::MultiplyWrapping
                | BinaryOp::Remainder
                | BinaryOp::BitAnd
                | BinaryOp::BitOr
                | BinaryOp::BitXor
                | BinaryOp::ShiftLeft
                | BinaryOp::ShiftRight
        ) {
            Self::push(&mut self.integers, element, b)?;
        }
        Self::push(
            &mut self.needs,
            Need {
                term: element,
                predicate: Predicate::Copy,
            },
            b,
        )?;
        b.lookup(frame.expressions.len(), WORK)?;
        if frame.expressions.insert(target.id, element).is_some() {
            return Err(fail("original indexed target node is reused"));
        }
        Ok(())
    }
    pub(super) fn slice_call(
        &mut self,
        frame: &mut Frame,
        node: NodeId,
        callee: &SemanticOriginalExpr,
        arguments: &[SemanticOriginalExpr],
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<Term>> {
        self.step(depth, b)?;
        let CoreExprKind::Member {
            object,
            member,
            owner: None,
        } = &callee.kind
        else {
            return Ok(None);
        };
        if member != "len" {
            return Ok(None);
        }
        let CoreExprKind::Name(name) = &object.kind else {
            return Ok(None);
        };
        let Some(term) = self.local(frame, name, b)? else {
            return Ok(None);
        };
        let Kind::Address { inner, .. } = self.state.arena[self.type_root(term, b)?] else {
            return Ok(None);
        };
        if !matches!(self.state.arena[self.type_root(inner, b)?], Kind::Slice(_)) {
            return Ok(None);
        }
        if !arguments.is_empty() {
            return Err(fail(
                "original borrowed-slice length has no value arguments",
            ));
        }
        let input = self.borrowed_argument(frame, object, b, depth + 1)?;
        let anchor = self.registration_anchor(frame, object, b, depth + 1)?;
        b.lookup(self.state.intrinsics.len(), WORK)?;
        if self
            .state
            .intrinsics
            .insert((frame.definition, node), sealed::Intrinsic::SliceLength)
            .is_some()
        {
            return Err(fail(
                "original slice length has competing operation identities",
            ));
        }
        b.lookup(self.state.intrinsic_inputs.len(), WORK)?;
        self.state
            .intrinsic_inputs
            .insert((frame.definition, node), vec![input]);
        b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
        self.state
            .intrinsic_anchors
            .insert((frame.definition, node), anchor);
        Ok(Some(self.atom(Type::I64, b)?))
    }
}
