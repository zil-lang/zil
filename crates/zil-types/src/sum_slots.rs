//! Mutable sums use ordinary complete Package storage, never an ABI wrapper.
use super::*;
impl Builder<'_, '_> {
    pub(super) fn sum_slot(&mut self, name: &str, ty: Type, value: ValueId, span: Span) -> R<()> {
        let work = inference::type_snapshot_work(&ty, span, self.limits)?;
        self.closures
            .snapshot(work.saturating_mul(3).saturating_add(name.len()), span)?;
        let package = Type::Package {
            name: format!(
                "$sum240${:?}${}",
                self.function.id,
                self.function.locals.len()
            ),
            fields: vec![("$value240".into(), ty.clone())],
        };
        let value = self.value(
            InstructionKind::MakePackage {
                fields: vec![value],
            },
            package.clone(),
            span,
        )?;
        let local = self.local(name, package, true, span)?;
        self.store(local, value, span)?;
        self.sum_slots.insert(local, ty);
        Ok(())
    }
    pub(super) fn take_sum_slot(
        &mut self,
        local: LocalId,
        expected: Option<&Type>,
        span: Span,
        replacement: bool,
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
        self.closures.snapshot(work.saturating_mul(4), span)?;
        let ty = ty.clone();
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        // Outside the proved self argument, move the complete wrapper first.
        // This prevents a later assignment from reviving an already moved name.
        let source = if replacement {
            local
        } else {
            let package = self.function.locals[local.0 as usize].ty.clone();
            let value = self.value(InstructionKind::TakeLocal { local }, package.clone(), span)?;
            let temporary = self.temp(package, span)?;
            self.store(temporary, value, span)?;
            temporary
        };
        let value = self
            .internal_product_field(
                InstructionKind::TakePackageField {
                    local: source,
                    index: 0,
                },
                Some(ty.clone()),
                span,
            )?
            .expect("sum field value");
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
                || !signature.callable.is_empty()
                || signature.state_parameter.is_some()
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
        self.replacement_input = previous;
        let (value, actual) = result?;
        self.same(&actual, &ty, rhs.span)?;
        let prepared = self.temp(ty.clone(), span)?;
        self.store(prepared, value, span)?;
        self.internal_product_field(
            InstructionKind::DropPackageField { local, index: 0 },
            None,
            span,
        )?;
        let value = self.value(InstructionKind::TakeLocal { local: prepared }, ty, span)?;
        self.internal_product_field(
            InstructionKind::StorePackageField {
                local,
                index: 0,
                value,
            },
            None,
            span,
        )?;
        Ok(true)
    }
}
