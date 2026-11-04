//! Owning indirect assignment reserves the actual base and inline route.
//! A ticket is not an eagerly materialized exclusive endpoint reference.
use super::*;
use zil_ir::{PlaceProjection, ReservationId};
pub(super) struct Prepared {
    pub(super) id: ReservationId,
    pub(super) scope_depth: usize,
}
fn selected<'a>(ty: &'a Type, member: &str) -> Option<(u32, &'a Type)> {
    match ty {
        Type::Tuple(fields) => member
            .parse::<usize>()
            .ok()
            .and_then(|i| fields.get(i).map(|t| (i as u32, t))),
        Type::Package { fields, .. } | Type::Record { fields, .. } => fields
            .iter()
            .enumerate()
            .find(|(_, (name, _))| name == member)
            .map(|(i, (_, ty))| (i as u32, ty)),
        // In particular, never strip Managed or a stored-life qualification.
        _ => None,
    }
}
impl Builder<'_, '_> {
    pub(super) fn abort_exiting_replacements(
        &mut self,
        depth: usize,
        kind: CleanupExitKind,
        span: Span,
    ) -> R<()> {
        self.closures
            .snapshot(self.prepared_replacements.len(), span)?;
        let exiting = self
            .prepared_replacements
            .iter()
            .rev()
            .filter(|p| p.scope_depth > depth)
            .map(|p| p.id)
            .collect::<Vec<_>>();
        if !exiting.is_empty() && kind == CleanupExitKind::Fallthrough {
            return Err(error(
                "E-PLACE-REPLACE",
                span,
                "ordinary statement completion leaves a prepared owning replacement",
            ));
        }
        for reservation in exiting {
            self.emit(
                InstructionKind::AbortPlaceReservation { reservation },
                None,
                span,
            )?;
        }
        Ok(())
    }
    pub(super) fn owning_address_assignment(
        &mut self,
        target: &CoreExpr,
        rhs: &CoreExpr,
    ) -> R<bool> {
        let mut fields = Vec::new();
        let mut root = target;
        while let CoreExprKind::Member {
            object,
            member,
            owner,
        } = &root.kind
        {
            self.closures.snapshot(1, root.span)?;
            fields.push((member, owner.as_ref(), root.span));
            root = object;
        }
        let address_root = match &root.kind {
            CoreExprKind::Unary {
                op: UnaryOp::Dereference,
                operand,
            } => operand.as_ref(),
            _ => root,
        };
        let CoreExprKind::Name(name) = &address_root.kind else {
            return Ok(false);
        };
        self.closures.snapshot(
            self.scopes.len().saturating_mul(
                (self.function.locals.len().saturating_add(1).ilog2() as usize + 1)
                    .saturating_mul(name.len().saturating_add(1)),
            ),
            address_root.span,
        )?;
        let Some(local) = self.lookup(name) else {
            return Ok(false);
        };
        let declared = &self.function.locals[local.0 as usize].ty;
        let Type::Address { mutable, inner } = declared else {
            return Ok(false);
        };
        if !matches!(
            inner.as_ref(),
            Type::Package { .. }
                | Type::Tuple(_)
                | Type::Enum { .. }
                | Type::ErrorUnion { .. }
                | Type::Managed(_)
        ) {
            return Ok(false);
        }
        let mut at = inner.as_ref();
        self.closures.snapshot(fields.len(), target.span)?;
        let mut path = Vec::with_capacity(fields.len());
        for (member, _, span) in fields.iter().rev() {
            let width = match at {
                Type::Package { fields, .. } | Type::Record { fields, .. } => fields
                    .iter()
                    .fold(fields.len(), |n, (name, _)| n.saturating_add(name.len())),
                Type::Tuple(_) => member.len().saturating_add(1),
                _ => return Ok(false),
            };
            self.closures.snapshot(width, *span)?;
            let Some((index, next)) = selected(at, member) else {
                return Ok(false);
            };
            path.push(PlaceProjection::Field(index));
            at = next;
        }
        // The first concrete lane is a closed user-resource leaf. Other
        // owning endpoints retain their existing diagnostic until their tag,
        // partial flags and retained lifetime bridge is implemented.
        let Type::Managed(payload) = at else {
            return Ok(false);
        };
        self.closures.snapshot_type(at, 2, 0, target.span)?;
        if !matches!(payload.as_ref(), Type::Record { .. }) || !payload.is_copy_value() {
            return Ok(false);
        }
        if !*mutable {
            return Err(error(
                "E-ADDRESS-PERMISSION",
                target.span,
                "owning replacement needs an actual exclusive parent",
            ));
        }
        let expected = at.clone();
        self.closures.snapshot_type(declared, 1, 0, target.span)?;
        let declared = declared.clone();
        let Type::Address { inner, .. } = &declared else {
            unreachable!()
        };
        let mut at = inner.as_ref();
        for ((_, owner, span), step) in fields.iter().rev().zip(&path) {
            self.member_owner(at, *owner, *span)?;
            let PlaceProjection::Field(index) = step else {
                unreachable!()
            };
            at = match at {
                Type::Tuple(fields) => &fields[*index as usize],
                Type::Package { fields, .. } | Type::Record { fields, .. } => {
                    &fields[*index as usize].1
                }
                _ => unreachable!(),
            };
        }
        // One evaluation of the genuine parent, before evaluating RHS.
        let (address, actual) = if self.borrowed_captures.contains_key(&local) {
            // A reference-pattern name denotes its referent in ordinary
            // expressions; selecting its existing pointer must not read/copy
            // that owning referent or create a new exclusive endpoint window.
            self.address_place(address_root, true)?
        } else {
            self.expr(address_root, None)?
        };
        self.same(&actual, &declared, address_root.span)?;
        let reservation = ReservationId(self.next_replacement);
        self.next_replacement = self.next_replacement.checked_add(1).ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "owning replacement identity overflow",
                Some(target.span),
            )
        })?;
        self.emit(
            InstructionKind::ReservePlaceAddress {
                reservation,
                address,
                path,
            },
            None,
            target.span,
        )?;
        self.closures.snapshot(1, target.span)?;
        let floor = self.prepared_replacements.len();
        self.prepared_replacements.push(Prepared {
            id: reservation,
            scope_depth: self.scopes.len(),
        });
        let result = (|| {
            let (value, actual) = self.transfer_expr(rhs, Some(&expected))?;
            if self.current.is_none() {
                return Ok(());
            }
            self.same(&actual, &expected, rhs.span)?;
            self.emit(
                InstructionKind::CommitPlaceReplace { reservation, value },
                None,
                target.span,
            )?;
            Ok(())
        })();
        // Restore only this preparation's suffix; outer caller obligations
        // stay live while mutually exclusive error branches emit their Aborts.
        self.prepared_replacements.truncate(floor);
        result?;
        Ok(true)
    }
}
