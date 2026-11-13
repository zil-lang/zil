//! One address layer. Pointer bytes and execution permission are separate.
use super::*;
impl Builder<'_, '_> {
    /// Extract the actual stored address, rather than a child reborrow. The
    /// independent flow kernels prove occupancy and required refill at exits.
    pub(super) fn move_out_call(
        &mut self,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        let [target] = arguments else {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "move_out requires one stored field place",
            ));
        };
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &target.kind
        else {
            return Err(error(
                "E-MOVE-OUT-PLACE",
                target.span,
                "move_out requires a stored address field",
            ));
        };
        let (storage, address_ty) = self.projection_base(object, true)?;
        let Type::Address {
            mutable: true,
            inner,
        } = address_ty
        else {
            return Err(error(
                "E-ADDRESS-PERMISSION",
                target.span,
                "move_out requires a mutable holder place",
            ));
        };
        self.closures.snapshot_type(&inner, 2, 2, target.span)?;
        let Type::BorrowedStorage {
            storage: physical,
            life,
        } = inner.as_ref()
        else {
            return Err(error(
                "E-MOVE-OUT-PLACE",
                target.span,
                "move_out requires a qualified stored address field",
            ));
        };
        self.member_owner(&inner, owner.as_ref(), target.span)?;
        let (index, ty) = aggregates::field(physical, member, target.span)?;
        let path = vec![PlaceProjection::Field(index)];
        let mode = match ty {
            Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
            Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
            _ => {
                return Err(error(
                    "E-MOVE-OUT-PLACE",
                    target.span,
                    "move_out requires a declared stored address slot",
                ));
            }
        };
        if !life
            .slots
            .iter()
            .any(|slot| slot.path == path && slot.mode == mode)
        {
            return Err(error(
                "E-MOVE-OUT-PLACE",
                target.span,
                "move_out requires a declared stored address slot",
            ));
        }
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        Ok((
            self.value(
                InstructionKind::LoadStoredBorrow {
                    storage,
                    path,
                    mode: StoredBorrowLoad::Take,
                },
                ty.clone(),
                span,
            )?,
            ty,
        ))
    }
    pub(super) fn affine_type(&self, ty: &Type) -> bool {
        fn contains_exclusive_slot(ty: &Type) -> bool {
            match ty {
                Type::BorrowedStorage { storage, life } => {
                    life.slots
                        .iter()
                        .any(|slot| slot.mode == StoredBorrowMode::ExclusiveAddress)
                        || contains_exclusive_slot(storage)
                }
                Type::Tuple(fields) => fields.iter().any(contains_exclusive_slot),
                Type::Record { fields, .. } | Type::Package { fields, .. } => fields
                    .iter()
                    .any(|(_, field)| contains_exclusive_slot(field)),
                Type::Array { element, .. } => contains_exclusive_slot(element),
                Type::Enum { variants, .. } => variants
                    .iter()
                    .any(|(_, fields)| fields.iter().any(contains_exclusive_slot)),
                Type::ErrorUnion { error, result } => {
                    contains_exclusive_slot(error) || contains_exclusive_slot(result)
                }
                _ => false,
            }
        }
        if contains_exclusive_slot(ty) {
            return true;
        }
        match ty {
            Type::MutexGuard { .. } | Type::OwnedDyn { .. } => true,
            Type::Package { .. } => true,
            Type::BorrowedStorage { storage, .. }
                if matches!(storage.as_ref(), Type::Package { .. })
                    || matches!(storage.as_ref(), Type::Tuple(_)) && owned_shapes::tuple(ty) =>
            {
                true
            }
            Type::Tuple(_) => owned_shapes::tuple(ty),
            Type::ErrorUnion { .. } => errors::owned(ty),
            Type::Enum { .. } => owned_shapes::enumeration(ty),
            ty if packages::owned_array_supported(ty) => true,
            Type::Address { mutable: true, .. } => true,
            Type::Closure(id) => {
                self.owned_closure(ty)
                    || self.closures.pending[id.0 as usize]
                        .captures
                        .iter()
                        .any(|capture| {
                            (capture.borrowed && capture.written)
                                || matches!(capture.ty, Type::Address { mutable: true, .. })
                                    && (capture.moved || capture.written)
                        })
            }
            _ => false,
        }
    }
    pub(super) fn transfer_expr(
        &mut self,
        e: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let entered = self
            .closures
            .semantic_begin(self.function.id, e.id, e.span)?;
        let result = self.transfer_expr_context(e, expected);
        self.closures.semantic_end(
            self.function.id,
            e.id,
            entered,
            result.as_ref().ok().map(|r| r.0),
            e.span,
        )?;
        result
    }
    fn transfer_expr_context(
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
        if let Some(value) = self.take_replacement_input(e, expected)? {
            return Ok(value);
        }
        if matches!(&e.kind, CoreExprKind::Name(_)) {
            return self.transfer_name(e, expected);
        }
        self.expr(e, expected)
    }
    fn transfer_name(&mut self, e: &CoreExpr, expected: Option<&Type>) -> R<(ValueId, Type)> {
        if let CoreExprKind::Name(name) = &e.kind
            && let Some(local) = self.lookup(name)
            && !self.borrowed_captures.contains_key(&local)
        {
            if let Some(value) = self.take_sum_slot(local, expected, e.span, false)? {
                return Ok(value);
            }
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
            let actual = &self.function.locals[local.0 as usize].ty;
            self.closures.array_snapshot(actual, 1, e.span)?;
            let ty = self.function.locals[local.0 as usize].ty.clone();
            // A hygienic Sequence only preserves the value already evaluated
            // for a pipeline/spread. A fresh or explicitly given owner must
            // reach its Managed parameter without becoming an ordinary alias.
            // User bindings remain descriptions unless source spells `give`.
            let sequenced_owner =
                matches!((&ty, expected), (Type::Managed(_), Some(Type::Managed(_))))
                    && self.sequenced_values.contains(&local);
            if self.affine_type(&ty)
                || self.ordinary_managed_values.contains(&local)
                || sequenced_owner
                || (self.forwarding_modes && self.factory_declared_kind(e)?.is_some())
            {
                if let Some(expected) = expected {
                    self.same(&ty, expected, e.span)?;
                }
                self.closures.array_snapshot(&ty, 1, e.span)?;
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
            return self.argument_description(e, expected);
        }
        if let Type::Address { mutable, inner } = expected {
            return self.argument_address(e, expected, *mutable, inner);
        }
        self.transfer_expr(e, Some(expected))
    }
    fn argument_description(&mut self, e: &CoreExpr, expected: &Type) -> R<(ValueId, Type)> {
        let (value, ty) = self.expr(e, None)?;
        let (value, ty) = self.resource_descriptor(value, ty, e.span)?;
        self.same(&ty, expected, e.span)?;
        Ok((value, ty))
    }
    fn argument_address(
        &mut self,
        e: &CoreExpr,
        expected: &Type,
        mutable: bool,
        inner: &Type,
    ) -> R<(ValueId, Type)> {
        if let Type::Dyn { interface } = inner
            && let CoreExprKind::Unary { op, operand } = &e.kind
            && matches!(op, UnaryOp::Borrow | UnaryOp::BorrowMut)
            && let CoreExprKind::Name(name) = &operand.kind
            && let Some(local) = self.lookup(name)
            && self
                .function
                .locals
                .get(local.0 as usize)
                .is_some_and(|decl| {
                    decl.ty
                        == (Type::OwnedDyn {
                            interface: *interface,
                        })
                })
        {
            if mutable && *op != UnaryOp::BorrowMut {
                return Err(error(
                    "E-DYN-RECEIVER",
                    e.span,
                    "shared owned dyn borrow cannot provide exclusive slot",
                ));
            }
            if mutable && !self.function.locals[local.0 as usize].mutable {
                return Err(error(
                    "E-DYN-RECEIVER",
                    e.span,
                    "exclusive owned dyn borrow needs a mutable owner local",
                ));
            }
            let ty = Type::Address {
                mutable,
                inner: Box::new(Type::Dyn {
                    interface: *interface,
                }),
            };
            let value = self.value(
                InstructionKind::BorrowDynOwned { local, mutable },
                ty.clone(),
                e.span,
            )?;
            return Ok((value, ty));
        }
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
        if mutable && !available {
            return Err(error(
                "E-ADDRESS-PERMISSION",
                e.span,
                "a shared address cannot grant exclusive access",
            ));
        }
        if let Type::Dyn { interface } = inner {
            if actual_inner.as_ref() == inner {
                return Ok((
                    self.value(
                        InstructionKind::ReborrowAddress { address, mutable },
                        expected.clone(),
                        e.span,
                    )?,
                    expected.clone(),
                ));
            }
            return self.dyn_coerce(address, &actual_inner, *interface, mutable, e.span);
        }
        self.same(inner, &actual_inner, e.span)?;
        self.closures.array_snapshot(expected, 2, e.span)?;
        Ok((
            self.value(
                InstructionKind::ReborrowAddress { address, mutable },
                expected.clone(),
                e.span,
            )?,
            expected.clone(),
        ))
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
        self.closures.array_snapshot(&ty, 1, e.span)?;
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
        if !mutable
            && stored::shared_record_child(&field, &mut self.match_work_left, span)?
            && matches!(&self.function.locals[local.0 as usize].ty,
                Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))
        {
            // Keep the actual parent shell as the projection's carrier. The
            // Record has no separate resource cell or cleanup responsibility.
            let parent_work = inference::type_snapshot_work(
                &self.function.locals[local.0 as usize].ty,
                span,
                self.limits,
            )?;
            self.closures.snapshot(parent_work, span)?;
            let parent = self.function.locals[local.0 as usize].ty.clone();
            let address = self.value(
                InstructionKind::BorrowLocal { local },
                Type::Address {
                    mutable: false,
                    inner: Box::new(parent),
                },
                span,
            )?;
            let ty = Type::Address {
                mutable: false,
                inner: Box::new(field),
            };
            return Ok((
                self.value(
                    InstructionKind::AddressField { address, index },
                    ty.clone(),
                    span,
                )?,
                ty,
            ));
        }
        let shared_owned = !mutable
            && (packages::product::supported(&field)
                || packages::product::owned_supported(&field)
                || matches!(&field, Type::Managed(payload)
                    if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value()));
        if !field.is_copy_value() && !matches!(field, Type::Closure(_)) && !shared_owned {
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
    pub(super) fn address_place(&mut self, e: &CoreExpr, mutable: bool) -> R<(ValueId, Type)> {
        match &e.kind {
            CoreExprKind::Name(name) => {
                let local = self.lookup(name).ok_or_else(|| {
                    error("E-UNKNOWN-NAME", e.span, format!("unknown value {name}"))
                })?;
                let actual = &self.function.locals[local.0 as usize].ty;
                let pure_array = !mutable
                    && matches!(actual, Type::Array { .. })
                    && !actual.is_copy_value()
                    && stored::pure_inline(actual, &mut self.match_work_left, self.limits, e.span)?;
                let address_array =
                    self.closures.array_billing || stored::shared_array_related(actual);
                if pure_array || address_array {
                    self.closures.snapshot_type(actual, 1, 0, e.span)?;
                    if address_array {
                        self.closures
                            .snapshot(self.function.locals[local.0 as usize].name.len(), e.span)?;
                    }
                }
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
                let product_head = matches!(decl.ty, Type::Package { .. });
                if product_head {
                    let work = inference::type_snapshot_work(&decl.ty, e.span, self.limits)?;
                    self.closures.snapshot(work, e.span)?;
                    if !packages::product::supported(&decl.ty) {
                        // Only the new inline Store owner enters the ordinary
                        // Product branch. Reserve both structural walks before
                        // inspecting its complete children; old accepted
                        // Product heads retain their exact previous fee.
                        self.closures.snapshot(work.saturating_mul(4), e.span)?;
                        if !(dynamic_store::contains_inline_carrier(&decl.ty)
                            || packages::has_store_fields(&decl.ty))
                            || !packages::product::owned_supported(&decl.ty)
                        {
                            return Err(unsupported(
                                e.span,
                                if mutable {
                                    "Product heads require finite complete ordinary children"
                                } else {
                                    "shared Product heads require finite complete ordinary children"
                                },
                            ));
                        }
                    }
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
                    && !stored::shared_value(&decl.ty)
                    && !pure_array
                    && !matches!(decl.ty, Type::Closure(_) | Type::OwnedDyn { .. })
                    && !resource_head
                    && !matches!(decl.ty, Type::MutexGuard { .. })
                    && !(mutable
                        && matches!(decl.ty, Type::Managed(_))
                        && (tasks::resource(&decl.ty) || registration::asynchronous(&decl.ty)))
                    && !product_head
                    && !owned_shapes::sum(&decl.ty)
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
                if pure_array || address_array {
                    // The earlier snapshot pays for decl.clone(); this one
                    // pays the separate full Address result clone below.
                    self.closures.snapshot_type(&ty, 1, 0, e.span)?;
                }
                Ok((self.value(kind, ty.clone(), e.span)?, ty))
            }
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => {
                let result = self.expr(operand, None)?;
                if !matches!(&result.1, Type::Address { inner, .. } if inner.is_address_pointee())
                    && !stored::shared_address(&result.1)
                    && !result.1.is_shared_resource_head_address()
                    && !result.1.is_package_address()
                    && !reference_patterns::sum_address(&result.1)
                {
                    return Err(error(
                        "E-OPERATOR-TYPE",
                        e.span,
                        "dereference place requires one address layer",
                    ));
                }
                Ok(result)
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => {
                if !self.indirect_place(e)
                    && let Some((local, index, field)) = self.package_place(e)?
                    && !(mutable && packages::product::owned_supported(&field))
                {
                    return self.borrow_package_field(local, index, field, mutable, e.span);
                }
                let (address, ty) = self.projection_base(object, mutable)?;
                let Type::Address { mutable, inner } = ty else {
                    unreachable!()
                };
                self.member_owner(&inner, owner.as_ref(), e.span)?;
                // This new owning-child address path retains the full field
                // type. Reserve its snapshots before field() clones it or the
                // finite Product qualifier traverses its descendants.
                if let Type::Package { fields, .. } = inner.as_ref() {
                    if mutable {
                        self.closures.snapshot(
                            fields.len().saturating_mul(member.len().saturating_add(1)),
                            e.span,
                        )?;
                    }
                    if let Some((_, field)) = fields.iter().find(|(name, _)| name == member)
                        && matches!(field, Type::Package { .. } | Type::Managed(_))
                    {
                        let work = inference::type_snapshot_work(field, e.span, self.limits)?;
                        self.closures.snapshot(work.saturating_mul(2), e.span)?;
                    }
                }
                if let Type::Record { fields, .. } = stored::physical(&inner)
                    && let Some((_, field)) = fields.iter().find(|(name, _)| name == member)
                    && !field.is_copy_value()
                    && stored::pure_inline(field, &mut self.match_work_left, self.limits, e.span)?
                {
                    self.closures.snapshot_type(field, 1, 0, e.span)?;
                }
                let (index, selected) = aggregates::field_ref(&inner, member, e.span)?;
                let record_child =
                    stored::shared_record_child(selected, &mut self.match_work_left, e.span)?
                        || stored::shared_array_shape(
                            selected,
                            &mut self.match_work_left,
                            self.limits.max_call_depth,
                            e.span,
                        )?;
                if record_child {
                    // This clone is distinct from the Address result clone.
                    self.closures.snapshot_type(selected, 1, 0, e.span)?;
                }
                let field = selected.clone();
                if mutable && record_child {
                    return Err(unsupported(
                        e.span,
                        "exclusive nested Record windows require their slot consumer",
                    ));
                }
                let borrowed_owned = (matches!(&field, Type::Managed(_))
                    && resources::supported(&field))
                    || packages::product::supported(&field)
                    || packages::product::owned_supported(&field)
                    || !mutable
                        && matches!(&field, Type::Managed(payload)
                            if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value());
                if matches!(inner.as_ref(), Type::Package { .. })
                    && !field.is_copy_value()
                    && !matches!(field, Type::Closure(_))
                    && !borrowed_owned
                {
                    return Err(unsupported(
                        e.span,
                        "Package windows only project finite Copy, concrete callback fields or qualified Product/resource heads",
                    ));
                }
                if matches!(field, Type::Closure(_)) {
                    self.check_owned_array_closure(&field, 1, e.span)?;
                }
                let ty = Type::Address {
                    mutable,
                    inner: Box::new(field),
                };
                if record_child {
                    self.closures.snapshot_type(&ty, 1, 0, e.span)?;
                } else if let Type::Address { inner, .. } = &ty
                    && !inner.is_copy_value()
                    && stored::pure_inline(inner, &mut self.match_work_left, self.limits, e.span)?
                {
                    self.closures.snapshot_type(&ty, 1, 0, e.span)?;
                }
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
                if let Type::Address { inner, .. } = &ty
                    && !inner.is_copy_value()
                    && stored::pure_inline(inner, &mut self.match_work_left, self.limits, e.span)?
                {
                    self.closures.snapshot_type(&ty, 1, 0, e.span)?;
                }
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
    pub(super) fn projection_base(
        &mut self,
        object: &CoreExpr,
        mutable: bool,
    ) -> R<(ValueId, Type)> {
        if self.known_address_checked(object)? || dynamic_store::borrow_expression(object) {
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
                self.known_address(object)
                    || dynamic_store::borrow_expression(object)
                    || self.indirect_place(object)
            }
            _ => false,
        }
    }
    fn same_stable_place(&mut self, a: &CoreExpr, b: &CoreExpr, depth: usize) -> R<bool> {
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "assignment place comparison depth exceeded",
                Some(a.span),
            ));
        }
        let span = a.span;
        self.closures.snapshot(1, span)?;
        match (&a.kind, &b.kind) {
            (CoreExprKind::Name(a), CoreExprKind::Name(b))
            | (CoreExprKind::Integer(a), CoreExprKind::Integer(b)) => {
                self.closures
                    .snapshot(a.len().saturating_add(b.len()), span)?;
                Ok(a == b)
            }
            (
                CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    operand: a,
                },
                CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    operand: b,
                },
            ) => self.same_stable_place(a, b, depth + 1),
            (
                CoreExprKind::Member {
                    object: a,
                    member: an,
                    owner: ao,
                },
                CoreExprKind::Member {
                    object: b,
                    member: bn,
                    owner: bo,
                },
            ) => {
                self.closures
                    .snapshot(an.len().saturating_add(bn.len()), a.span)?;
                for owner in [ao, bo].into_iter().flatten() {
                    self.closures.snapshot_type(owner, 1, 0, a.span)?;
                }
                Ok(an == bn && ao == bo && self.same_stable_place(a, b, depth + 1)?)
            }
            (
                CoreExprKind::Index {
                    object: a,
                    index: ai,
                },
                CoreExprKind::Index {
                    object: b,
                    index: bi,
                },
            ) => Ok(self.same_stable_place(a, b, depth + 1)?
                && self.same_stable_place(ai, bi, depth + 1)?),
            (CoreExprKind::Literal(a), CoreExprKind::Literal(b))
                if a.ty().is_integer() && b.ty().is_integer() =>
            {
                Ok(a == b)
            }
            _ => Ok(false),
        }
    }
    pub(super) fn assign_address(&mut self, target: &CoreExpr, rhs: &CoreExpr) -> R<()> {
        if self.owning_address_assignment(target, rhs)? {
            return Ok(());
        }
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
        let prepared = if let CoreExprKind::Binary { op, left, right } = &rhs.kind
            && inner.is_integer()
            && !matches!(
                op,
                BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or
            )
            && self.same_stable_place(target, left, 0)?
        {
            // The old value is read through the actual prepared target. A
            // second projected child would conflict with the pending write.
            let old = self.value(
                InstructionKind::ReadAddress { address },
                inner.as_ref().clone(),
                left.span,
            )?;
            let (right, ty) = self.expr(right, Some(&inner))?;
            self.same(&ty, &inner, rhs.span)?;
            Some((
                self.value(
                    InstructionKind::Binary {
                        op: *op,
                        left: old,
                        right,
                    },
                    inner.as_ref().clone(),
                    rhs.span,
                )?,
                inner.as_ref().clone(),
            ))
        } else {
            None
        };
        let (value, actual) = if let Some(value) = prepared {
            value
        } else {
            self.expr(rhs, Some(&inner))?
        };
        self.same(&actual, &inner, rhs.span)?;
        self.emit(
            InstructionKind::WriteAddress { address, value },
            None,
            target.span,
        )?;
        Ok(())
    }

    pub(super) fn assign_stored_slot(&mut self, target: &CoreExpr, rhs: &CoreExpr) -> R<bool> {
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &target.kind
        else {
            return Ok(false);
        };
        if !self.known_type_is(object, |ty| {
            let ty = match ty {
                Type::Address { inner, .. } => inner.as_ref(),
                _ => ty,
            };
            matches!(ty, Type::BorrowedStorage { storage, .. }
                if match storage.as_ref() {
                    Type::Record { fields, .. } | Type::Package { fields, .. } => fields.iter().any(|(name, field)| name == member && matches!(field, Type::Address { .. })),
                    _ => false,
                })
        }) {
            return Ok(false);
        }
        let (storage, address_ty) = self.projection_base(object, true)?;
        let Type::Address {
            mutable: true,
            inner,
        } = address_ty
        else {
            return Err(error(
                "E-ADDRESS-PERMISSION",
                target.span,
                "stored slot replacement requires a mutable holder",
            ));
        };
        let Type::BorrowedStorage {
            storage: physical,
            life,
        } = inner.as_ref()
        else {
            unreachable!("qualified stored assignment detection")
        };
        self.member_owner(inner.as_ref(), owner.as_ref(), target.span)?;
        let (index, field_ty) = aggregates::field(physical, member, target.span)?;
        let path = vec![PlaceProjection::Field(index)];
        let mode = match field_ty {
            Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
            Type::Address { mutable: false, .. } => StoredBorrowMode::SharedAddress,
            _ => return Ok(false),
        };
        if !life
            .slots
            .iter()
            .any(|slot| slot.path == path && slot.mode == mode)
        {
            return Err(unsupported(
                target.span,
                "stored assignment requires a declared address slot",
            ));
        }
        let (value, actual) = self.transfer_expr(rhs, Some(&field_ty))?;
        self.same(&actual, &field_ty, rhs.span)?;
        self.emit(
            InstructionKind::StoreStoredBorrow {
                storage,
                path,
                value,
            },
            None,
            target.span,
        )?;
        Ok(true)
    }
}
