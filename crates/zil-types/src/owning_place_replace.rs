//! Owning indirect assignment reserves the actual base and inline route.
//! A ticket is not an eagerly materialized exclusive endpoint reference.
use super::*;
use zil_ir::{PlaceProjection, ReservationId};
pub(super) struct Prepared {
    pub(super) id: ReservationId,
    pub(super) scope_depth: usize,
}
#[cfg(test)]
thread_local! { static PLANNED_FIELDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
fn lookup_work(scopes: usize, locals: usize, bytes: usize, span: Span) -> R<usize> {
    let levels = locals.checked_ilog2().map_or(1, |n| n as usize + 2);
    scopes
        .checked_mul(levels)
        .and_then(|n| n.checked_mul(12))
        .and_then(|n| n.checked_mul(bytes.saturating_add(1)))
        .ok_or_else(|| {
            Diagnostic::limit(
                Phase::Types,
                "replacement lexical lookup fee overflow",
                Some(span),
            )
        })
}
fn selected<'a>(
    ty: &'a Type,
    member: &str,
    state: &mut closures::State,
    span: Span,
) -> R<Option<(u32, &'a Type)>> {
    match ty {
        Type::Package { fields, .. } | Type::Record { fields, .. } => {
            // Pay for the planning walk before inspecting variable-width names.
            state.snapshot(fields.len(), span)?;
            let mut width = fields.len();
            for (name, _) in fields {
                #[cfg(test)]
                PLANNED_FIELDS.with(|n| n.set(n.get() + 1));
                width = width
                    .checked_add(name.len())
                    .and_then(|n| n.checked_add(member.len().saturating_add(1)))
                    .ok_or_else(|| {
                        Diagnostic::limit(
                            Phase::Types,
                            "replacement field selection fee overflow",
                            Some(span),
                        )
                    })?;
            }
            state.snapshot(width, span)?;
        }
        Type::Tuple(_) => state.snapshot(member.len().saturating_add(1), span)?,
        _ => return Ok(None),
    }
    Ok(selected_paid(ty, member))
}
fn selected_paid<'a>(ty: &'a Type, member: &str) -> Option<(u32, &'a Type)> {
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
fn nested_package_endpoint(ty: &Type, state: &mut closures::State, span: Span) -> R<bool> {
    state.snapshot_type(ty, 1, 0, span)?;
    let mut pending = vec![ty];
    let mut owns = false;
    while let Some(ty) = pending.pop() {
        state.snapshot(1, span)?;
        match ty {
            Type::Package { fields, .. } => {
                state.snapshot(fields.len(), span)?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Managed(payload)
                if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value() =>
            {
                owns = true
            }
            Type::Managed(payload)
                if matches!(payload.as_ref(), Type::OwnedStore { element }
                    if dynamic_store::inline_element(element)
                        && dynamic_store::supported_element(element)) =>
            {
                owns = true
            }
            ty if ty.is_b0_scalar() => {}
            _ => return Ok(false),
        }
    }
    Ok(owns)
}
fn whole_sum_endpoint(ty: &Type, state: &mut closures::State, span: Span) -> R<bool> {
    state.snapshot_type(ty, 1, 0, span)?;
    let mut pending = vec![ty];
    let mut owns = false;
    while let Some(ty) = pending.pop() {
        state.snapshot(1, span)?;
        match ty {
            Type::Managed(payload)
                if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value() =>
            {
                owns = true
            }
            Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }) => {
                let Type::OwnedStore { element } = payload.as_ref() else {
                    unreachable!()
                };
                if !dynamic_store::supported_element(element) {
                    return Ok(false);
                }
                owns = true
            }
            Type::Tuple(fields) => {
                state.snapshot(fields.len(), span)?;
                pending.extend(fields);
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                state.snapshot(fields.len(), span)?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Enum { variants, .. } => {
                state.snapshot(variants.len(), span)?;
                for (_, fields) in variants {
                    state.snapshot(fields.len(), span)?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                state.snapshot(2, span)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            Type::Array { element, .. } if element.is_copy_value() => {
                state.snapshot(1, span)?;
                pending.push(element);
            }
            ty if ty.is_b0_scalar() => {}
            _ => return Ok(false),
        }
    }
    Ok(owns)
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
        // Literal and binary expressions cannot produce a Managed carrier.
        // Keep ordinary scalar writes on their existing address path instead
        // of planning a second lexical lookup and an owning reservation.
        if matches!(
            rhs.kind,
            CoreExprKind::Literal(_) | CoreExprKind::Binary { .. }
        ) {
            return Ok(false);
        }
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
            lookup_work(
                self.scopes.len(),
                self.function.locals.len(),
                name.len(),
                address_root.span,
            )?,
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
            let Some((index, next)) = selected(at, member, self.closures, *span)? else {
                return Ok(false);
            };
            path.push(PlaceProjection::Field(index));
            at = next;
        }
        // Ticket data describes the actual endpoint, not a declaration-based
        // permission. Each kernel separately recovers its root and completeness.
        match at {
            Type::Managed(payload) => {
                self.closures.snapshot_type(at, 2, 0, target.span)?;
                if !(matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value()
                    || matches!(payload.as_ref(), Type::OwnedStore { element }
                        if dynamic_store::inline_element(element)
                            && dynamic_store::supported_element(element)))
                {
                    return Ok(false);
                }
            }
            Type::Package { fields, .. } => {
                self.closures.snapshot_type(at, 2, 0, target.span)?;
                self.closures.snapshot(fields.len(), target.span)?;
                let mut owners = 0;
                for (_, field) in fields {
                    if let Type::Managed(payload) = field {
                        if !(matches!(payload.as_ref(), Type::Record { .. })
                            && payload.is_copy_value()
                            || matches!(payload.as_ref(), Type::OwnedStore { element }
                                if dynamic_store::inline_element(element)
                                    && dynamic_store::supported_element(element)))
                        {
                            return Ok(false);
                        }
                        owners += 1;
                    } else if matches!(field, Type::Package { .. }) {
                        if !nested_package_endpoint(field, self.closures, target.span)? {
                            return Ok(false);
                        }
                        owners += 1;
                    } else if !field.is_b0_scalar() {
                        return Ok(false);
                    }
                }
                if owners == 0 {
                    return Ok(false);
                }
            }
            Type::Enum { variants, .. } => {
                self.closures.snapshot_type(at, 2, 0, target.span)?;
                self.closures.snapshot(variants.len(), target.span)?;
                let mut owning = false;
                let mut ordinary = false;
                'variants: for (_, fields) in variants {
                    self.closures.snapshot(fields.len(), target.span)?;
                    let mut owners = 0;
                    for field in fields {
                        if let Type::Managed(payload) = field {
                            if !matches!(payload.as_ref(), Type::Record { .. })
                                || !payload.is_copy_value()
                            {
                                ordinary = true;
                                break 'variants;
                            }
                            owners += 1;
                        } else if !field.is_b0_scalar() {
                            ordinary = true;
                            break 'variants;
                        }
                    }
                    owning |= owners != 0;
                }
                if ordinary {
                    if !path.is_empty() || !whole_sum_endpoint(at, self.closures, target.span)? {
                        return Ok(false);
                    }
                } else if !owning {
                    return Ok(false);
                }
            }
            Type::Tuple(_) | Type::ErrorUnion { .. } if path.is_empty() => {
                if !whole_sum_endpoint(at, self.closures, target.span)? {
                    return Ok(false);
                }
            }
            _ => return Ok(false),
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn field_planner_stops_before_a_wide_scan_when_caller_work_is_low() {
        let fields = (0..256)
            .map(|i| {
                (
                    format!("{}{:04}", "long_common_prefix_".repeat(32), i),
                    Type::I32,
                )
            })
            .collect::<Vec<_>>();
        let ty = Type::Record {
            name: "Wide".into(),
            fields,
        };
        let limits = Limits::default();
        let mut state = closures::State::new(0, &limits);
        state
            .snapshot(limits.max_analysis_steps - 3, Span::default())
            .unwrap();
        PLANNED_FIELDS.with(|n| n.set(0));
        let error = selected(&ty, "last", &mut state, Span::default()).unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        assert_eq!(
            PLANNED_FIELDS.with(|n| n.get()),
            0,
            "unpaid planning scan happened before rejection"
        );
        assert_eq!(state.remaining_work(), 3);
    }
    #[test]
    fn long_prefix_field_selection_and_lexical_lookup_have_exact_fresh_boundaries() {
        let fields = (0..64)
            .map(|i| (format!("{}{:04}", "same_prefix_".repeat(16), i), Type::I32))
            .collect::<Vec<_>>();
        let member = fields.last().unwrap().0.clone();
        let field_work = fields.len() * 2
            + fields
                .iter()
                .map(|(name, _)| name.len() + member.len() + 1)
                .sum::<usize>();
        let lexical_work = lookup_work(4, 256, member.len(), Span::default()).unwrap();
        assert!(lexical_work >= 4 * 8 * 12 * (member.len() + 1));
        let ty = Type::Record {
            name: "Wide".into(),
            fields,
        };
        let boundary = field_work + lexical_work;
        let limits = Limits::default();
        for (work, success) in [(boundary, true), (boundary - 1, false), (boundary, true)] {
            let mut state = closures::State::new(0, &limits);
            state
                .snapshot(limits.max_analysis_steps - work, Span::default())
                .unwrap();
            state.snapshot(lexical_work, Span::default()).unwrap();
            let result = selected(&ty, &member, &mut state, Span::default());
            if success {
                assert_eq!(result.unwrap().unwrap().0, 63);
                assert_eq!(state.remaining_work(), 0)
            } else {
                assert_eq!(
                    result.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit
                )
            }
        }
    }
}
