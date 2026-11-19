//! Finite owning Values. SSA packages are complete; local fields may transfer
//! their responsibility and be refilled without moving the remaining shell.
use super::*;
pub(super) mod product;

/// The first field-composition slice keeps nested array fields one level deep.
fn leaf_package(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, t)| matches!(t, Type::Managed(_)))
        && fields.iter().all(|(_, t)| t.is_copy_value()
            || matches!(t, Type::Managed(_) if resources::supported(t) && !dynamic_store::is_carrier(t))))
}

pub(super) fn array_field_supported(ty: &Type) -> bool {
    matches!(ty, Type::Array { element, length } if *length > 0 && leaf_package(element))
}

pub(super) fn has_array_fields(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, t)| array_field_supported(t)))
}

pub(super) fn has_store_fields(ty: &Type) -> bool {
    fn contained(ty: &Type) -> bool {
        if dynamic_store::is_carrier(ty) {
            return true;
        }
        match ty {
            Type::Package { fields, .. } => fields.iter().any(|(_, ty)| contained(ty)),
            Type::Tuple(fields) => fields.iter().any(contained),
            Type::Enum { variants, .. } => variants
                .iter()
                .any(|(_, fields)| fields.iter().any(contained)),
            Type::ErrorUnion { error, result } => contained(error) || contained(result),
            _ => false,
        }
    }
    matches!(ty, Type::Package { .. }) && contained(ty)
}

pub(super) fn supported(ty: &Type) -> bool {
    let Type::Package { fields, .. } = ty else {
        return false;
    };
    fields.iter().any(|(_, t)| matches!(t, Type::Managed(_)))
        && fields.iter().all(|(_, t)| {
            t.is_copy_value()
                || array_field_supported(t)
                || matches!(t, Type::Closure(_))
                || (matches!(t, Type::Managed(_))
                    && resources::supported(t)
                    && !dynamic_store::element(t).is_some_and(dynamic_store::inline_element))
        })
}

/// Ordinary owned values have a separate Product gate. Store, Sum and
/// owned-array callers deliberately retain the older `supported` profile.
pub(super) fn ordinary(ty: &Type) -> bool {
    supported(ty)
        || (product::nested(ty) && product::owned_supported(ty))
        || (matches!(ty, Type::Package { .. }) && owned_shapes::value(ty))
}

/// Capture qualification is separate from ordinary/container admission. The
/// new Product branch still requires an actual complete ownership transfer;
/// its field-state and capture-mode checks are performed by the source flow.
pub(super) fn capture_supported(ty: &Type) -> bool {
    supported(ty) || (product::nested(ty) && product::supported(ty))
}

/// A separate predicate prevents existing nominal Package operations from
/// accidentally accepting an array merely because it owns resources.
pub(super) fn owned_array_supported(ty: &Type) -> bool {
    matches!(ty, Type::Array { element, length } if *length > 0 && ((supported(element) && !callable_fields::has_fields(element)) || matches!(element.as_ref(), Type::Closure(_))))
}

pub(super) fn projection_type(ty: Type) -> Type {
    match ty {
        Type::Managed(payload) => Type::Desc(payload),
        other => other,
    }
}

impl Builder<'_, '_> {
    pub(super) fn owned_array_input(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        let entered = self.closures.semantic_begin(self.function.id, expression)?;
        let result = self.owned_array_input_context(expression, expected);
        self.closures.semantic_end(
            self.function.id,
            expression.id,
            entered,
            result.as_ref().ok().map(|row| row.0),
            self.current.map(|block| crate::semantics::emission::Point {
                block,
                instruction: self.function.blocks[block.0 as usize].instructions.len(),
            }),
            expression.span,
        )?;
        result
    }
    fn owned_array_input_context(
        &mut self,
        expression: &CoreExpr,
        expected: Option<&Type>,
    ) -> R<(ValueId, Type)> {
        // Entering a unique element slot transfers a concrete environment even
        // when its captures carry no resource cleanup obligations.
        if let CoreExprKind::Name(name) = &expression.kind
            && let Some(local) = self.lookup(name)
            && let Type::Closure(id) = self.function.locals[local.0 as usize].ty
        {
            let ty = Type::Closure(id);
            if let Some(expected) = expected {
                self.same(&ty, expected, expression.span)?;
            }
            return Ok((
                self.value(
                    InstructionKind::TakeLocal { local },
                    ty.clone(),
                    expression.span,
                )?,
                ty,
            ));
        }
        self.transfer_expr(expression, expected)
    }

