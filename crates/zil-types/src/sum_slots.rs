//! Mutable owning sums keep their actual type and a single lexical owner.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn sum_slot(&mut self, name: &str, ty: Type, value: ValueId, span: Span) -> R<()> {
        let work = inference::type_snapshot_work(&ty, span, self.limits)?;
        self.closures
            .snapshot(work.saturating_add(name.len()), span)?;
        let local = self.local(name, ty.clone(), true, span)?;
        self.store(local, value, span)?;
        if dynamic_store::contains_inline_carrier(&ty) {
            self.closures.snapshot(
                self.inline_store_sum_slots.len().saturating_add(1).ilog2() as usize + 1,
                span,
            )?;
            self.inline_store_sum_slots.insert(local);
        }
        self.sum_slots.insert(local, ty);
        Ok(())
    }
    pub(super) fn take_sum_slot(
        &mut self,
        local: LocalId,
        expected: Option<&Type>,
        span: Span,
    ) -> R<Option<(ValueId, Type)>> {
        if self.sum_slots.is_empty() {
            return Ok(None);
        }
        self.closures.snapshot(
            self.sum_slots.len().saturating_add(1).ilog2() as usize + 1,
            span,
        )?;
        let Some(ty) = self.sum_slots.get(&local) else {
            return Ok(None);
        };
        let work = inference::type_snapshot_work(ty, span, self.limits)?;
        let copies_and_comparison = 2 + usize::from(expected.is_some());
        self.closures
            .snapshot(work.saturating_mul(copies_and_comparison), span)?;
        let ty = ty.clone();
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        let value = self.value(InstructionKind::TakeLocal { local }, ty.clone(), span)?;
        Ok(Some((value, ty)))
    }
    pub(super) fn replace_sum_slot(
        &mut self,
        local: LocalId,
        rhs: &CoreExpr,
        span: Span,
    ) -> R<bool> {
        if self.sum_slots.is_empty() {
            return Ok(false);
        }
        self.closures.snapshot(
            self.sum_slots.len().saturating_add(1).ilog2() as usize + 1,
            span,
        )?;
        let Some(ty) = self.sum_slots.get(&local) else {
            return Ok(false);
        };
        let work = inference::type_snapshot_work(ty, span, self.limits)?;
        self.closures.snapshot(
            work.saturating_mul(4)
                .saturating_add(self.function.locals[local.0 as usize].name.len()),
            span,
        )?;
        let ty = ty.clone();
        let name = self.function.locals[local.0 as usize].name.clone();
        let references = product_replacement::References {
            target: &name,
            state: self.closures,
            limits: self.limits,
        }
        .expr(rhs, false, 0)?;
        let previous = self.replacement_input;
        if references {
            let CoreExprKind::Call { callee, arguments } = &rhs.kind else {
                return Err(unsupported(
                    rhs.span,
                    "Sum self replacement requires a static call with one direct whole-target argument",
                ));
            };
            let target = match &callee.kind {
                CoreExprKind::TypeApply { callee, .. } => callee.as_ref(),
                _ => callee.as_ref(),
            };
            let direct = match &target.kind {
                CoreExprKind::Name(n) => self.lookup(n).is_none(),
                CoreExprKind::Member { object, .. } => {
                    matches!(&object.kind,CoreExprKind::Name(n) if self.lookup(n).is_none())
                }
                _ => false,
            };
            if !direct {
                return Err(unsupported(
                    rhs.span,
                    "Sum self replacement requires an ordinary static function",
                ));
            }
            let signature = self.resolve_callee(target)?;
            if signature.foreign
                || signature.cleanup
                || signature.receiver.is_some()
            {
                return Err(unsupported(
                    rhs.span,
                    "Sum self replacement requires an ordinary static signature",
                ));
            }
            let mut found = false;
            for argument in arguments {
                if matches!(&argument.kind,CoreExprKind::Name(n) if n==&name && self.lookup(n)==Some(local))
                {
                    if found {
                        return Err(unsupported(
                            argument.span,
                            "Sum replacement repeats its target",
                        ));
                    }
                    found = true;
                } else if (product_replacement::References {
                    target: &name,
                    state: self.closures,
                    limits: self.limits,
                })
                .expr(argument, false, 0)?
                {
                    return Err(unsupported(
                        argument.span,
                        "other Sum replacement arguments cannot reference the target",
                    ));
                }
            }
            if !found {
                return Err(unsupported(
                    rhs.span,
                    "Sum replacement needs one direct target argument",
                ));
            }
            self.replacement_input = Some(local);
        }
        let result = self.transfer_expr(rhs, Some(&ty));
        let consumed = self.replacement_input.is_none();
        self.replacement_input = previous;
        let (value, actual) = result?;
        self.same(&actual, &ty, rhs.span)?;
        if references && !consumed {
            return Err(unsupported(
                rhs.span,
                "Sum replacement did not move its declared self argument",
            ));
        }
        let prepared = self.temp(ty.clone(), span)?;
        self.store(prepared, value, span)?;
        if !references {
            // Taking the previously initialized owner preserves rejection of
            // reassigning a name already moved by an earlier statement.
            let old = self.value(InstructionKind::TakeLocal { local }, ty.clone(), span)?;
            let floor = self.resource_temps.len();
            let retiring = self.temp(ty.clone(), span)?;
            self.store(retiring, old, span)?;
            self.finish_resource_temps_from(floor, span)?;
        }
        let value = self.value(InstructionKind::TakeLocal { local: prepared }, ty, span)?;
        self.store(local, value, span)?;
        Ok(true)
    }
}
