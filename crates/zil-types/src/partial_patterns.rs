//! General owning patterns test real storage completely before binding refs or
//! moving subtrees. A retained parent keeps every untaken field responsibility.
use super::*;
use zil_ir::PlaceProjection;
type Bindings = Vec<(String, ValueId, Type, Span)>;
struct Place {
    local: LocalId,
    path: Vec<PlaceProjection>,
    ty: Type,
    root: Type,
}
fn has_owned_tuple(pattern: &CorePattern, ty: &Type) -> bool {
    match (&pattern.kind, ty) {
        (CorePatternKind::Tuple(ps), Type::Tuple(ts)) => {
            !ty.is_copy_value() || ps.iter().zip(ts).any(|(p, t)| has_owned_tuple(p, t))
        }
        (
            CorePatternKind::Variant {
                variant, fields, ..
            },
            Type::Enum { variants, .. },
        ) => variants
            .iter()
            .find(|(n, _)| n == variant)
            .is_some_and(|(_, ts)| fields.iter().zip(ts).any(|(p, t)| has_owned_tuple(p, t))),
        _ => false,
    }
}
impl Builder<'_, '_> {
    pub(super) fn partial_pattern_needed(&self, p: &CorePattern, ty: &Type) -> bool {
        matches!(ty, Type::ErrorUnion { .. })
            || owned_shapes::tuple(ty)
            || owned_shapes::enumeration(ty)
                && (reference_patterns::has_reference(p) || has_owned_tuple(p, ty))
    }
    fn owner_pattern_place(&mut self, e: &CoreExpr, consume: bool, extend: bool) -> R<Place> {
        fn locate(this: &mut Builder<'_, '_>, e: &CoreExpr) -> R<Option<Place>> {
            match &e.kind {
                CoreExprKind::Name(name) => {
                    let Some(local) = this.lookup(name) else {
                        return Ok(None);
                    };
                    if this.borrowed_captures.contains_key(&local)
                        || this.closure_captures.contains_key(&local)
                        || this.stored_copy_captures.contains_key(&local)
                    {
                        return Ok(None);
                    }
                    let ty = this.function.locals[local.0 as usize].ty.clone();
                    Ok(Some(Place {
                        local,
                        path: Vec::new(),
                        root: ty.clone(),
                        ty,
                    }))
                }
                CoreExprKind::Member {
                    object,
                    member,
                    owner,
                } => {
                    let Some(mut place) = locate(this, object)? else {
                        return Ok(None);
                    };
                    this.member_owner(&place.ty, owner.as_ref(), e.span)?;
                    let (index, ty) = aggregates::field(&place.ty, member, e.span)?;
                    if place.path.len() >= this.limits.max_call_depth {
                        return Err(Diagnostic::limit(
                            Phase::Types,
                            "pattern place path depth exceeded",
                            Some(e.span),
                        ));
                    }
                    this.closures.snapshot(1, e.span)?;
                    place.path.push(PlaceProjection::Field(index));
                    place.ty = ty;
                    Ok(Some(place))
                }
                _ => Ok(None),
            }
        }
        if !consume && let Some(place) = locate(self, e)? {
            return Ok(place);
        }
        let (v, t) = if consume && matches!(e.kind, CoreExprKind::Member { .. }) {
            if let Some(place) = locate(self, e)? {
                let kind = match &place.root {
                    Type::Tuple(_) => InstructionKind::TakeTupleField {
                        local: place.local,
                        path: place.path,
                    },
                    Type::Package { .. } if place.path.len() == 1 => {
                        let PlaceProjection::Field(index) = place.path[0] else {
                            unreachable!()
                        };
                        InstructionKind::TakePackageField {
                            local: place.local,
                            index,
                        }
                    }
                    _ => {
                        return Err(unsupported(
                            e.span,
                            "nested consuming Product member pattern needs its actual owner path",
                        ));
                    }
                };
                (self.value(kind, place.ty.clone(), e.span)?, place.ty)
            } else {
                self.transfer_expr(e, None)?
            }
        } else {
            self.transfer_expr(e, None)?
        };
        self.closures.snapshot_type(&t, 3, 0, e.span)?;
        self.finish_resource_temps(e.span)?;
        let local = if extend {
            let name = format!("$patternOwner340${}", self.function.locals.len());
            self.local(&name, t.clone(), true, e.span)?
        } else {
            self.temp(t.clone(), e.span)?
        };
        self.store(local, v, e.span)?;
        Ok(Place {
            local,
            path: Vec::new(),
            root: t.clone(),
            ty: t,
        })
    }
    pub(super) fn pattern_variant_index(&self, p: &CorePattern, ty: &Type) -> R<u32> {
        let CorePatternKind::Variant {
            variant,
            ty: member,
            fields,
        } = &p.kind
        else {
            unreachable!()
        };
        match ty {
            Type::Enum { .. } => {
                enums::pattern_variant(member, variant, ty, p.span).map(|(index, _)| index as u32)
            }
            Type::ErrorUnion { error: e, result }
                if variant == "@type-member" && fields.len() == 1 =>
            {
                match (member == e.as_ref(), member == result.as_ref()) {
                    (true, false) => Ok(1),
                    (false, true) => Ok(0),
                    (true, true) => Err(error(
                        "E-ERROR-PATTERN-AMBIGUOUS",
                        p.span,
                        "equal error and success types need an explicit arm discriminator",
                    )),
                    _ => Err(error(
                        "E-ERROR-PATTERN-TYPE",
                        p.span,
                        "pattern member is neither exact ErrorUnion payload type",
                    )),
                }
            }
            _ => Err(error(
                "E-PATTERN-TYPE",
                p.span,
                "variant pattern requires an actual Sum",
            )),
        }
    }
    fn owner_pattern_read(
        &mut self,
        root: LocalId,
        path: &[PlaceProjection],
        ty: &Type,
        span: Span,
    ) -> R<ValueId> {
        if path.is_empty() {
            return self.value(InstructionKind::Load(root), ty.clone(), span);
        }
        let address = self.value(
            InstructionKind::BorrowPlaceLocal {
                local: root,
                path: path.to_vec(),
                mutable: false,
            },
            Type::Address {
                mutable: false,
                inner: Box::new(ty.clone()),
            },
            span,
        )?;
        self.value(InstructionKind::ReadAddress { address }, ty.clone(), span)
    }
    #[allow(clippy::too_many_arguments)]
    fn owner_pattern_walk(
        &mut self,
        p: &CorePattern,
        root: LocalId,
        path: &mut Vec<PlaceProjection>,
        ty: &Type,
        root_ty: &Type,
        fail: BlockId,
        bind: bool,
        consume: bool,
        out: &mut Bindings,
    ) -> R<()> {
        self.closures
            .snapshot_type(ty, 4, path.len().saturating_mul(3), p.span)?;
        match &p.kind {
            CorePatternKind::Tuple(ps) if ps.is_empty() && *ty == Type::Unit => Ok(()),
            CorePatternKind::ErrorArm { is_error, pattern } => {
                let Type::ErrorUnion {
                    error: error_ty,
                    result,
                } = ty
                else {
                    return Err(error(
                        "E-MATCH-TYPE",
                        p.span,
                        "error arm requires an ErrorUnion",
                    ));
                };
                if !bind {
                    let tag = self.value(
                        InstructionKind::PlaceTagLocal {
                            local: root,
                            path: path.clone(),
                        },
                        Type::I32,
                        p.span,
                    )?;
                    self.pattern_branch(tag, Constant::I32(i32::from(*is_error)), fail, p.span)?;
                }
                path.push(PlaceProjection::ErrorArm(*is_error));
                self.owner_pattern_walk(
                    pattern,
                    root,
                    path,
                    if *is_error { error_ty } else { result },
                    root_ty,
                    fail,
                    bind,
                    consume,
                    out,
                )?;
                path.pop();
                Ok(())
            }
            CorePatternKind::Tuple(ps) => {
                let Type::Tuple(ts) = ty else {
                    unreachable!("covered Tuple")
                };
                for (i, (p, t)) in ps.iter().zip(ts).enumerate() {
                    path.push(PlaceProjection::Field(i as u32));
                    self.owner_pattern_walk(p, root, path, t, root_ty, fail, bind, consume, out)?;
                    path.pop();
                }
                Ok(())
            }
            CorePatternKind::Variant { fields, .. } => {
                let variant = self.pattern_variant_index(p, ty)?;
                if !bind {
                    let tag = self.value(
                        InstructionKind::PlaceTagLocal {
                            local: root,
                            path: path.clone(),
                        },
                        Type::I32,
                        p.span,
                    )?;
                    self.pattern_branch(tag, Constant::I32(variant as i32), fail, p.span)?;
                }
                match ty {
                    Type::Enum { variants, .. } => {
                        for (i, (p, t)) in
                            fields.iter().zip(&variants[variant as usize].1).enumerate()
                        {
                            path.push(PlaceProjection::Variant(VariantProjection {
                                variant,
                                index: i as u32,
                            }));
                            self.owner_pattern_walk(
                                p, root, path, t, root_ty, fail, bind, consume, out,
                            )?;
                            path.pop();
                        }
                    }
                    Type::ErrorUnion { error, result } => {
                        path.push(PlaceProjection::ErrorArm(variant == 1));
                        self.owner_pattern_walk(
                            &fields[0],
                            root,
                            path,
                            if variant == 1 { error } else { result },
                            root_ty,
                            fail,
                            bind,
                            consume,
                            out,
                        )?;
                        path.pop();
                    }
                    _ => unreachable!("covered Sum"),
                }
                Ok(())
            }
            CorePatternKind::Reference { mutable, pattern } => {
                if !bind {
                    return Ok(());
                }
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
                        "exclusive pattern needs mutable actual storage",
                    ));
                }
                if *mutable && matches!(ty, Type::Managed(_)) {
                    return Err(error(
                        "E-PATTERN-RESOURCE-HEAD",
                        p.span,
                        "ordinary actual resource head references remain shared",
                    ));
                }
                let kind = if path.is_empty() {
                    if *mutable {
                        InstructionKind::BorrowLocalMut { local: root }
                    } else {
                        InstructionKind::BorrowLocal { local: root }
                    }
                } else {
                    InstructionKind::BorrowPlaceLocal {
                        local: root,
                        path: path.clone(),
                        mutable: *mutable,
                    }
                };
                let address = Type::Address {
                    mutable: *mutable,
                    inner: Box::new(ty.clone()),
                };
                let value = self.value(kind, address.clone(), p.span)?;
                self.pattern_test(pattern, value, &address, fail, out)
            }
            CorePatternKind::Name(name) if bind => {
                let value = if ty.is_copy_value() {
                    self.owner_pattern_read(root, path, ty, p.span)?
                } else {
                    let kind = if path.is_empty() {
                        InstructionKind::TakeLocal { local: root }
                    } else {
                        match root_ty {
                            Type::Tuple(_) => InstructionKind::TakeTupleField {
                                local: root,
                                path: path.clone(),
                            },
                            Type::Enum { .. } | Type::ErrorUnion { .. } => {
                                InstructionKind::TakeVariantField {
                                    local: root,
                                    path: path.clone(),
                                }
                            }
                            _ => {
                                return Err(unsupported(
                                    p.span,
                                    "selective Product-root pattern is under implementation",
                                ));
                            }
                        }
                    };
                    self.value(kind, ty.clone(), p.span)?
                };
                out.push((name.clone(), value, ty.clone(), p.span));
                Ok(())
            }
            CorePatternKind::Wildcard if bind && consume && !ty.is_copy_value() => {
                let name = format!("$discard340${}", out.len());
                let named = CorePattern {
                    kind: CorePatternKind::Name(name),
                    ..p.clone()
                };
                self.owner_pattern_walk(&named, root, path, ty, root_ty, fail, true, consume, out)
            }
            CorePatternKind::Name(_) | CorePatternKind::Wildcard => Ok(()),
            _ if !bind => {
                let value = self.owner_pattern_read(root, path, ty, p.span)?;
                self.pattern_test(p, value, ty, fail, &mut Vec::new())
            }
            _ => Ok(()),
        }
    }
    fn owner_pattern_values(
        &mut self,
        p: &CorePattern,
        place: &Place,
        fail: BlockId,
        consume: bool,
    ) -> R<Bindings> {
        self.owner_pattern_walk(
            p,
            place.local,
            &mut place.path.clone(),
            &place.ty,
            &place.root,
            fail,
            false,
            consume,
            &mut Vec::new(),
        )?;
        let mut values = Vec::new();
        self.owner_pattern_walk(
            p,
            place.local,
            &mut place.path.clone(),
            &place.ty,
            &place.root,
            fail,
            true,
            consume,
            &mut values,
        )?;
        Ok(values)
    }
    pub(super) fn partial_pattern_match(
        &mut self,
        e: &CoreExpr,
        arms: &[CoreMatchArm],
        span: Span,
    ) -> R<()> {
        let consume = !arms
            .iter()
            .any(|a| reference_patterns::has_reference(&a.pattern));
        let place = self.owner_pattern_place(e, consume, false)?;
        self.check_match_coverage(&place.ty, arms, span)?;
        // Mutually exclusive arms start with the same prepared owners. A
        // cleanup emitted for one arm must not remove another arm's debt.
        self.closures.snapshot(self.resource_temps.len(), span)?;
        let prepared = self.resource_temps[self.resource_temp_floor..].to_vec();
        let mut ends = Vec::new();
        for arm in arms {
            if consume {
                self.closures.snapshot(prepared.len(), arm.span)?;
                self.resource_temps.truncate(self.resource_temp_floor);
                self.resource_temps.extend_from_slice(&prepared);
            }

            let fail = self.block_id()?;
            let values = self.owner_pattern_values(&arm.pattern, &place, fail, consume)?;
            self.push_scope(arm.span)?;
            if consume {
                self.finish_resource_temps(arm.span)?;
            }
            for (name, value, ty, span) in values.into_iter().rev() {
                let local = self.local(&name, ty, false, span)?;
                self.store(local, value, span)?;
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
    pub(super) fn partial_pattern_binding(
        &mut self,
        p: &CorePattern,
        mutable: bool,
        expected: Option<&Type>,
        e: &CoreExpr,
    ) -> R<()> {
        let place = self.owner_pattern_place(e, false, true)?;
        if let Some(expected) = expected {
            self.same(&place.ty, expected, e.span)?;
        }
        if !self.irrefutable_pattern(p, &place.ty)? {
            return Err(error(
                "E-PATTERN-REFUTABLE",
                p.span,
                "ordinary binding requires an irrefutable pattern",
            ));
        }
        let impossible = self.block_id()?;
        let values = self.owner_pattern_values(p, &place, impossible, false)?;
        self.function.blocks[impossible.0 as usize].terminator = Terminator::Trap;
        for (name, value, ty, span) in values.into_iter().rev() {
            let local = self.local(&name, ty, mutable, span)?;
            self.store(local, value, span)?;
        }
        Ok(())
    }
    pub(super) fn partial_pattern_condition(
        &mut self,
        p: &CorePattern,
        e: &CoreExpr,
        fail: BlockId,
    ) -> R<(bool, Bindings)> {
        let place = self.owner_pattern_place(e, false, false)?;
        let irrefutable = self.irrefutable_pattern(p, &place.ty)?;
        let values = self.owner_pattern_values(p, &place, fail, false)?;
        Ok((irrefutable, values))
    }
}