    pub(super) fn check_package_field_closure(&mut self, field: &Type, span: Span) -> R<()> {
        if let Type::Closure(id) = field
            && self.closures.semantic_emission.has_original_context()
        {
            let before = self.closures.remaining_work();
            let mut remaining = before;
            let checked = self.closures.semantic_emission.require_original_closure_transport(
                *id, &mut remaining, span,
            );
            self.closures.snapshot(before - remaining, span)?;
            checked?;
            // The Package field retains this complete environment. The
            // transport check admits its actual representation; the original
            // capture header and the actual place operation still account for
            // every borrowed source and its live window.
            return self.closures.factory_transportable(*id, span);
        }
        self.check_owned_array_closure(field, 1, span)
    }

    pub(super) fn check_owned_array_closure(
        &mut self,
        element: &Type,
        length: u32,
        span: Span,
    ) -> R<()> {
        let Type::Closure(root) = element else {
            if matches!(element, Type::Package { .. }) {
                let work = inference::type_snapshot_work(element, span, self.limits)?;
                self.closures
                    .snapshot(work.saturating_mul(length as usize), span)?;
            }
            return Ok(());
        };
        let mut pending = vec![*root];
        while let Some(id) = pending.pop() {
            let count = self
                .closures
                .pending
                .get(id.0 as usize)
                .ok_or_else(|| {
                    error(
                        "E-CLOSURE-INFERENCE",
                        span,
                        "array element closure identity is absent",
                    )
                })?
                .captures
                .len();
            // Walk occurrences, not only distinct IDs. Charge each full type
            // subtree at array multiplicity before stored_type clones it.
            self.closures
                .snapshot(count.saturating_add(length as usize), span)?;
            for index in 0..count {
                let work = inference::type_snapshot_work(
                    &self.closures.pending[id.0 as usize].captures[index].ty,
                    span,
                    self.limits,
                )?;
                self.closures
                    .snapshot(work.saturating_mul(length as usize), span)?;
                let capture = self.closures.pending[id.0 as usize].captures[index].stored_type();
                match capture {
                    Type::Closure(child) => pending.push(child),
                    Type::Managed(_) | Type::Package { .. } => {}
                    ty if ty.is_copy_value() => {}
                    _ => {
                        return Err(unsupported(
                            span,
                            "owning callback array elements require self-contained environments",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn owned_array_place(&self, target: &CoreExpr) -> R<Option<(LocalId, u32, Type)>> {
        let CoreExprKind::Index { object, index } = &target.kind else {
            return Ok(None);
        };
        if self
            .known_type_ref(object)
            .is_some_and(|ty| !owned_array_supported(ty))
        {
            return Ok(None);
        }
        let Some(ty) = self.known_type(object) else {
            return Ok(None);
        };
        if !owned_array_supported(&ty) {
            return Ok(None);
        }
        let CoreExprKind::Name(name) = &object.kind else {
            return Err(unsupported(
                object.span,
                "owning array element operations require a named local shell",
            ));
        };
        let local = self
            .lookup(name)
            .ok_or_else(|| error("E-UNKNOWN-NAME", object.span, "unknown owning array local"))?;
        let Type::Array { element, length } = ty else {
            unreachable!("owned array predicate")
        };
        let literal = match &index.kind {
            CoreExprKind::Integer(text) => integer(text, Some(&Type::I64), false, index.span)?,
            CoreExprKind::Literal(value) => value.clone(),
            CoreExprKind::Unary {
                op: UnaryOp::Negate,
                operand,
            } if matches!(operand.kind, CoreExprKind::Integer(_)) => {
                let CoreExprKind::Integer(text) = &operand.kind else {
                    unreachable!()
                };
                integer(text, Some(&Type::I64), true, index.span)?
            }
            _ => {
                return Err(unsupported(
                    index.span,
                    "owning array indices require a fixed integer literal; dynamic locations are not exact slots",
                ));
            }
        };
        let index_value = match literal {
            Constant::I32(value) => i64::from(value),
            Constant::I8(value) => i64::from(value),
            Constant::I16(value) => i64::from(value),
            Constant::Isize(value) => value,
            Constant::I64(value) => value,
            Constant::U8(value) => i64::from(value),
            Constant::U16(value) => i64::from(value),
            Constant::U32(value) => i64::from(value),
            _ => {
                return Err(error(
                    "E-INDEX-TYPE",
                    index.span,
                    "array index must be an integer",
                ));
            }
        };
        let index_value = u32::try_from(index_value)
            .ok()
            .filter(|value| *value < length)
            .ok_or_else(|| {
                error(
                    "E-INDEX-BOUNDS",
                    index.span,
                    "owning array index is outside its fixed length",
                )
            })?;
        Ok(Some((local, index_value, *element)))
    }

    fn owned_array_assign(&mut self, target: &CoreExpr, rhs: &CoreExpr) -> R<bool> {
        let Some((local, index, element)) = self.owned_array_place(target)? else {
            return Ok(false);
        };
        if !self.function.locals[local.0 as usize].mutable {
            return Err(error(
                "E-IMMUTABLE-ASSIGNMENT",
                target.span,
                "owning array element assignment requires a mutable local shell",
            ));
        }
        let (value, actual) = self.owned_array_input(rhs, Some(&element))?;
        self.same(&actual, &element, rhs.span)?;
        let prepared = self.stage_argument(value, &element, rhs.span)?;
        self.emit(
            InstructionKind::DropOwnedArrayElement { local, index },
            None,
            target.span,
        )?;
        let value = self.commit_arguments(vec![prepared], rhs.span)?[0];
        self.emit(
            InstructionKind::StoreOwnedArrayElement {
                local,
                index,
                value,
            },
            None,
            target.span,
        )?;
        Ok(true)
    }

    /// A direct invocation locates its field before evaluating any arguments.
    /// Ordinary reads still go through package_field and take the environment.
    /// No temporary, group, projection or Sequence is silently made a place.
    pub(super) fn package_callable_callee(
        &mut self,
        target: &CoreExpr,
    ) -> R<Option<(ValueId, ClosureId, CallableKind)>> {
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &target.kind
        else {
            return Ok(None);
        };
        let CoreExprKind::Name(name) = &object.kind else {
            return self.store_callable_callee(target);
        };
        // Bound all newly introduced scope/name and field comparisons before
        // their lookup. Inspect the borrowed declaration without cloning the
        // whole Package type or allocating a second projection graph.
        let depth = self.function.locals.len().saturating_add(1).ilog2() as usize + 1;
        self.closures.snapshot(
            self.scopes
                .len()
                .saturating_mul(depth)
                .saturating_mul(name.len().saturating_add(1)),
            target.span,
        )?;
        let Some(local) = self.lookup(name) else {
            return Ok(None);
        };
        self.member_owner_work(owner.as_ref(), target.span)?;
        member_owner::check_member_owner(
            &self.function.locals[local.0 as usize].ty,
            owner.as_ref(),
            target.span,
        )?;
        let Type::Package { fields, .. } = &self.function.locals[local.0 as usize].ty else {
            return if self.function.locals[local.0 as usize]
                .ty
                .is_package_address()
            {
                self.store_callable_callee(target)
            } else {
                Ok(None)
            };
        };
        self.closures.snapshot(
            fields.len().saturating_mul(member.len().saturating_add(1)),
            target.span,
        )?;
        let Some((index, (_, Type::Closure(id)))) = fields
            .iter()
            .enumerate()
            .find(|(_, (name, _))| name == member)
        else {
            return Ok(None);
        };
        let (index, id) = (index as u32, *id);
        self.semantic_local_place(object, local)?;
        let kind = self.field_callable_kind(id, target)?;
        let value = if kind == CallableKind::FnOnce {
            self.value(
                InstructionKind::TakePackageField { local, index },
                Type::Closure(id),
                target.span,
            )?
        } else {
            self.borrow_package_field(
                local,
                index,
                Type::Closure(id),
                kind == CallableKind::FnMut,
                target.span,
            )?
            .0
        };
        Ok(Some((value, id, kind)))
    }

    fn field_callable_kind(&mut self, id: ClosureId, target: &CoreExpr) -> R<CallableKind> {
        let span = target.span;
        if let Some(kind) = self.declared_state_kind(target, None, id)? {
            return Ok(kind);
        }
        let count = self.closures.pending[id.0 as usize].captures.len();
        self.closures
            .snapshot(count.saturating_mul(2).saturating_add(1), span)?;
        for index in 0..count {
            let work = inference::type_snapshot_work(
                &self.closures.pending[id.0 as usize].captures[index].ty,
                span,
                self.limits,
            )?;
            // callable_kind and the existing call path inspect stored capture
            // types. Pay for their snapshots before constructing any of them.
            self.closures.snapshot(work.saturating_mul(4), span)?;
        }
        Ok(self
            .state_callable_mode
            .filter(|(bound, _)| *bound == id)
            .map_or_else(
                || self.closures.pending[id.0 as usize].callable_kind(),
                |(_, kind)| kind,
            ))
    }

    /// A Store window grants an address, never an owned environment value.
    /// Classify without evaluating the source, then locate/reborrow exactly
    /// once before the caller evaluates any arguments.
    fn store_callable_callee(
        &mut self,
        target: &CoreExpr,
    ) -> R<Option<(ValueId, ClosureId, CallableKind)>> {
        let mut base = target;
        let mut selectors = Vec::new();
        while let CoreExprKind::Member { object, member, owner } = &base.kind {
            if selectors.len() >= self.limits.max_call_depth {
                return Err(Diagnostic::limit(Phase::Types, "callback projection depth exceeded", Some(base.span)));
            }
            self.member_owner_work(owner.as_ref(), base.span)?;
            self.closures.snapshot(2, base.span)?;
            selectors.push((member, owner.as_ref(), base.span));
            base = object;
        }
        let (source, from_store) = match &base.kind {
            CoreExprKind::Name(name) => (name, false),
            CoreExprKind::Call { callee, arguments }
                if matches!(
                    dynamic_store::primitive(callee),
                    Some("store_at" | "store_at_mut")
                ) =>
            {
                let Some(CoreExpr {
                    kind: CoreExprKind::Name(name),
                    ..
                }) = arguments.first()
                else {
                    return Ok(None);
                };
                (name, true)
            }
            _ => return Ok(None),
        };
        let depth = self.function.locals.len().saturating_add(1).ilog2() as usize + 1;
        self.closures.snapshot(
            self.scopes
                .len()
                .saturating_mul(depth)
                .saturating_mul(source.len().saturating_add(1)),
            target.span,
        )?;
        let Some(local) = self.lookup(source) else {
            return Ok(None);
        };
        let ty = &self.function.locals[local.0 as usize].ty;
        let element = if from_store {
            dynamic_store::element(ty)
        } else if let Type::Address { inner, .. } = ty {
            Some(inner.as_ref())
        } else {
            None
        };
        let Some(mut selected) = element else { return Ok(None); };
        for (member, owner, span) in selectors.into_iter().rev() {
            member_owner::check_member_owner(selected, owner, span)?;
            let Type::Package { fields, .. } = selected else { return Ok(None); };
            self.closures.snapshot(fields.len().saturating_mul(member.len().saturating_add(1)), span)?;
            let Some((_, child)) = fields.iter().find(|(name, _)| name == member) else {
                return Ok(None);
            };
            selected = child;
        }
        let Type::Closure(id) = selected else { return Ok(None); };
        let id = *id;
        let kind = self.field_callable_kind(id, target)?;
        if kind == CallableKind::FnOnce {
            return Err(error(
                "E-CLOSURE-MODE",
                target.span,
                "FnOnce requires ownership of the selected callback",
            ));
        }
        let (address, _) = self.borrow_place_mode(target, kind == CallableKind::FnMut)?;
        Ok(Some((address, id, kind)))
    }

    pub(super) fn package_place(&mut self, target: &CoreExpr) -> R<Option<(LocalId, u32, Type)>> {
        let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &target.kind
        else {
            return Ok(None);
        };
        if !self.known_type_is(object, |ty| {
            matches!(stored::physical(ty), Type::Package { .. })
        }) {
            return Ok(None);
        }
        let CoreExprKind::Name(name) = &object.kind else {
            return Err(unsupported(
                target.span,
                "package field transfer or assignment requires a named local shell",
            ));
        };
        let local = self
            .lookup(name)
            .ok_or_else(|| error("E-UNKNOWN-NAME", object.span, "unknown package local"))?;
        // A captured Product names its referent, not this activation's
        // Address local. Its fields must use the actual address planner.
        if self.borrowed_captures.contains_key(&local) {
            return Ok(None);
        }
        self.member_owner_work(owner.as_ref(), target.span)?;
        member_owner::check_member_owner(
            &self.function.locals[local.0 as usize].ty,
            owner.as_ref(),
            target.span,
        )?;
        let (index, ty) = aggregates::field(
            &self.function.locals[local.0 as usize].ty,
            member,
            target.span,
        )?;
        Ok(Some((local, index, ty)))
    }

    pub(super) fn package_place_hint(&self, target: &CoreExpr) -> R<Option<(LocalId, u32, Type)>> {
        let CoreExprKind::Member { object, member, .. } = &target.kind else {
            return Ok(None);
        };
        if !self.known_type_is(object, |ty| {
            matches!(stored::physical(ty), Type::Package { .. })
        }) {
            return Ok(None);
        }
        let CoreExprKind::Name(name) = &object.kind else {
            return Err(unsupported(
                target.span,
                "package field transfer or assignment requires a named local shell",
            ));
        };
        let local = self
            .lookup(name)
            .ok_or_else(|| error("E-UNKNOWN-NAME", object.span, "unknown package local"))?;
        // A captured Product names its referent, not this activation's
        // Address local. Its fields must use the actual address planner.
        if self.borrowed_captures.contains_key(&local) {
            return Ok(None);
        }
        let (index, ty) = aggregates::field(
            &self.function.locals[local.0 as usize].ty,
            member,
            target.span,
        )?;
        Ok(Some((local, index, ty)))
    }

    pub(super) fn package_assign(&mut self, target: &CoreExpr, rhs: &CoreExpr) -> R<bool> {
        if self.owned_array_assign(target, rhs)? {
            return Ok(true);
        }
        // An explicit dereference of a named Package address is a referent
        // place, never a copied local shell. Classify without evaluating it;
        // the ordinary assignment dispatcher then invokes assign_address and
        // its complete owning endpoint planner exactly once.
        if let CoreExprKind::Member { object, .. } = &target.kind
            && let CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } = &object.kind
            && let CoreExprKind::Name(name) = &operand.kind
        {
            let levels = self.function.locals.len().saturating_add(1).ilog2() as usize + 1;
            self.closures.snapshot(
                self.scopes
                    .len()
                    .saturating_mul(levels)
                    .saturating_mul(12)
                    .saturating_mul(name.len().saturating_add(1)),
                target.span,
            )?;
            if let Some(local) = self.lookup(name)
                && matches!(&self.function.locals[local.0 as usize].ty,
                    Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Package { .. }))
            {
                return Ok(false);
            }
        }
        let Some((local, index, ty)) = self.package_place(target)? else {
            return Ok(false);
        };
        if let CoreExprKind::Member { object, .. } = &target.kind {
            if matches!(object.kind, CoreExprKind::Name(_)) {
                self.semantic_local_place(object, local)?;
            }
        }
        if !self.function.locals[local.0 as usize].mutable {
            return Err(error(
                "E-IMMUTABLE-ASSIGNMENT",
                target.span,
                "package field assignment requires a mutable local shell",
            ));
        }
        // Resolve the static local and field before evaluating RHS. A real
        // temporary holds its new obligation until old-field cleanup commits.
        let (value, actual) = if matches!(ty, Type::Closure(_)) {
            self.check_package_field_closure(&ty, rhs.span)?;
            self.owned_array_input(rhs, Some(&ty))?
        } else {
            self.transfer_expr(rhs, Some(&ty))?
        };
        self.same(&actual, &ty, rhs.span)?;
        let value = if matches!(
            ty,
            Type::Managed(_) | Type::Closure(_) | Type::Package { .. }
        ) || array_field_supported(&ty)
            || matches!(&ty, Type::BorrowedStorage { storage, .. }
                if matches!(storage.as_ref(), Type::Package { .. })
                    && product::owned_supported(&ty))
            || owned_shapes::sum(&ty)
        {
            let prepared = self.stage_argument(value, &ty, rhs.span)?;
            self.emit(
                InstructionKind::DropPackageField { local, index },
                None,
                target.span,
            )?;
            self.commit_arguments(vec![prepared], rhs.span)?[0]
        } else {
            value
        };
        self.emit(
            InstructionKind::StorePackageField {
                local,
                index,
                value,
            },
            None,
            target.span,
        )?;
        Ok(true)
    }

    pub(super) fn local_product_path(
        &mut self,
        e: &CoreExpr,
    ) -> R<Option<(LocalId, Vec<zil_ir::PlaceProjection>, Type)>> {
        match &e.kind {
            CoreExprKind::Name(name) => {
                let Some(local) = self.lookup(name) else {
                    return Ok(None);
                };
                let ty = &self.function.locals[local.0 as usize].ty;
                if !matches!(ty, Type::Package { .. })
                    || self.borrowed_captures.contains_key(&local)
                {
                    return Ok(None);
                }
                self.closures.snapshot_type(ty, 1, 1, e.span)?;
                Ok(Some((local, Vec::new(), ty.clone())))
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } => {
                let Some((local, mut path, ty)) = self.local_product_path(object)? else {
                    return Ok(None);
                };
                if !matches!(ty, Type::Package { .. }) {
                    return Ok(None);
                }
                self.member_owner(&ty, owner.as_ref(), e.span)?;
                let (index, child) = aggregates::field(&ty, member, e.span)?;
                if path.len() >= self.limits.max_call_depth {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "nested Product field path depth exceeded",
                        Some(e.span),
                    ));
                }
                self.closures
                    .snapshot(path.len().saturating_add(1), e.span)?;
                path.push(zil_ir::PlaceProjection::Field(index));
                Ok(Some((local, path, child)))
            }
            _ => Ok(None),
        }
    }

    /// A Copy read or Managed description never transfers its owning intermediate child.
    /// The real root and every field owner are checked before the borrow is
    /// emitted; no temporary Package is substituted for the original storage.
    fn nested_product_observed_field(
        &mut self,
        object: &CoreExpr,
        member: &str,
        owner: Option<&Type>,
        span: Span,
    ) -> R<Option<(ValueId, Type)>> {
        if !matches!(object.kind, CoreExprKind::Member { .. }) {
            return Ok(None);
        }
        let Some((local, mut path, ty)) = self.local_product_path(object)? else {
            return Ok(None);
        };
        if !matches!(ty, Type::Package { .. }) {
            return Ok(None);
        }
        self.member_owner(&ty, owner, span)?;
        let (index, field) = aggregates::field(&ty, member, span)?;
        if !field.is_copy_value() && !matches!(field, Type::Managed(_)) {
            return Ok(None);
        }
        self.closures
            .snapshot_type(&field, 2, path.len().saturating_add(1), span)?;
        path.push(zil_ir::PlaceProjection::Field(index));
        let address = self.value(
            InstructionKind::BorrowPlaceLocal {
                local,
                path,
                mutable: false,
            },
            Type::Address {
                mutable: false,
                inner: Box::new(field.clone()),
            },
            span,
        )?;
        let (instruction, observed) = match field {
            Type::Managed(payload) => (
                InstructionKind::DescribeAddress { address },
                Type::Desc(payload),
            ),
            field => (InstructionKind::ReadAddress { address }, field),
        };
        Ok(Some((
            self.value(instruction, observed.clone(), span)?,
            observed,
        )))
    }

    pub(super) fn package_field(
        &mut self,
        object: &CoreExpr,
        member: &str,
        owner: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        if let Some((local, index, element)) = self.owned_array_place(object)? {
            self.member_owner(&element, owner, span)?;
            let (field, ty) = aggregates::field(&element, member, span)?;
            if array_field_supported(&ty) {
                return Err(unsupported(
                    span,
                    "take the complete Package array element before moving its owning field",
                ));
            }
            let ty = projection_type(ty);
            let value = self.value(
                InstructionKind::ProjectOwnedArrayField {
                    local,
                    index,
                    field,
                },
                ty.clone(),
                span,
            )?;
            return Ok((value, ty));
        }
        if let Some(value) = self.nested_product_observed_field(object, member, owner, span)? {
            return Ok(value);
        }
        // A local projection borrows the actual inline head. A temporary value
        // owns a full-expression slot; its descriptor cannot outlive that slot.
        let (local, ty) = if let CoreExprKind::Name(name) = &object.kind
            && let Some(local) = self.lookup(name)
            && matches!(
                stored::physical(&self.function.locals[local.0 as usize].ty),
                Type::Package { .. }
            ) {
            self.semantic_local_place(object, local)?;
            (local, self.function.locals[local.0 as usize].ty.clone())
        } else {
            let (value, ty) = self.transfer_expr(object, None)?;
            if !matches!(stored::physical(&ty), Type::Package { .. }) {
                return Err(error(
                    "E-FIELD-TYPE",
                    span,
                    "package projection requires a package",
                ));
            }
            let local = self.temp(ty.clone(), object.span)?;
            self.store(local, value, object.span)?;
            (local, ty)
        };
        self.member_owner(&ty, owner, span)?;
        let (index, field) = aggregates::field(&ty, member, span)?;
        let kind = if matches!(field, Type::Managed(_)) {
            InstructionKind::DescribePackageField { local, index }
        } else if array_field_supported(&field) || owned_shapes::field(&field) {
            InstructionKind::TakePackageField { local, index }
        } else {
            InstructionKind::ReadPackageField { local, index }
        };
        let ty = projection_type(field);
        Ok((self.value(kind, ty.clone(), span)?, ty))
    }
}

#[cfg(test)]
mod field_transfer_tests {
    use super::*;
    use zil_base::{FileId, ModuleId, SourceUnit};

    #[test]
    fn original_task_failure_moves_callable_inputs_and_reads_copy_reason() {
        // Keep the real async cleanup and failure branches. In particular the
        // returned callback owns a child Task; a zero-class shape is not Copy.
        let text = include_str!(
            "../../../tests/language-readiness/task-owner-transport/nested-callable-child-cleanup.zil"
        );
        let source = SourceUnit::new(FileId(0), ModuleId(0), "task-fields.zil", text);
        let parsed = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let target = TargetProfile::default();
        let aliases = BTreeMap::new();
        // Task worker preparation requires the original declaration/call
        // context. The scalar module convenience entry deliberately does not
        // manufacture that context from an already-specialized worker.
        let modules = zil_expand::expand_program_with_semantics(
            std::slice::from_ref(&source),
            &[&parsed],
            &[Vec::new()],
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &aliases,
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .into_iter()
        .map(|(module, _)| module)
        .collect::<Vec<_>>();
        assert!(modules.iter().all(|module| module.core.semantic.is_some()));
        let program = CoreProgram {
            root: source.module_id,
            source_fingerprint: source.fingerprint(),
            modules,
        };
        let typed = check_program(&program, &target, &limits).unwrap().module;
        assert!(typed.semantic.is_some());
        let mut read_reason = false;
        let mut take_callbacks = false;
        let mut unpack_callbacks = false;
        for function in &typed.functions {
            for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
                match &instruction.kind {
                    InstructionKind::ReadPackageField { local, index }
                    | InstructionKind::TakePackageField { local, index } => {
                        let Type::Package { name, fields } =
                            stored::physical(&function.locals[local.0 as usize].ty)
                        else {
                            continue;
                        };
                        if !name.starts_with("@std.task:v1:TaskCreateFailure:") {
                            continue;
                        }
                        let (name, ty) = &fields[*index as usize];
                        if matches!(&instruction.kind, InstructionKind::ReadPackageField { .. }) {
                            assert!(ty.is_copy_value(), "non-Copy failure input read: {ty:?}");
                            read_reason |= name == "reason";
                        } else if name == "inputs"
                            && matches!(stored::physical(ty), Type::Tuple(fields)
                                if fields.iter().any(|ty| matches!(ty, Type::Closure(_))))
                        {
                            take_callbacks = true;
                            assert_eq!(
                                function.values[instruction.result.unwrap().0 as usize],
                                *ty
                            );
                        }
                    }
                    InstructionKind::UnpackTuple { local, fields } => {
                        if let Type::Tuple(types) =
                            stored::physical(&function.locals[local.0 as usize].ty)
                            && types.iter().any(|ty| matches!(ty, Type::Closure(_)))
                        {
                            assert_eq!(fields.len(), types.len());
                            for (field, ty) in fields.iter().zip(types) {
                                assert_eq!(&function.locals[field.0 as usize].ty, ty);
                            }
                            unpack_callbacks = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        assert!(
            read_reason,
            "actual failure reason should retain its Copy read"
        );
        assert!(
            take_callbacks,
            "actual callback tuple must transfer from failure.inputs"
        );
        assert!(
            unpack_callbacks,
            "destructuring must transfer the complete callback child"
        );
    }
}
