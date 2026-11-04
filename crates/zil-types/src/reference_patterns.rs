//! Reference bindings are addresses of real storage, never copies of a payload.
use super::*;
pub(super) fn has_reference(p: &CorePattern) -> bool {
    match &p.kind {
        CorePatternKind::Reference { .. } => true,
        CorePatternKind::Tuple(fields) | CorePatternKind::Variant { fields, .. } => {
            fields.iter().any(has_reference)
        }
        _ => false,
    }
}
fn needs_mutable(p: &CorePattern) -> bool {
    match &p.kind {
        CorePatternKind::Reference { mutable, pattern } => *mutable || needs_mutable(pattern),
        CorePatternKind::Tuple(fs) | CorePatternKind::Variant { fields: fs, .. } => {
            fs.iter().any(needs_mutable)
        }
        _ => false,
    }
}
impl Builder<'_, '_> {
    fn inline_pattern_place(&mut self, e: &CoreExpr, mutable: bool) -> R<Option<(ValueId, Type)>> {
        let mut members = Vec::new();
        let mut cursor = e;
        while let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &cursor.kind
        {
            if members.len() >= self.limits.max_call_depth {
                return Err(Diagnostic::limit(
                    Phase::Types,
                    "field pattern path depth",
                    Some(e.span),
                ));
            }
            self.closures.snapshot(1, e.span)?;
            members.push((member.as_str(), owner.as_ref(), cursor.span));
            cursor = object;
        }
        let CoreExprKind::Name(name) = &cursor.kind else {
            return Ok(None);
        };
        if members.is_empty() {
            return Ok(None);
        }
        members.reverse();
        let Some(local) = self.lookup(name) else {
            return Ok(None);
        };
        if self.stored_copy_captures.contains_key(&local)
            || self.borrowed_captures.contains_key(&local)
            || self.closure_captures.contains_key(&local)
        {
            return Ok(None);
        }
        let declaration = &self.function.locals[local.0 as usize];
        let work = inference::type_snapshot_work(&declaration.ty, e.span, self.limits)?;
        self.closures.snapshot(work.saturating_mul(2), e.span)?;
        if !declaration.ty.is_copy_value() {
            return Ok(None);
        }
        let mut ty = declaration.ty.clone();
        let available = declaration.mutable;
        let mut path = Vec::new();
        for (member, owner, span) in members {
            self.member_owner(&ty, owner, span)?;
            let (index, next) = aggregates::field(&ty, member, span)?;
            path.push(index);
            ty = next;
        }
        if mutable && !available {
            return Err(error(
                "E-IMMUTABLE-ASSIGNMENT",
                e.span,
                "exclusive field pattern requires mutable storage",
            ));
        }
        let t = Type::Address {
            mutable,
            inner: Box::new(ty),
        };
        let v = self.value(
            InstructionKind::BorrowFieldLocal {
                local,
                path,
                mutable,
            },
            t.clone(),
            e.span,
        )?;
        Ok(Some((v, t)))
    }
    pub(super) fn reference_binding(
        &mut self,
        p: &CorePattern,
        mutable: bool,
        expected: Option<&Type>,
        e: &CoreExpr,
    ) -> R<()> {
        let place = if matches!(
            e.kind,
            CoreExprKind::Name(_)
                | CoreExprKind::Member { .. }
                | CoreExprKind::Index { .. }
                | CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    ..
                }
        ) {
            e.clone()
        } else {
            let (v, t) = self.transfer_expr(e, expected)?;
            let name = format!("$pattern340${}", self.function.locals.len());
            let local = self.local(&name, t, true, e.span)?;
            self.store(local, v, e.span)?;
            CoreExpr {
                kind: CoreExprKind::Name(name),
                ..e.clone()
            }
        };
        let ty = self.known_type(&place).ok_or_else(|| {
            error(
                "E-PATTERN-TYPE",
                place.span,
                "reference pattern needs a known real place",
            )
        })?;
        if let Some(expected) = expected {
            self.same(&ty, expected, place.span)?;
        }
        if !self.irrefutable_pattern(p, &ty)? {
            return Err(error(
                "E-PATTERN-REFUTABLE",
                p.span,
                "ordinary binding requires an irrefutable pattern",
            ));
        }
        let mut bindings = Vec::new();
        self.binding_place(p, &place, expected, &mut bindings)?;
        for (name, value, ty, span) in bindings {
            let local = self.local(&name, ty, mutable, span)?;
            self.store(local, value, span)?;
        }
        Ok(())
    }
    fn binding_place(
        &mut self,
        p: &CorePattern,
        e: &CoreExpr,
        expected: Option<&Type>,
        bindings: &mut Vec<(String, ValueId, Type, Span)>,
    ) -> R<()> {
        self.closures.snapshot(1, p.span)?;
        match &p.kind {
            CorePatternKind::Reference { mutable, pattern } => {
                let (value, ty) = if let Some(place) = self.inline_pattern_place(e, *mutable)? {
                    place
                } else {
                    self.borrow_place_mode(e, *mutable)?
                };
                let Type::Address { inner, .. } = &ty else {
                    unreachable!()
                };
                if let Some(wanted) = expected {
                    self.same(inner, wanted, e.span)?;
                }
                let fail = self.current.unwrap();
                self.pattern_test(pattern, value, &ty, fail, bindings)
            }
            CorePatternKind::Tuple(fields) => {
                let ty = self.known_type(e).ok_or_else(|| {
                    error(
                        "E-PATTERN-TYPE",
                        e.span,
                        "tuple reference pattern needs a known tuple place",
                    )
                })?;
                let Type::Tuple(types) = &ty else {
                    return Err(error(
                        "E-MATCH-TYPE",
                        p.span,
                        "tuple pattern requires a tuple",
                    ));
                };
                if fields.len() != types.len() {
                    return Err(error(
                        "E-MATCH-ARITY",
                        p.span,
                        "tuple pattern arity mismatch",
                    ));
                }
                for (index, (p, ty)) in fields.iter().zip(types).enumerate() {
                    let child = CoreExpr {
                        kind: CoreExprKind::Member {
                            object: Box::new(e.clone()),
                            member: index.to_string(),
                            owner: None,
                        },
                        ..e.clone()
                    };
                    self.binding_place(p, &child, Some(ty), bindings)?;
                }
                Ok(())
            }
            CorePatternKind::Name(_) | CorePatternKind::Wildcard => {
                let (v, t) = if let Some((address, Type::Address { inner, .. })) =
                    self.inline_pattern_place(e, false)?
                {
                    let t = *inner;
                    let v =
                        self.value(InstructionKind::ReadAddress { address }, t.clone(), e.span)?;
                    (v, t)
                } else {
                    self.expr(e, expected)?
                };
                let fail = self.current.unwrap();
                self.pattern_test(p, v, &t, fail, bindings)
            }
            _ => Err(error(
                "E-PATTERN-REFUTABLE",
                p.span,
                "ordinary binding requires an irrefutable pattern",
            )),
        }
    }
    pub(super) fn condition_uses_place(&self, p: &CorePattern, e: &CoreExpr) -> bool {
        has_reference(p)
            || matches!(self.known_type(e), Some(ref t) if owned_shapes::enumeration(t))
            || matches!(
                &e.kind,
                CoreExprKind::Unary {
                    op: UnaryOp::Dereference,
                    ..
                }
            ) && matches!(self.known_type(e), Some(Type::Enum { .. } | Type::Tuple(_)))
    }
    pub(super) fn reference_condition(
        &mut self,
        p: &CorePattern,
        e: &CoreExpr,
        fail: BlockId,
    ) -> R<(bool, Vec<(String, ValueId, Type, Span)>)> {
        let mut bindings = Vec::new();
        if let CoreExprKind::Unary {
            op: UnaryOp::Dereference,
            operand,
        } = &e.kind
        {
            let (address, t) = self.expr(operand, None)?;
            let Type::Address { mutable, inner } = t else {
                return Err(error(
                    "E-PATTERN-TYPE",
                    e.span,
                    "borrowed condition requires an address",
                ));
            };
            let irrefutable = self.irrefutable_pattern(p, &inner)?;
            self.reference_address_test(
                p,
                address,
                &mut Vec::new(),
                &inner,
                mutable,
                fail,
                &mut bindings,
            )?;
            return Ok((irrefutable, bindings));
        }
        if matches!(
            &e.kind,
            CoreExprKind::Member { .. } | CoreExprKind::Index { .. }
        ) {
            let mutable = needs_mutable(p);
            let (address, t) = if let Some(place) = self.inline_pattern_place(e, mutable)? {
                place
            } else {
                self.borrow_place_mode(e, mutable)?
            };
            let Type::Address { inner, .. } = t else {
                unreachable!()
            };
            let irrefutable = self.irrefutable_pattern(p, &inner)?;
            self.reference_address_test(
                p,
                address,
                &mut Vec::new(),
                &inner,
                mutable,
                fail,
                &mut bindings,
            )?;
            return Ok((irrefutable, bindings));
        }
        let (root, ty) = if let CoreExprKind::Name(name) = &e.kind {
            let local = self
                .lookup(name)
                .ok_or_else(|| error("E-UNKNOWN-NAME", e.span, "unknown condition place"))?;
            (local, self.function.locals[local.0 as usize].ty.clone())
        } else {
            let (v, t) = self.transfer_expr(e, None)?;
            let name = format!("$condition340${}", self.function.locals.len());
            let local = self.local(&name, t.clone(), true, e.span)?;
            self.store(local, v, e.span)?;
            (local, t)
        };
        let irrefutable = self.irrefutable_pattern(p, &ty)?;
        self.reference_local_test(p, root, &mut Vec::new(), &ty, fail, &mut bindings)?;
        Ok((irrefutable, bindings))
    }
    pub(super) fn reference_match(
        &mut self,
        e: &CoreExpr,
        arms: &[CoreMatchArm],
        span: Span,
    ) -> R<()> {
        if let CoreExprKind::Unary {
            op: UnaryOp::Dereference,
            operand,
        } = &e.kind
        {
            return self.reference_address_match(operand, arms, span);
        }
        if matches!(
            e.kind,
            CoreExprKind::Member { .. } | CoreExprKind::Index { .. }
        ) {
            let mutable = arms.iter().any(|a| needs_mutable(&a.pattern));
            let (address, ty) = if let Some(place) = self.inline_pattern_place(e, mutable)? {
                place
            } else {
                self.borrow_place_mode(e, mutable)?
            };
            return self.finish_reference_address_match(address, ty, arms, span);
        }
        let (root, ty) = if let CoreExprKind::Name(name) = &e.kind {
            let local = self
                .lookup(name)
                .ok_or_else(|| error("E-UNKNOWN-NAME", e.span, "unknown match place"))?;
            (local, self.function.locals[local.0 as usize].ty.clone())
        } else {
            let (v, t) = self.transfer_expr(e, None)?;
            let name = format!("$match340${}", self.function.locals.len());
            let local = self.local(&name, t.clone(), true, e.span)?;
            self.store(local, v, e.span)?;
            (local, t)
        };
        self.check_match_coverage(&ty, arms, span)?;
        let mut ends = Vec::new();
        for arm in arms {
            let fail = self.block_id()?;
            let mut bindings = Vec::new();
            self.reference_local_test(
                &arm.pattern,
                root,
                &mut Vec::new(),
                &ty,
                fail,
                &mut bindings,
            )?;
            self.push_scope(arm.span)?;
            for (name, v, t, span) in bindings {
                let local = self.local(&name, t, false, span)?;
                self.store(local, v, span)?;
            }
            self.block(&arm.body)?;
            if self.current.is_some() {
                self.end_scopes(self.scopes.len() - 1, arm.span)?;
            }
            if let Some(end) = self.current.take() {
                ends.push(end)
            }
            self.pop_scope();
            self.current = Some(fail);
        }
        self.terminate(Terminator::Trap);
        if !ends.is_empty() {
            let join = self.block_id()?;
            for end in ends {
                self.function.blocks[end.0 as usize].terminator = Terminator::Jump(join)
            }
            self.current = Some(join)
        }
        Ok(())
    }
    fn reference_local_test(
        &mut self,
        p: &CorePattern,
        root: LocalId,
        path: &mut Vec<VariantProjection>,
        ty: &Type,
        fail: BlockId,
        bindings: &mut Vec<(String, ValueId, Type, Span)>,
    ) -> R<()> {
        self.closures
            .snapshot(path.len().saturating_add(1), p.span)?;
        match &p.kind {
            CorePatternKind::Tuple(_) if matches!(ty, Type::Tuple(_)) => {
                if path.is_empty() {
                    return self.reference_inline_local_test(
                        p,
                        root,
                        &mut Vec::new(),
                        ty,
                        fail,
                        bindings,
                    );
                }
                let mutable = needs_mutable(p);
                if mutable && !self.function.locals[root.0 as usize].mutable {
                    return Err(error(
                        "E-IMMUTABLE-ASSIGNMENT",
                        p.span,
                        "exclusive tuple pattern requires mutable storage",
                    ));
                }
                let t = Type::Address {
                    mutable,
                    inner: Box::new(ty.clone()),
                };
                let address = self.value(
                    InstructionKind::BorrowVariantLocal {
                        local: root,
                        path: path.clone(),
                        mutable,
                    },
                    t,
                    p.span,
                )?;
                self.reference_address_test(
                    p,
                    address,
                    &mut Vec::new(),
                    ty,
                    mutable,
                    fail,
                    bindings,
                )
            }
            CorePatternKind::Reference { mutable, pattern } => {
                if !matches!(
                    pattern.kind,
                    CorePatternKind::Name(_) | CorePatternKind::Wildcard
                ) {
                    return Err(error(
                        "E-PATTERN-REFERENCE",
                        p.span,
                        "reference binding requires a name or discard",
                    ));
                }
                if *mutable && !self.function.locals[root.0 as usize].mutable {
                    return Err(error(
                        "E-IMMUTABLE-ASSIGNMENT",
                        p.span,
                        "exclusive pattern requires a mutable place",
                    ));
                }
                let t = Type::Address {
                    mutable: *mutable,
                    inner: Box::new(ty.clone()),
                };
                let kind = if path.is_empty() {
                    if *mutable {
                        InstructionKind::BorrowLocalMut { local: root }
                    } else {
                        InstructionKind::BorrowLocal { local: root }
                    }
                } else {
                    InstructionKind::BorrowVariantLocal {
                        local: root,
                        path: path.clone(),
                        mutable: *mutable,
                    }
                };
                let v = self.value(kind, t.clone(), p.span)?;
                self.pattern_test(pattern, v, &t, fail, bindings)
            }
            CorePatternKind::Variant {
                variant, fields, ..
            } => {
                let Type::Enum { variants, .. } = ty else {
                    unreachable!("coverage checked")
                };
                let (index, (_, types)) = variants
                    .iter()
                    .enumerate()
                    .find(|(_, (n, _))| n == variant)
                    .unwrap();
                let tag = self.value(
                    InstructionKind::VariantTagLocal {
                        local: root,
                        path: path.clone(),
                    },
                    Type::I32,
                    p.span,
                )?;
                self.pattern_branch(tag, Constant::I32(index as i32), fail, p.span)?;
                for (field, (p, ty)) in fields.iter().zip(types).enumerate() {
                    path.push(VariantProjection {
                        variant: index as u32,
                        index: field as u32,
                    });
                    self.reference_local_test(p, root, path, ty, fail, bindings)?;
                    path.pop();
                }
                Ok(())
            }
            CorePatternKind::Wildcard => Ok(()),
            _ if ty.is_copy_value() => {
                let kind = if path.is_empty() {
                    InstructionKind::Load(root)
                } else {
                    InstructionKind::ReadVariantLocal {
                        local: root,
                        path: path.clone(),
                    }
                };
                let v = self.value(kind, ty.clone(), p.span)?;
                self.pattern_test(p, v, ty, fail, bindings)
            }
            _ => Err(error(
                "E-PATTERN-MOVE-BORROW",
                p.span,
                "a borrowed match payload requires an explicit reference or discard",
            )),
        }
    }
}

