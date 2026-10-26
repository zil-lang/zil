//! Finite owning Values. SSA packages are complete; local fields may transfer
//! their responsibility and be refilled without moving the remaining shell.
use super::*;

pub(super) fn supported(ty: &Type) -> bool {
    matches!(ty, Type::Package { fields, .. }
        if fields.iter().any(|(_, t)| matches!(t, Type::Managed(_)))
        && fields.iter().all(|(_, t)| t.is_copy_value()
            || matches!(t, Type::Managed(_) if resources::supported(t))))
}

pub(super) fn projection_type(ty: Type) -> Type {
    match ty {
        Type::Managed(payload) => Type::Desc(payload),
        other => other,
    }
}

impl Builder<'_, '_> {
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
        let (value, actual) = self.transfer_expr(rhs, Some(&ty))?;
        self.same(&actual, &ty, rhs.span)?;
        let value = if matches!(ty, Type::Managed(_)) {
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
        } else {
            InstructionKind::ReadPackageField { local, index }
        };
        let ty = projection_type(field);
        Ok((self.value(kind, ty.clone(), span)?, ty))
    }
}
