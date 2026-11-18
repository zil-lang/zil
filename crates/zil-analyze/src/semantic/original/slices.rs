//! Borrowed sequence places preserve the complete element type and reference
//! occurrence. Reading an element does not consume its exclusive slice head.
use super::*;

pub(in crate::semantic) enum LengthReceiver {
    Local(LocalAnchor),
    Expression(NodeId),
}

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
            // A literal keeps its complete shared byte-view type as an atom.
            // Reading its element does not manufacture a new reference role
            // or assert that an arbitrary address has a literal source.
            Kind::Atom(ref ty) if strings::is_byte_type(ty) => {
                if write {
                    return Err(fail(
                        "original sequence write requires an exclusive address",
                    ));
                }
                self.atom(Type::U8, b)
            }
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
        let (term, temporary) = if let CoreExprKind::Name(name) = &object.kind {
            let Some(term) = self.local(frame, name, b)? else {
                return Ok(None);
            };
            (term, false)
        } else {
            // These other receiver forms can resolve as lexical or trait
            // member calls. Leave their original evaluation to that resolver.
            if sealed::path(object, b, depth + 1)?.is_some()
                || self.place(frame, object, false, b, depth + 1)?.is_some()
            {
                return Ok(None);
            }
            (self.borrowed_argument(frame, object, b, depth + 1)?, true)
        };
        let slice = match self.state.arena[self.type_root(term, b)?] {
            Kind::Address { inner, .. } => matches!(
                self.state.arena[self.type_root(inner, b)?],
                Kind::Slice(_)
            ),
            Kind::Atom(ref ty) => strings::is_byte_type(ty),
            _ => false,
        };
        if !slice {
            if temporary {
                return Err(unknown(
                    "original temporary length receiver is not a borrowed slice",
                ));
            }
            return Ok(None);
        }
        if !arguments.is_empty() {
            return Err(fail(
                "original borrowed-slice length has no value arguments",
            ));
        }
        let input = if temporary {
            term
        } else {
            let input = self.borrowed_argument(frame, object, b, depth + 1)?;
            let anchor = self.registration_anchor(frame, object, b, depth + 1)?;
            b.lookup(self.state.intrinsic_anchors.len(), WORK)?;
            self.state
                .intrinsic_anchors
                .insert((frame.definition, node), anchor);
            input
        };
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
        Ok(Some(self.atom(Type::I64, b)?))
    }
}

impl Checked {
    pub(in crate::semantic) fn slice_length_receiver(
        &self,
        instance: &SemanticInstance,
        site: SemanticSiteId,
        inventory: &Inventory<'_>,
        b: &mut Budget,
    ) -> CompileResult<LengthReceiver> {
        let definition = inventory.definition(instance.definition, b)?;
        let SemanticDefinitionBody::FunctionWithBody { site_nodes, .. } = &definition.body else {
            return Err(fail("original slice length has no declaration body"));
        };
        let mut selected = None;
        for link in site_nodes {
            b.spend(1, WORK)?;
            if link.site != site {
                continue;
            }
            let SemanticOriginalNode::Expression(node) = link.node else {
                return Err(fail("slice length selects an original statement"));
            };
            if selected.replace(node).is_some() {
                return Err(fail("slice length has competing original occurrences"));
            }
        }
        let node = selected.ok_or_else(|| fail("slice length has no original occurrence"))?;
        b.lookup(self.intrinsic_anchors.len(), WORK)?;
        if let Some(anchor) = self.intrinsic_anchors.get(&(instance.definition, node)) {
            return Ok(LengthReceiver::Local(*anchor));
        }
        let directory = inventory.original_index(instance.definition, b)?;
        let CoreExprKind::Call { callee, .. } = &directory.get_expr(node, b)?.kind else {
            return Err(fail("original slice length is not a call"));
        };
        let CoreExprKind::Member { object, .. } = &callee.kind else {
            return Err(fail("original slice length has no receiver expression"));
        };
        Ok(LengthReceiver::Expression(object.id))
    }
}