pub(super) fn sum_address(ty: &Type) -> bool {
    matches!(ty,Type::Address{inner,..} if matches!(inner.as_ref(),Type::Enum{..}) && owned_shapes::value(inner))
}
impl Builder<'_, '_> {
    fn reference_address_match(
        &mut self,
        operand: &CoreExpr,
        arms: &[CoreMatchArm],
        span: Span,
    ) -> R<()> {
        let (address, t) = self.expr(operand, None)?;
        self.finish_reference_address_match(address, t, arms, span)
    }
    fn finish_reference_address_match(
        &mut self,
        address: ValueId,
        t: Type,
        arms: &[CoreMatchArm],
        span: Span,
    ) -> R<()> {
        let Type::Address { mutable, inner } = t else {
            return Err(error(
                "E-MATCH-TYPE",
                span,
                "borrowed match requires an address",
            ));
        };
        self.check_match_coverage(&inner, arms, span)?;
        let mut ends = Vec::new();
        for arm in arms {
            let fail = self.block_id()?;
            let mut bindings = Vec::new();
            self.reference_address_test(
                &arm.pattern,
                address,
                &mut Vec::new(),
                &inner,
                mutable,
                fail,
                &mut bindings,
            )?;
            self.push_scope(arm.span)?;
            for (name, v, t, s) in bindings {
                let l = self.local(&name, t, false, s)?;
                self.store(l, v, s)?
            }
            self.block(&arm.body)?;
            if self.current.is_some() {
                self.end_scopes(self.scopes.len() - 1, arm.span)?
            }
            if let Some(end) = self.current.take() {
                ends.push(end)
            }
            self.pop_scope();
            self.current = Some(fail);
        }
        self.terminate(Terminator::Trap);
        if !ends.is_empty() {
            let join = self.block_id()?;
            for end in ends {
                self.function.blocks[end.0 as usize].terminator = Terminator::Jump(join)
            }
            self.current = Some(join)
        }
        Ok(())
    }
    // Keep the real place, active variant path and publication destinations explicit.
    #[allow(clippy::too_many_arguments)]
    fn reference_address_test(
        &mut self,
        p: &CorePattern,
        address: ValueId,
        path: &mut Vec<VariantProjection>,
        ty: &Type,
        available: bool,
        fail: BlockId,
        bindings: &mut Vec<(String, ValueId, Type, Span)>,
    ) -> R<()> {
        self.closures.snapshot(path.len() + 1, p.span)?;
        match &p.kind {
            CorePatternKind::Wildcard => Ok(()),
            CorePatternKind::Tuple(fields) if matches!(ty, Type::Tuple(_)) => {
                let Type::Tuple(types) = ty else {
                    unreachable!()
                };
                let base = if path.is_empty() {
                    address
                } else {
                    let mutable = needs_mutable(p);
                    if mutable && !available {
                        return Err(error(
                            "E-ADDRESS-PERMISSION",
                            p.span,
                            "exclusive tuple pattern requires mutable input",
                        ));
                    }
                    let t = Type::Address {
                        mutable,
                        inner: Box::new(ty.clone()),
                    };
                    self.value(
                        InstructionKind::BorrowVariantAddress {
                            address,
                            path: path.clone(),
                            mutable,
                        },
                        t,
                        p.span,
                    )?
                };
                let writable = if path.is_empty() {
                    available
                } else {
                    needs_mutable(p)
                };
                for (index, (field, ty)) in fields.iter().zip(types).enumerate() {
                    let t = Type::Address {
                        mutable: writable,
                        inner: Box::new(ty.clone()),
                    };
                    let pointer = self.value(
                        InstructionKind::AddressField {
                            address: base,
                            index: index as u32,
                        },
                        t,
                        field.span,
                    )?;
                    self.reference_address_test(
                        field,
                        pointer,
                        &mut Vec::new(),
                        ty,
                        writable,
                        fail,
                        bindings,
                    )?;
                }
                Ok(())
            }
            CorePatternKind::Variant {
                variant, fields, ..
            } => {
                let Type::Enum { variants, .. } = ty else {
                    unreachable!("coverage checked")
                };
                let (index, (_, types)) = variants
                    .iter()
                    .enumerate()
                    .find(|(_, (n, _))| n == variant)
                    .unwrap();
                let tag = self.value(
                    InstructionKind::VariantTagAddress {
                        address,
                        path: path.clone(),
                    },
                    Type::I32,
                    p.span,
                )?;
                self.pattern_branch(tag, Constant::I32(index as i32), fail, p.span)?;
                for (field, (p, t)) in fields.iter().zip(types).enumerate() {
                    path.push(VariantProjection {
                        variant: index as u32,
                        index: field as u32,
                    });
                    self.reference_address_test(p, address, path, t, available, fail, bindings)?;
                    path.pop();
                }
                Ok(())
            }
            CorePatternKind::Reference { mutable, pattern } => {
                if *mutable && !available {
                    return Err(error(
                        "E-ADDRESS-PERMISSION",
                        p.span,
                        "shared input cannot grant an exclusive pattern",
                    ));
                }
                let t = Type::Address {
                    mutable: *mutable,
                    inner: Box::new(ty.clone()),
                };
                let kind = if path.is_empty() {
                    InstructionKind::ReborrowAddress {
                        address,
                        mutable: *mutable,
                    }
                } else {
                    InstructionKind::BorrowVariantAddress {
                        address,
                        path: path.clone(),
                        mutable: *mutable,
                    }
                };
                let v = self.value(kind, t.clone(), p.span)?;
                self.pattern_test(pattern, v, &t, fail, bindings)
            }
            _ if ty.is_copy_value() => {
                let t = Type::Address {
                    mutable: false,
                    inner: Box::new(ty.clone()),
                };
                let pointer = if path.is_empty() {
                    address
                } else {
                    self.value(
                        InstructionKind::BorrowVariantAddress {
                            address,
                            path: path.clone(),
                            mutable: false,
                        },
                        t,
                        p.span,
                    )?
                };
                let value = self.value(
                    InstructionKind::ReadAddress { address: pointer },
                    ty.clone(),
                    p.span,
                )?;
                self.pattern_test(p, value, ty, fail, bindings)
            }
            _ => Err(error(
                "E-PATTERN-MOVE-BORROW",
                p.span,
                "borrowed payload cannot transfer its owner's responsibility",
            )),
        }
    }
}

