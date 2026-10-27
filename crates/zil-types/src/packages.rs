//! Finite owning Values. SSA packages are complete; local fields may transfer
//! their responsibility and be refilled without moving the remaining shell.
use super::*;

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

pub(super) fn supported(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, t)| matches!(t, Type::Managed(_)))
        && fields.iter().all(|(_, t)| t.is_copy_value()
            || array_field_supported(t)
            || matches!(t, Type::Closure(_))
            || matches!(t, Type::Managed(_) if resources::supported(t) && !dynamic_store::is_carrier(t))))
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
            Constant::I64(value) => value,
            Constant::U8(value) => i64::from(value),
            Constant::U16(value) => i64::from(value),
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

    pub(super) fn package_place(&self, target: &CoreExpr) -> R<Option<(LocalId, u32, Type)>> {
        let CoreExprKind::Member { object, member } = &target.kind else {
            return Ok(None);
        };
        if !matches!(self.known_type(object), Some(Type::Package { .. })) {
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
        let Some((local, index, ty)) = self.package_place(target)? else {
            return Ok(false);
        };
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
            self.check_owned_array_closure(&ty, 1, rhs.span)?;
            self.owned_array_input(rhs, Some(&ty))?
        } else {
            self.transfer_expr(rhs, Some(&ty))?
        };
        self.same(&actual, &ty, rhs.span)?;
        let value =
            if matches!(ty, Type::Managed(_) | Type::Closure(_)) || array_field_supported(&ty) {
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

    pub(super) fn package_field(
        &mut self,
        object: &CoreExpr,
        member: &str,
        span: Span,
    ) -> R<(ValueId, Type)> {
        if let Some((local, index, element)) = self.owned_array_place(object)? {
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
        // A local projection borrows the actual inline head. A temporary value
        // owns a full-expression slot; its descriptor cannot outlive that slot.
        let (local, ty) = if let CoreExprKind::Name(name) = &object.kind
            && let Some(local) = self.lookup(name)
            && matches!(
                self.function.locals[local.0 as usize].ty,
                Type::Package { .. }
            ) {
            (local, self.function.locals[local.0 as usize].ty.clone())
        } else {
            let (value, ty) = self.transfer_expr(object, None)?;
            if !matches!(ty, Type::Package { .. }) {
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
        let (index, field) = aggregates::field(&ty, member, span)?;
        let kind = if matches!(field, Type::Managed(_)) {
            InstructionKind::DescribePackageField { local, index }
        } else if array_field_supported(&field) || matches!(field, Type::Closure(_)) {
            InstructionKind::TakePackageField { local, index }
        } else {
            InstructionKind::ReadPackageField { local, index }
        };
        let ty = projection_type(field);
        Ok((self.value(kind, ty.clone(), span)?, ty))
    }
}
