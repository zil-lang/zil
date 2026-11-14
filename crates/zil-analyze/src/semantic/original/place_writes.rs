//! Interpret a complete source write place before its RHS. Observing a field
//! may peel an owning/address wrapper; writing additionally needs the actual
//! root's mutability and each traversed reference's exclusive qualification.
use super::*;

struct WritePlace {
    selected: Term,
    local: Option<usize>,
    writable: bool,
    description: bool,
    projected: bool,
}
impl Solver<'_, '_> {
    fn write_record(
        &self,
        frame: &mut Frame,
        node: NodeId,
        term: Term,
        b: &mut Budget,
    ) -> CompileResult<()> {
        b.lookup(frame.expressions.len(), WORK)?;
        if frame.expressions.insert(node, term).is_some() {
            return Err(fail(
                "original write place reuses a complete source expression node",
            ));
        }
        Ok(())
    }
    fn write_local(
        &mut self,
        frame: &mut Frame,
        name: &str,
        node: NodeId,
        initialized: bool,
        b: &mut Budget,
    ) -> CompileResult<Option<WritePlace>> {
        let mut selected = None;
        for (index, local) in frame.locals.iter().enumerate().rev() {
            b.spend(local.name.len().saturating_add(name.len() + 1), WORK)?;
            if local.name == name {
                selected = Some(index);
                break;
            }
        }
        let Some(index) = selected else {
            return Ok(None);
        };
        let local = &frame.locals[index];
        if initialized && !local.initialized {
            return Err(fail(
                "original projected write has no initialized root occupant",
            ));
        }
        let (selected, writable, used) = (local.ty, local.mutable, local.used);
        if initialized && used {
            Self::push(
                &mut self.needs,
                Need {
                    term: selected,
                    predicate: Predicate::Copy,
                },
                b,
            )?;
        }
        if initialized {
            self.record_environment_read(frame, index, node, b)?;
        }
        if index < frame.inherited {
            b.lookup(frame.captured.len(), WORK)?;
            frame.captured.insert(index, selected);
        }
        self.write_record(frame, node, selected, b)?;
        Ok(Some(WritePlace {
            selected,
            local: Some(index),
            writable,
            description: false,
            projected: false,
        }))
    }
    fn write_object(
        &mut self,
        frame: &mut Frame,
        object: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<WritePlace> {
        if let Some(place) = self.write_place(frame, object, true, b, depth + 1)? {
            return Ok(place);
        }
        let selected = self.expression(frame, object, b, depth + 1)?;
        match self.state.arena[self.type_root(selected, b)?] {
            Kind::Address { .. } | Kind::Desc { .. } => Ok(WritePlace {
                selected,
                local: None,
                writable: false,
                description: false,
                projected: true,
            }),
            _ => Err(unknown(
                "original projected write needs its complete initialized owning root or returned address role",
            )),
        }
    }
    fn write_payload(
        &self,
        place: &mut WritePlace,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<()> {
        self.step(depth, b)?;
        for _ in depth..=b.partial_depth {
            let term = self.type_root(place.selected, b)?;
            match self.state.arena[term] {
                Kind::Address { mutable, inner, .. } => {
                    if !mutable {
                        return Err(fail(
                            "original field write cannot strengthen a shared address into exclusive access",
                        ));
                    }
                    place.selected = inner;
                    place.writable = true;
                    place.projected = true;
                }
                Kind::Desc { inner, .. } => {
                    // Desc supplies writable-description access, never an
                    // owning slot. The final selected value must remain Copy.
                    place.selected = inner;
                    place.writable = true;
                    place.description = true;
                    place.projected = true;
                }
                Kind::Wrapped(Wrapper::Managed, inner) => {
                    // Ownership by itself does not override an immutable
                    // direct binding or an already shared reference edge.
                    place.selected = inner;
                    place.projected = true;
                }
                _ => return Ok(()),
            }
        }
        Err(resource(
            "original write place exceeds its complete wrapper depth",
        ))
    }
    fn write_place(
        &mut self,
        frame: &mut Frame,
        target: &SemanticOriginalExpr,
        initialized: bool,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<Option<WritePlace>> {
        self.step(depth, b)?;
        let place = match &target.kind {
            CoreExprKind::Name(name) => {
                return self.write_local(frame, name, target.id, initialized, b);
            }
            CoreExprKind::Group(inner) => {
                let Some(place) = self.write_place(frame, inner, initialized, b, depth + 1)? else {
                    return Ok(None);
                };
                place
            }
            CoreExprKind::Ascribed { ty, value } => {
                let Some(place) = self.write_place(frame, value, initialized, b, depth + 1)? else {
                    return Ok(None);
                };
                let expected = self.recipe(frame, ty, b, depth + 1)?;
                self.unify(place.selected, expected, depth + 1, b)?;
                place
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => {
                let mut place = self.write_object(frame, object, b, depth + 1)?;
                self.write_payload(&mut place, b, depth + 1)?;
                if let Some(owner) = owner {
                    let expected = self.recipe(frame, owner, b, depth + 1)?;
                    self.unify(place.selected, expected, depth + 1, b)?;
                }
                // write_payload already checked every implicit wrapper; field
                // is used only for nominal member identity and substitution.
                let (_, selected) = self.field(place.selected, member, b, depth + 1)?;
                place.selected = selected;
                place.projected = true;
                place
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let source = self.write_place(frame, operand, true, b, depth + 1)?;
                let (term, local, description) = if let Some(source) = source {
                    (source.selected, source.local, source.description)
                } else {
                    (self.expression(frame, operand, b, depth + 1)?, None, false)
                };
                let Kind::Address {
                    mutable: true,
                    inner,
                    ..
                } = self.state.arena[self.type_root(term, b)?]
                else {
                    return Err(fail(
                        "original dereference write requires its actual exclusive address",
                    ));
                };
                WritePlace {
                    selected: inner,
                    local,
                    writable: true,
                    description,
                    projected: true,
                }
            }
            CoreExprKind::Index { object, index } => {
                let mut place = self.write_object(frame, object, b, depth + 1)?;
                self.write_payload(&mut place, b, depth + 1)?;
                let element = match self.state.arena[self.type_root(place.selected, b)?] {
                    Kind::Array { element, .. } | Kind::Slice(element) => element,
                    _ => {
                        return Err(fail(
                            "original indexed write selects another complete sequence constructor",
                        ));
                    }
                };
                let index = self.expression(frame, index, b, depth + 1)?;
                Self::push(&mut self.numeric, index, b)?;
                Self::push(&mut self.integers, index, b)?;
                place.selected = element;
                place.projected = true;
                place
            }
            _ => return Ok(None),
        };
        self.write_record(frame, target.id, place.selected, b)?;
        Ok(Some(place))
    }
    fn write_access(
        &mut self,
        frame: &mut Frame,
        place: &WritePlace,
        initialized: bool,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !place.writable {
            return Err(fail(
                "original projected write targets an immutable owned root",
            ));
        }
        if place.description {
            Self::push(
                &mut self.needs,
                Need {
                    term: place.selected,
                    predicate: Predicate::Copy,
                },
                b,
            )?;
        }
        if let Some(index) = place.local {
            let local = frame
                .locals
                .get(index)
                .ok_or_else(|| fail("original projected write loses its exact lexical root"))?;
            if initialized && !local.initialized {
                return Err(fail(
                    "original projected write loses its initialized root occupant",
                ));
            }
            if index < frame.inherited {
                b.lookup(self.mutating_closures.len(), WORK)?;
                self.mutating_closures.insert(frame.definition);
            }
        }
        Ok(())
    }
    fn write_complete(
        &mut self,
        frame: &mut Frame,
        place: &WritePlace,
        b: &mut Budget,
    ) -> CompileResult<()> {
        if !frame.alive {
            return Ok(());
        }
        if let Some(index) = place.local {
            let local = frame
                .locals
                .get(index)
                .ok_or_else(|| fail("original write RHS loses its complete source root"))?;
            if place.projected && !local.initialized {
                return Err(fail(
                    "original write RHS leaves its projected root uninitialized",
                ));
            }
            // A source RHS may read the same root by value. That is legal only
            // when this entire original root is independently Copy. Replacing
            // one field cannot silently reinitialize a moved owning aggregate.
            if place.projected && local.used {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: local.ty,
                        predicate: Predicate::Copy,
                    },
                    b,
                )?;
            }
            frame.locals[index].used = false;
            if !place.projected {
                frame.locals[index].initialized = true;
            }
        }
        Ok(())
    }
    pub(super) fn projected_assign(
        &mut self,
        frame: &mut Frame,
        target: &SemanticOriginalExpr,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        if matches!(target.kind, CoreExprKind::Name(_)) {
            return Ok(false);
        }
        let Some(place) = self.write_place(frame, target, false, b, depth + 1)? else {
            return Ok(false);
        };
        self.write_access(frame, &place, place.projected, b)?;
        let actual = self.expression_expected(frame, value, place.selected, b, depth + 1)?;
        self.unify(actual, place.selected, depth + 1, b)?;
        if frame.alive {
            let old_live = if place.projected {
                true
            } else {
                let local = &frame.locals[place
                    .local
                    .ok_or_else(|| fail("original whole assignment has no exact root local"))?];
                local.initialized && !local.used
            };
            if old_live {
                Self::push(
                    &mut self.needs,
                    Need {
                        term: place.selected,
                        predicate: Predicate::SyncDrop,
                    },
                    b,
                )?;
            }
        }
        self.write_complete(frame, &place, b)?;
        Ok(true)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn projected_compound(
        &mut self,
        frame: &mut Frame,
        target: &SemanticOriginalExpr,
        op: BinaryOp,
        value: &SemanticOriginalExpr,
        b: &mut Budget,
        depth: usize,
    ) -> CompileResult<bool> {
        self.step(depth, b)?;
        if matches!(target.kind, CoreExprKind::Name(_)) {
            return Ok(false);
        }
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
                "original projected compound write has a nonassignment operator",
            ));
        }
        let Some(place) = self.write_place(frame, target, true, b, depth + 1)? else {
            return Ok(false);
        };
        self.write_access(frame, &place, true, b)?;
        Self::push(
            &mut self.needs,
            Need {
                term: place.selected,
                predicate: Predicate::Copy,
            },
            b,
        )?;
        let actual = self.expression_expected(frame, value, place.selected, b, depth + 1)?;
        self.unify(actual, place.selected, depth + 1, b)?;
        Self::push(&mut self.numeric, place.selected, b)?;
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
            Self::push(&mut self.integers, place.selected, b)?;
        }
        self.write_complete(frame, &place, b)?;
        Ok(true)
    }
}