impl Builder<'_, '_> {
    fn reference_inline_local_test(
        &mut self,
        p: &CorePattern,
        root: LocalId,
        path: &mut Vec<u32>,
        ty: &Type,
        fail: BlockId,
        bindings: &mut Vec<(String, ValueId, Type, Span)>,
    ) -> R<()> {
        self.closures
            .snapshot(path.len().saturating_add(1), p.span)?;
        match &p.kind {
            CorePatternKind::Wildcard => Ok(()),
            CorePatternKind::Tuple(fields) => {
                let Type::Tuple(types) = ty else {
                    unreachable!("covered tuple")
                };
                for (index, (field, t)) in fields.iter().zip(types).enumerate() {
                    path.push(index as u32);
                    self.reference_inline_local_test(field, root, path, t, fail, bindings)?;
                    path.pop();
                }
                Ok(())
            }
            CorePatternKind::Reference { mutable, pattern } => {
                if *mutable && !self.function.locals[root.0 as usize].mutable {
                    return Err(error(
                        "E-IMMUTABLE-ASSIGNMENT",
                        p.span,
                        "exclusive field pattern requires mutable storage",
                    ));
                }
                let t = Type::Address {
                    mutable: *mutable,
                    inner: Box::new(ty.clone()),
                };
                let kind = if path.is_empty() {
                    if *mutable {
                        InstructionKind::BorrowLocalMut { local: root }
                    } else {
                        InstructionKind::BorrowLocal { local: root }
                    }
                } else {
                    InstructionKind::BorrowFieldLocal {
                        local: root,
                        path: path.clone(),
                        mutable: *mutable,
                    }
                };
                let value = self.value(kind, t.clone(), p.span)?;
                self.pattern_test(pattern, value, &t, fail, bindings)
            }
            CorePatternKind::Variant { .. } => {
                let mutable = needs_mutable(p);
                if mutable && !self.function.locals[root.0 as usize].mutable {
                    return Err(error(
                        "E-IMMUTABLE-ASSIGNMENT",
                        p.span,
                        "exclusive nested pattern requires mutable storage",
                    ));
                }
                let t = Type::Address {
                    mutable,
                    inner: Box::new(ty.clone()),
                };
                let kind = if path.is_empty() {
                    if mutable {
                        InstructionKind::BorrowLocalMut { local: root }
                    } else {
                        InstructionKind::BorrowLocal { local: root }
                    }
                } else {
                    InstructionKind::BorrowFieldLocal {
                        local: root,
                        path: path.clone(),
                        mutable,
                    }
                };
                let value = self.value(kind, t, p.span)?;
                self.reference_address_test(p, value, &mut Vec::new(), ty, mutable, fail, bindings)
            }
            _ if ty.is_copy_value() => {
                let kind = if path.is_empty() {
                    InstructionKind::Load(root)
                } else {
                    let t = Type::Address {
                        mutable: false,
                        inner: Box::new(ty.clone()),
                    };
                    let address = self.value(
                        InstructionKind::BorrowFieldLocal {
                            local: root,
                            path: path.clone(),
                            mutable: false,
                        },
                        t,
                        p.span,
                    )?;
                    InstructionKind::ReadAddress { address }
                };
                let value = self.value(kind, ty.clone(), p.span)?;
                self.pattern_test(p, value, ty, fail, bindings)
            }
            _ => Err(error(
                "E-PATTERN-MOVE-BORROW",
                p.span,
                "nonCopy field needs a selective transfer",
            )),
        }
    }
}
