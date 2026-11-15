//! A Copy assignment reads an identical stable RHS place through the address
//! already prepared for that assignment. This is a lowering scope, not a new
//! reference capability: calls, loans and invalidation remain in the real CFG.
use super::*;

#[derive(PartialEq, Eq)]
struct StablePlace {
    root: LocalId,
    index_bindings: Vec<LocalId>,
}

pub(crate) struct PreparedRead {
    target: CoreExpr,
    place: StablePlace,
    address: ValueId,
    ty: Type,
}

impl Builder<'_, '_> {
    fn stable_assignment_root(&mut self, e: &CoreExpr, depth: usize) -> R<Option<StablePlace>> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "prepared assignment place exceeds its finite depth",
                Some(e.span),
            ));
        }
        self.closures.snapshot(1, e.span)?;
        match &e.kind {
            CoreExprKind::Name(name) => {
                self.closures.snapshot(
                    self.scopes
                        .len()
                        .saturating_mul(self.function.locals.len().saturating_add(1))
                        .saturating_add(name.len()),
                    e.span,
                )?;
                let Some(local) = self.lookup(name) else {
                    return Ok(None);
                };
                let declaration = &self.function.locals[local.0 as usize];
                // A mutable pointer binding could be reassigned during the
                // RHS. Inline storage keeps its identity; replacing/moving
                // that storage must still pass the actual pending loan.
                if declaration.mutable
                    && matches!(declaration.ty, Type::Address { .. } | Type::Desc(_))
                {
                    self.closures
                        .snapshot(self.borrowed_captures.len().saturating_add(1), e.span)?;
                    let captured_inline = if let Some(captured) = self.borrowed_captures.get(&local)
                    {
                        self.closures.snapshot_type(captured, 1, 0, e.span)?;
                        self.closures.snapshot_type(&declaration.ty, 1, 0, e.span)?;
                        !matches!(captured, Type::Address { .. } | Type::Desc(_))
                            && matches!(&declaration.ty, Type::Address { inner, .. } if inner.as_ref() == captured)
                    } else {
                        false
                    };
                    // A borrowed capture local stores the fixed environment
                    // address. Its mutable flag describes the inline referent,
                    // while an assignment writes through that address. A
                    // captured pointer variable can itself be rebound and is
                    // deliberately excluded above.
                    if !captured_inline {
                        return Ok(None);
                    }
                }
                Ok(Some(StablePlace {
                    root: local,
                    index_bindings: Vec::new(),
                }))
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } if matches!(operand.kind, CoreExprKind::Name(_)) => {
                self.stable_assignment_root(operand, depth + 1)
            }
            CoreExprKind::Member { object, .. } => {
                let Some(root) = self.stable_assignment_root(object, depth + 1)? else {
                    return Ok(None);
                };
                // Do not reuse a path that reads an intermediate pointer
                // field: its stored occupant may change during RHS effects.
                if !matches!(object.kind, CoreExprKind::Name(_))
                    && self.known_address_checked(object)?
                {
                    return Ok(None);
                }
                Ok(Some(root))
            }
            CoreExprKind::Index { object, index } => {
                let Some(mut place) = self.stable_assignment_root(object, depth + 1)? else {
                    return Ok(None);
                };
                let binding = match &index.kind {
                    CoreExprKind::Integer(_) => None,
                    CoreExprKind::Literal(value) if value.ty().is_integer() => None,
                    CoreExprKind::Name(name) => {
                        self.closures.snapshot(
                            self.scopes
                                .len()
                                .saturating_mul(self.function.locals.len().saturating_add(1))
                                .saturating_add(name.len()),
                            index.span,
                        )?;
                        let Some(local) = self.lookup(name) else {
                            return Ok(None);
                        };
                        let declaration = &self.function.locals[local.0 as usize];
                        if declaration.mutable || !declaration.ty.is_integer() {
                            return Ok(None);
                        }
                        // Expand's compound assignment evaluates an index
                        // once into an immutable ordinary local. The same
                        // rule applies to any actual immutable integer value;
                        // neither a generated name nor equal syntax proves it.
                        Some(local)
                    }
                    _ => return Ok(None),
                };
                if !matches!(object.kind, CoreExprKind::Name(_))
                    && self.known_address_checked(object)?
                {
                    return Ok(None);
                }
                if let Some(binding) = binding {
                    self.closures
                        .snapshot(place.index_bindings.len().saturating_add(1), index.span)?;
                    place.index_bindings.try_reserve(1).map_err(|_| {
                        Diagnostic::limit(
                            Phase::Types,
                            "prepared index binding allocation failed",
                            Some(index.span),
                        )
                    })?;
                    place.index_bindings.push(binding);
                }
                Ok(Some(place))
            }
            // Calls, mutable/effectful indices and arbitrary dereferences must
            // retain their separate actual evaluation and access obligations.
            _ => Ok(None),
        }
    }

    pub(crate) fn prepare_copy_read(
        &mut self,
        target: &CoreExpr,
        address: ValueId,
        ty: &Type,
    ) -> R<()> {
        self.closures.snapshot_type(ty, 1, 1, target.span)?;
        if !ty.is_copy_value() {
            return Ok(());
        }
        let Some(place) = self.stable_assignment_root(target, 0)? else {
            return Ok(());
        };
        let before = self.closures.remaining_work();
        let mut remaining = before;
        inference::charge_expression_snapshot(target, self.limits, &mut remaining)?;
        self.closures.snapshot(before - remaining, target.span)?;
        self.prepared_copy_read = Some(PreparedRead {
            target: target.clone(),
            place,
            address,
            ty: ty.clone(),
        });
        Ok(())
    }

    pub(crate) fn read_prepared_copy(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<Option<(ValueId, Type)>> {
        let Some(prepared) = self.prepared_copy_read.take() else {
            return Ok(None);
        };
        let result = (|| {
            // Saved root and index LocalIds distinguish RHS shadows from
            // the target's bindings even when their names are identical.
            let place = self.stable_assignment_root(e, 0)?;
            self.closures.snapshot(
                prepared
                    .place
                    .index_bindings
                    .len()
                    .saturating_add(place.as_ref().map_or(0, |place| place.index_bindings.len()))
                    .saturating_add(1),
                e.span,
            )?;
            if place.as_ref() != Some(&prepared.place)
                || !self.same_stable_place(&prepared.target, e, 0)?
            {
                return Ok(None);
            }
            self.closures.snapshot_type(&prepared.ty, 2, 0, e.span)?;
            if let Some(expected) = expected {
                self.same(&prepared.ty, expected, e.span)?;
            }
            let value = self.value(
                InstructionKind::ReadAddress {
                    address: prepared.address,
                },
                prepared.ty.clone(),
                e.span,
            )?;
            Ok(Some((value, prepared.ty.clone())))
        })();
        // Restore on success and failure. Nested assignment lowering saves
        // this entry on its Rust call frame and installs its own RHS scope.
        self.prepared_copy_read = Some(prepared);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use zil_base::NodeId;

    fn expression(kind: CoreExprKind) -> CoreExpr {
        CoreExpr {
            id: NodeId(0),
            kind,
            span: Span::default(),
        }
    }
    fn name(name: &str) -> CoreExpr {
        expression(CoreExprKind::Name(name.into()))
    }
    fn field() -> CoreExpr {
        expression(CoreExprKind::Member {
            object: Box::new(name("state")),
            member: "value".into(),
            owner: None,
        })
    }
    fn address_type() -> Type {
        Type::Address {
            mutable: true,
            inner: Box::new(Type::Record {
                name: "State".into(),
                fields: vec![("value".into(), Type::I32)],
            }),
        }
    }
    fn builder<'a, 'b>(
        signatures: &'a BTreeMap<String, Signature>,
        limits: &'a Limits,
        state: &'b mut closures::State,
    ) -> Builder<'a, 'b> {
        Builder::allocate(
            FunctionDraft {
                dyn_instance: None,
                retained_return_sources: None,
                storage_life: None,
                call_kind: CallKind::Direct,
                resume_bound: TaskResumeBound::Local,
                reference_return_bound: None,
                callable_return_contract: None,
                callable_contract: None,
                protocol_contracts: vec![],
                id: FunctionId(0),
                name: "assignment".into(),
                foreign: None,
                environment: None,
                parameters: vec![],
                return_type: Some(Type::Unit),
                result_mode: PassingMode::Infer,
                locals: vec![],
                values: vec![],
                blocks: vec![BasicBlock {
                    id: BlockId(0),
                    instructions: vec![],
                    terminator: Terminator::Unreachable,
                }],
                entry: BlockId(0),
                span: Span::default(),
            },
            None,
            None,
            vec![],
            None,
            None,
            BTreeSet::new(),
            signatures,
            limits,
            state,
        )
    }

    #[test]
    fn nested_rhs_reads_share_target_and_preserve_one_call_in_order() {
        let signatures = BTreeMap::new();
        let limits = Limits::default();
        let mut state = closures::State::new(1, &limits);
        let mut builder = builder(&signatures, &limits, &mut state);
        builder
            .local("state", address_type(), false, Span::default())
            .unwrap();
        builder
            .local(
                "effect",
                Type::FunctionPointer {
                    parameters: vec![],
                    result: Box::new(Type::I32),
                },
                false,
                Span::default(),
            )
            .unwrap();
        let twice = expression(CoreExprKind::Binary {
            op: BinaryOp::Add,
            left: Box::new(field()),
            right: Box::new(field()),
        });
        let rhs = expression(CoreExprKind::Binary {
            op: BinaryOp::Add,
            left: Box::new(twice),
            right: Box::new(expression(CoreExprKind::Call {
                callee: Box::new(name("effect")),
                arguments: vec![],
            })),
        });
        builder.assign_address(&field(), &rhs).unwrap();
        assert!(builder.prepared_copy_read.is_none());
        let instructions = &builder.function.blocks[0].instructions;
        let writes: Vec<_> = instructions
            .iter()
            .enumerate()
            .filter_map(|(index, i)| match i.kind {
                InstructionKind::WriteAddress { address, .. } => Some((index, address)),
                _ => None,
            })
            .collect();
        assert_eq!(writes.len(), 1);
        let reads: Vec<_> = instructions
            .iter()
            .enumerate()
            .filter_map(|(index, i)| match i.kind {
                InstructionKind::ReadAddress { address } => Some((index, address)),
                _ => None,
            })
            .collect();
        assert_eq!(reads.len(), 2);
        assert!(reads.iter().all(|(_, address)| *address == writes[0].1));
        let projections: Vec<_> = instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.kind, InstructionKind::AddressField { .. }))
            .collect();
        assert_eq!(projections.len(), 1);
        let calls: Vec<_> = instructions
            .iter()
            .enumerate()
            .filter(|(_, i)| matches!(i.kind, InstructionKind::CallIndirect { .. }))
            .collect();
        assert_eq!(calls.len(), 1);
        assert!(
            projections[0].0 < reads[0].0 && reads[1].0 < calls[0].0 && calls[0].0 < writes[0].0
        );
        builder.expr(&field(), None).unwrap();
        assert_eq!(
            builder.function.blocks[0]
                .instructions
                .iter()
                .filter(|i| matches!(i.kind, InstructionKind::AddressField { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn shadows_dynamic_indices_and_failure_cannot_reuse_another_preparation() {
        let signatures = BTreeMap::new();
        let limits = Limits::default();
        let mut state = closures::State::new(1, &limits);
        let mut builder = builder(&signatures, &limits, &mut state);
        builder
            .local("state", address_type(), false, Span::default())
            .unwrap();
        let (address, _) = builder.address_place(&field(), true).unwrap();
        builder
            .prepare_copy_read(&field(), address, &Type::I32)
            .unwrap();
        builder.push_scope(Span::default()).unwrap();
        builder
            .local("state", address_type(), false, Span::default())
            .unwrap();
        assert!(
            builder
                .read_prepared_copy(&field(), None)
                .unwrap()
                .is_none()
        );
        builder.pop_scope();
        assert!(
            builder
                .assign_address(&field(), &expression(CoreExprKind::Bool(true)))
                .is_err()
        );
        assert_eq!(
            builder.prepared_copy_read.as_ref().unwrap().address,
            address
        );
        builder.prepared_copy_read = None;
        assert!(
            builder
                .assign_address(&field(), &expression(CoreExprKind::Bool(true)))
                .is_err()
        );
        assert!(builder.prepared_copy_read.is_none());
        let dynamic = expression(CoreExprKind::Index {
            object: Box::new(name("state")),
            index: Box::new(name("index")),
        });
        assert!(
            builder
                .stable_assignment_root(&dynamic, 0)
                .unwrap()
                .is_none()
        );
        let effectful = expression(CoreExprKind::Member {
            object: Box::new(expression(CoreExprKind::Call {
                callee: Box::new(name("next")),
                arguments: vec![],
            })),
            member: "value".into(),
            owner: None,
        });
        assert!(
            builder
                .stable_assignment_root(&effectful, 0)
                .unwrap()
                .is_none()
        );
        builder
            .local("changing", address_type(), true, Span::default())
            .unwrap();
        assert!(
            builder
                .stable_assignment_root(&name("changing"), 0)
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn borrowed_inline_capture_and_immutable_index_keep_their_actual_bindings() {
        let signatures = BTreeMap::new();
        let limits = Limits::default();
        let mut state = closures::State::new(1, &limits);
        let mut builder = builder(&signatures, &limits, &mut state);
        let local = builder
            .local("state", address_type(), true, Span::default())
            .unwrap();
        let Type::Address { inner, .. } = address_type() else {
            unreachable!()
        };
        builder.borrowed_captures.insert(local, *inner);
        let rhs = expression(CoreExprKind::Binary {
            op: BinaryOp::Add,
            left: Box::new(field()),
            right: Box::new(expression(CoreExprKind::Integer("1".into()))),
        });
        builder.assign_address(&field(), &rhs).unwrap();
        assert_eq!(
            builder.function.blocks[0]
                .instructions
                .iter()
                .filter(|i| matches!(i.kind, InstructionKind::AddressField { .. }))
                .count(),
            1
        );

        builder
            .local(
                "values",
                Type::Address {
                    mutable: true,
                    inner: Box::new(Type::Slice {
                        element: Box::new(Type::U8),
                    }),
                },
                false,
                Span::default(),
            )
            .unwrap();
        builder
            .local("frozen", Type::I64, false, Span::default())
            .unwrap();
        let target = expression(CoreExprKind::Index {
            object: Box::new(name("values")),
            index: Box::new(name("frozen")),
        });
        let rhs = expression(CoreExprKind::Binary {
            op: BinaryOp::Add,
            left: Box::new(target.clone()),
            right: Box::new(expression(CoreExprKind::Literal(Constant::U8(1)))),
        });
        builder.assign_address(&target, &rhs).unwrap();
        assert_eq!(
            builder.function.blocks[0]
                .instructions
                .iter()
                .filter(|i| matches!(i.kind, InstructionKind::AddressIndex { .. }))
                .count(),
            1
        );
        let (address, _) = builder.address_place(&target, true).unwrap();
        builder
            .prepare_copy_read(&target, address, &Type::U8)
            .unwrap();
        builder.push_scope(Span::default()).unwrap();
        builder
            .local("frozen", Type::I64, false, Span::default())
            .unwrap();
        assert!(builder.read_prepared_copy(&target, None).unwrap().is_none());
        builder.pop_scope();
        assert!(builder.read_prepared_copy(&target, None).unwrap().is_some());
        builder.prepared_copy_read = None;
        builder
            .local("changing_index", Type::I64, true, Span::default())
            .unwrap();
        let changing = expression(CoreExprKind::Index {
            object: Box::new(name("values")),
            index: Box::new(name("changing_index")),
        });
        assert!(
            builder
                .stable_assignment_root(&changing, 0)
                .unwrap()
                .is_none()
        );

        let rebindable = Type::Address {
            mutable: true,
            inner: Box::new(address_type()),
        };
        let local = builder
            .local("captured_pointer", rebindable, true, Span::default())
            .unwrap();
        builder.borrowed_captures.insert(local, address_type());
        assert!(
            builder
                .stable_assignment_root(&name("captured_pointer"), 0)
                .unwrap()
                .is_none()
        );
    }
}
