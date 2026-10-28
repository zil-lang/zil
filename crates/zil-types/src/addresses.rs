//! One address layer. Pointer bytes and execution permission are separate.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn affine_type(&self, ty: &Type) -> bool {
        match ty {
            Type::Package { .. } => true,
            Type::ErrorUnion { .. } => errors::owned(ty),
            ty if packages::owned_array_supported(ty) => true,
            Type::Address { mutable: true, .. } => true,
            Type::Closure(id) => self.closures.pending[id.0 as usize]
                .captures
                .iter()
                .any(|c| {
                    matches!(
                        c.stored_type(),
                        Type::Address { mutable: true, .. }
                            | Type::Managed(_)
                            | Type::Package { .. }
                            | Type::Closure(_)
                    )
                }),
            _ => false,
        }
    }
    pub(super) fn transfer_expr(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        // An owned ordinary Value may inject into one union arm. Comparing its
        // unwrapped type against the whole union before injection would reject
        // a named Package even though a constructor of the same type works.
        if matches!(expected, Some(Type::ErrorUnion { .. })) {
            return self.expr(e, expected);
        }
        if let CoreExprKind::Name(name) = &e.kind
            && let Some(local) = self.lookup(name)
            && !self.borrowed_captures.contains_key(&local)
        {
            if self.closure_slots.contains_key(&local) {
                let ty = Type::Closure(self.closure_slots[&local]);
                if let Some(expected) = expected {
                    self.same(&ty, expected, e.span)?;
                }
                return self.take_closure_slot(local, e.span);
            }
            if let Some((index, ty)) = self.closure_captures.get(&local).cloned() {
                if let Some(expected) = expected {
                    self.same(&ty, expected, e.span)?;
                }
                return Ok((
                    self.value(InstructionKind::TakeCapture { index }, ty.clone(), e.span)?,
                    ty,
                ));
            }
            let ty = self.function.locals[local.0 as usize].ty.clone();
            // A hygienic Sequence only preserves the value already evaluated
            // for a pipeline/spread. A fresh or explicitly given owner must
            // reach its Managed parameter without becoming an ordinary alias.
            // User bindings remain descriptions unless source spells `give`.
            let sequenced_owner =
                matches!((&ty, expected), (Type::Managed(_), Some(Type::Managed(_))))
                    && self.sequenced_values.contains(&local);
            if self.affine_type(&ty) || sequenced_owner {
                if let Some(expected) = expected {
                    self.same(&ty, expected, e.span)?;
                }
                return Ok((
                    self.value(InstructionKind::TakeLocal { local }, ty.clone(), e.span)?,
                    ty,
                ));
            }
        }
        self.expr(e, expected)
    }
    pub(super) fn argument_expr(&mut self, e: &CoreExpr, expected: &Type) -> R<(ValueId, Type)> {
        if matches!(expected, Type::Desc(_)) {
            let (value, ty) = self.expr(e, None)?;
            let (value, ty) = self.resource_descriptor(value, ty, e.span)?;
            self.same(&ty, expected, e.span)?;
            return Ok((value, ty));
        }
        if let Type::Address { mutable, inner } = expected {
            let (address, actual) = self.expr(e, None)?;
            let Type::Address {
                mutable: available,
                inner: actual_inner,
            } = actual
            else {
                return Err(error(
                    "E-TYPE-MISMATCH",
                    e.span,
                    "address parameter requires an address",
                ));
            };
            if *mutable && !available {
                return Err(error(
                    "E-ADDRESS-PERMISSION",
                    e.span,
                    "a shared address cannot grant exclusive access",
                ));
            }
            self.same(inner, &actual_inner, e.span)?;
            return Ok((
                self.value(
                    InstructionKind::ReborrowAddress {
                        address,
                        mutable: *mutable,
                    },
                    expected.clone(),
                    e.span,
                )?,
                expected.clone(),
            ));
        }
        self.transfer_expr(e, Some(expected))
    }
    pub(super) fn borrow_place(&mut self, e: &CoreExpr) -> R<(ValueId, Type)> {
        self.borrow_place_mode(e, false)
    }
    pub(super) fn borrow_place_mode(&mut self, e: &CoreExpr, mutable: bool) -> R<(ValueId, Type)> {
        let (address, ty) = self.address_place(e, mutable)?;
        let Type::Address {
            mutable: available,
            inner,
        } = ty
        else {
            unreachable!()
        };
        if mutable && !available {
            return Err(error(
                "E-ADDRESS-PERMISSION",
                e.span,
                "shared address cannot be reborrowed exclusively",
            ));
        }
        let ty = Type::Address { mutable, inner };
        Ok((
            self.value(
                InstructionKind::ReborrowAddress { address, mutable },
                ty.clone(),
                e.span,
            )?,
            ty,
        ))
    }
    pub(super) fn borrow_package_field(
        &mut self,
        local: LocalId,
        index: u32,
        field: Type,
        mutable: bool,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let work = inference::type_snapshot_work(&field, span, self.limits)?;
        self.closures.snapshot(work.saturating_mul(2), span)?;
        if !field.is_copy_value() && !matches!(field, Type::Closure(_)) {
            return Err(unsupported(
                span,
                "package field addresses require a finite Copy field or concrete callback",
            ));
        }
        if mutable && !self.function.locals[local.0 as usize].mutable {
            return Err(error(
                "E-IMMUTABLE-ASSIGNMENT",
                span,
                "exclusive field borrow requires a mutable Package position",
            ));
        }
        self.check_owned_array_closure(&field, 1, span)?;
        let ty = Type::Address {
            mutable,
            inner: Box::new(field),
        };
        Ok((
            self.value(
                InstructionKind::BorrowPackageField {
                    local,
                    index,
                    mutable,
                },
                ty.clone(),
                span,
            )?,
            ty,
        ))
    }
    /// Locate a write target before evaluating its RHS. Projections retain the
    /// same authority; only an explicit ReborrowAddress creates a child window.
    fn address_place(&mut self, e: &CoreExpr, mutable: bool) -> R<(ValueId, Type)> {
        match &e.kind {
            CoreExprKind::Name(name) => {
                let local = self.lookup(name).ok_or_else(|| {
                    error("E-UNKNOWN-NAME", e.span, format!("unknown value {name}"))
                })?;
                let decl = self.function.locals[local.0 as usize].clone();
                if self.closure_slots.contains_key(&local) {
                    return self.borrow_closure_slot(local, mutable, e.span);
                }
                if let Some(index) = self.stored_copy_captures.get(&local).copied() {
                    if mutable && !decl.mutable {
                        return Err(error(
                            "E-IMMUTABLE-ASSIGNMENT",
                            e.span,
                            "captured Copy field has no exclusive access",
                        ));
                    }
                    let ty = Type::Address {
                        mutable,
                        inner: Box::new(decl.ty),
                    };
                    return Ok((
                        self.value(
                            InstructionKind::BorrowCapture { index, mutable },
                            ty.clone(),
                            e.span,
                        )?,
                        ty,
                    ));
                }
                if let Some((index, inner)) = self.closure_captures.get(&local).cloned() {
                    let available = matches!(decl.ty, Type::Address { mutable: true, .. });
                    if mutable && !available {
                        return Err(error(
                            "E-IMMUTABLE-ASSIGNMENT",
                            e.span,
                            "captured closure has no exclusive access",
                        ));
                    }
                    let ty = Type::Address {
                        mutable,
                        inner: Box::new(inner),
                    };
                    return Ok((
                        self.value(
                            InstructionKind::BorrowCapture { index, mutable },
                            ty.clone(),
                            e.span,
                        )?,
                        ty,
                    ));
                }
                if self.borrowed_captures.contains_key(&local) {
                    let Type::Address {
                        mutable: available, ..
                    } = &decl.ty
                    else {
                        unreachable!()
                    };
                    if mutable && !available {
                        return Err(error("E-IMMUTABLE-ASSIGNMENT", e.span, "capture is shared"));
                    }
                    return Ok((
                        self.value(InstructionKind::Load(local), decl.ty.clone(), e.span)?,
                        decl.ty,
                    ));
                }
                let resource_head = !mutable
                    && matches!(&decl.ty, Type::Managed(payload)
                        if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value());
                if resource_head && self.managed_captures.contains_key(&local) {
                    return Err(unsupported(
                        e.span,
                        "borrowing a captured resource head requires an explicit environment-head protocol",
                    ));
                }
                if !decl.ty.is_copy_value()
                    && !matches!(decl.ty, Type::Closure(_))
                    && !resource_head
                {
                    return Err(unsupported(
                        e.span,
                        "address targets must be finite Copy values or actual shared user-resource heads; descriptors and nested addresses are not head positions",
                    ));
                }
                if mutable && !decl.mutable {
                    return Err(error(
                        "E-IMMUTABLE-ASSIGNMENT",
                        e.span,
                        "exclusive borrow requires a mutable position",
                    ));
                }
                let ty = Type::Address {
                    mutable,
                    inner: Box::new(decl.ty),
                };
                let kind = if mutable {
                    InstructionKind::BorrowLocalMut { local }
                } else {
                    InstructionKind::BorrowLocal { local }
                };
                Ok((self.value(kind, ty.clone(), e.span)?, ty))
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let result = self.expr(operand, None)?;
                if !matches!(&result.1, Type::Address { inner, .. } if inner.is_address_pointee())
                    && !result.1.is_shared_resource_head_address()
                    && !result.1.is_package_address()
                {
                    return Err(error(
                        "E-OPERATOR-TYPE",
                        e.span,
                        "dereference place requires one address layer",
                    ));
                }
                Ok(result)
            }
            CoreExprKind::Member { object, member } => {
                if let Some((local, index, field)) = self.package_place(e)? {
                    return self.borrow_package_field(local, index, field, mutable, e.span);
                }
                let (address, ty) = self.projection_base(object, mutable)?;
                let Type::Address { mutable, inner } = ty else {
                    unreachable!()
                };
                let (index, field) = aggregates::field(&inner, member, e.span)?;
                if matches!(inner.as_ref(), Type::Package { .. })
                    && !field.is_copy_value()
                    && !matches!(field, Type::Closure(_))
                {
                    return Err(unsupported(
                        e.span,
                        "Store element windows only project finite Copy or concrete callback fields",
                    ));
                }
                if matches!(field, Type::Closure(_)) {
                    self.check_owned_array_closure(&field, 1, e.span)?;
                }
                let ty = Type::Address {
                    mutable,
                    inner: Box::new(field),
                };
                Ok((
                    self.value(
                        InstructionKind::AddressField { address, index },
                        ty.clone(),
                        e.span,
                    )?,
                    ty,
                ))
            }
            CoreExprKind::Index { object, index } => {
                if let Some((local, index, element @ Type::Closure(_))) =
                    self.owned_array_place(e)?
                {
                    if mutable && !self.function.locals[local.0 as usize].mutable {
                        return Err(error(
                            "E-IMMUTABLE-ASSIGNMENT",
                            e.span,
                            "exclusive callback element borrow requires a mutable array",
                        ));
                    }
                    let ty = Type::Address {
                        mutable,
                        inner: Box::new(element),
                    };
                    return Ok((
                        self.value(
                            InstructionKind::BorrowOwnedArrayElement {
                                local,
                                index,
                                mutable,
                            },
                            ty.clone(),
                            e.span,
                        )?,
                        ty,
                    ));
                }
                let (address, ty) = self.projection_base(object, mutable)?;
                let Type::Address { mutable, inner } = ty else {
                    unreachable!()
                };
                let (Type::Array { element, .. } | Type::Slice { element }) = *inner else {
                    return Err(error(
                        "E-INDEX-TYPE",
                        e.span,
                        "address index requires a finite array or borrowed slice",
                    ));
                };
                let (index, index_type) = self.expr(index, None)?;
                if !index_type.is_integer() {
                    return Err(error(
                        "E-INDEX-TYPE",
                        e.span,
                        "array index must be an integer",
                    ));
                }
                let ty = Type::Address {
                    mutable,
                    inner: element,
                };
                Ok((
                    self.value(
                        InstructionKind::AddressIndex { address, index },
                        ty.clone(),
                        e.span,
                    )?,
                    ty,
                ))
            }
            CoreExprKind::Slice { object, start, end } => {
                // Locate the base before either endpoint. The live base value
                // keeps its authority through endpoint calls; Verify checks
                // any conflicting effects before the final child reborrow.
                let (address, ty) = self.projection_base(object, mutable)?;
                let Type::Address {
                    mutable: available,
                    inner,
                } = ty
                else {
                    unreachable!()
                };
                if mutable && !available {
                    return Err(error(
                        "E-ADDRESS-PERMISSION",
                        e.span,
                        "shared slice cannot be reborrowed exclusively",
                    ));
                }
                let (Type::Array { element, .. } | Type::Slice { element }) = *inner else {
                    return Err(error(
                        "E-SLICE-TYPE",
                        e.span,
                        "range borrow requires a fixed array or an existing slice",
                    ));
                };
                if !element.is_copy_value() {
                    return Err(unsupported(
                        e.span,
                        "slice elements require finite Copy values",
                    ));
                }
                let mut endpoint = |value: &Option<Box<CoreExpr>>| -> R<Option<ValueId>> {
                    value
                        .as_ref()
                        .map(|expression| {
                            let (id, ty) = self.expr(expression, None)?;
                            if !ty.is_integer() {
                                return Err(error(
                                    "E-SLICE-BOUND-TYPE",
                                    expression.span,
                                    "slice endpoint must be an integer",
                                ));
                            }
                            Ok(id)
                        })
                        .transpose()
                };
                let start = endpoint(start)?;
                let end = endpoint(end)?;
                let ty = Type::Address {
                    mutable: available,
                    inner: Box::new(Type::Slice { element }),
                };
                Ok((
                    self.value(
                        InstructionKind::AddressSlice {
                            address,
                            start,
                            end,
                        },
                        ty.clone(),
                        e.span,
                    )?,
                    ty,
                ))
            }
            _ => Err(unsupported(
                e.span,
                "address requires a local, dereference, field, index or range place",
            )),
        }
    }
    fn projection_base(&mut self, object: &CoreExpr, mutable: bool) -> R<(ValueId, Type)> {
        if matches!(self.known_type(object), Some(Type::Address { .. }))
            || dynamic_store::borrow_expression(object)
        {
            self.expr(object, None)
        } else {
            self.address_place(object, mutable)
        }
    }
    pub(super) fn indirect_place(&self, e: &CoreExpr) -> bool {
        match &e.kind {
            CoreExprKind::Name(name) => self
                .lookup(name)
                .is_some_and(|l| self.borrowed_captures.contains_key(&l)),
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                ..
            } => true,
            CoreExprKind::Member { object, .. } | CoreExprKind::Index { object, .. } => {
                matches!(self.known_type(object), Some(Type::Address { .. }))
                    || dynamic_store::borrow_expression(object)
                    || self.indirect_place(object)
            }
            _ => false,
        }
    }
    pub(super) fn assign_address(&mut self, target: &CoreExpr, rhs: &CoreExpr) -> R<()> {
        let (address, ty) = self.address_place(target, true)?;
        let Type::Address {
            mutable: true,
            inner,
        } = ty
        else {
            return Err(error(
                "E-ADDRESS-PERMISSION",
                target.span,
                "indirect write requires an exclusive address",
            ));
        };
        if matches!(inner.as_ref(), Type::Closure(_)) {
            return Err(unsupported(
                target.span,
                "Store windows cannot replace an owning callback field",
            ));
        }
        let (value, actual) = self.expr(rhs, Some(&inner))?;
        self.same(&actual, &inner, rhs.span)?;
        self.emit(
            InstructionKind::WriteAddress { address, value },
            None,
            target.span,
        )?;
        Ok(())
    }
}
