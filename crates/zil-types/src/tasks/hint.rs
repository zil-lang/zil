//! Borrowed task hints retain ordinary projection/ascription/sequence behavior.
//! This chooses an expected boundary type; ordinary expression checking still
//! proves the actual operand, owner and address permission.
use super::*;
fn pay(left: &mut usize, n: usize, span: Span) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "task type hint work budget exceeded",
            Some(span),
        )
    })?;
    Ok(())
}
fn payload_view(t: &Type) -> &Type {
    let t = match t {
        Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner) => inner.as_ref(),
        _ => t,
    };
    if let Type::Managed(inner) = t {
        inner
    } else {
        t
    }
}
impl Builder<'_, '_> {
    pub(super) fn borrowed_task_hint<'b>(
        &'b self,
        e: &'b CoreExpr,
        left: &mut usize,
        depth: usize,
    ) -> R<Option<&'b Type>> {
        pay(left, 1, e.span)?;
        if depth > self.limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "task type hint depth exceeded",
                Some(e.span),
            ));
        }
        Ok(match &e.kind {
            CoreExprKind::Name(name) => {
                pay(
                    left,
                    name.len().saturating_add(1).saturating_mul(
                        self.function
                            .locals
                            .len()
                            .checked_ilog2()
                            .map_or(1, |n| n as usize + 2),
                    ),
                    e.span,
                )?;
                self.lookup(name).and_then(|id| {
                    self.borrowed_captures.get(&id).or_else(|| {
                        if self.closure_slots.contains_key(&id) {
                            None
                        } else {
                            self.sum_slots
                                .get(&id)
                                .or_else(|| self.function.locals.get(id.0 as usize).map(|l| &l.ty))
                        }
                    })
                })
            }
            CoreExprKind::Ascribed { ty, .. } | CoreExprKind::Record { ty, .. } => Some(ty),
            CoreExprKind::RecordUpdate { ty, base, .. } => match ty {
                Some(t) => Some(t),
                None => self.borrowed_task_hint(base, left, depth + 1)?,
            },
            CoreExprKind::Sequence { value, .. } | CoreExprKind::Unary { operand: value, .. } => {
                self.borrowed_task_hint(value, left, depth + 1)?
            }
            CoreExprKind::Try(value) => match self.borrowed_task_hint(value, left, depth + 1)? {
                Some(Type::ErrorUnion { result, .. }) => Some(result.as_ref()),
                _ => None,
            },
            CoreExprKind::If {
                then_value,
                else_value,
                ..
            } => match self.borrowed_task_hint(then_value, left, depth + 1)? {
                Some(t) => Some(t),
                None => self.borrowed_task_hint(else_value, left, depth + 1)?,
            },
            CoreExprKind::Index { object, .. } => match self
                .borrowed_task_hint(object, left, depth + 1)?
                .map(payload_view)
            {
                Some(Type::Array { element, .. } | Type::Slice { element }) => {
                    Some(element.as_ref())
                }
                _ => None,
            },
            CoreExprKind::Member { object, member, .. } => {
                pay(left, member.len().saturating_add(1), e.span)?;
                if let CoreExprKind::Tuple(elements) = &object.kind {
                    if let Some(element) =
                        member.parse::<usize>().ok().and_then(|i| elements.get(i))
                    {
                        self.borrowed_task_hint(element, left, depth + 1)?
                    } else {
                        None
                    }
                } else {
                    match self
                        .borrowed_task_hint(object, left, depth + 1)?
                        .map(payload_view)
                    {
                        Some(Type::Tuple(fields)) => {
                            member.parse::<usize>().ok().and_then(|i| fields.get(i))
                        }
                        Some(Type::Record { fields, .. } | Type::Package { fields, .. }) => {
                            pay(
                                left,
                                fields.len().saturating_mul(member.len().saturating_add(1)),
                                e.span,
                            )?;
                            fields
                                .iter()
                                .find(|(name, _)| name == member)
                                .map(|(_, t)| t)
                        }
                        _ => None,
                    }
                }
            }
            CoreExprKind::Call { callee, .. } => {
                if let CoreExprKind::Member { object, member, .. } = &callee.kind
                    && let Some(t) = self.borrowed_task_hint(object, left, depth + 1)?
                {
                    inference::charge_type_snapshot(t, e.span, self.limits, left, 1)?;
                    pay(
                        left,
                        member
                            .len()
                            .saturating_add(self.signatures.len())
                            .saturating_add(self.imports.map_or(0, |i| i.aliases.len())),
                        e.span,
                    )?;
                    if let Some(signature) = aggregates::resolve_type_method(
                        payload_view(t),
                        member,
                        self.signatures,
                        self.imports,
                    ) {
                        return Ok(Some(&signature.result));
                    }
                }
                if let Some(t) = self.borrowed_task_hint(callee, left, depth + 1)? {
                    match t {
                        Type::FunctionPointer { result, .. }
                        | Type::BorrowedFunctionPointer { result, .. } => return Ok(Some(result)),
                        Type::Closure(id) => {
                            return self.closures.borrowed_task_result(*id, left, e.span);
                        }
                        _ => {}
                    }
                }
                if let CoreExprKind::Name(name) = &callee.kind
                    && let Some(id) = self
                        .lookup(name)
                        .and_then(|l| self.closure_slots.get(&l))
                        .copied()
                {
                    return self.closures.borrowed_task_result(id, left, e.span);
                }
                let bytes = match &callee.kind {
                    CoreExprKind::Name(name) => name.len(),
                    CoreExprKind::Member { object, member, .. } => match &object.kind {
                        CoreExprKind::Name(alias) => {
                            alias.len().saturating_add(member.len()).saturating_add(1)
                        }
                        _ => 0,
                    },
                    _ => 0,
                };
                pay(
                    left,
                    bytes
                        .saturating_add(1)
                        .saturating_mul(
                            self.signatures
                                .len()
                                .checked_ilog2()
                                .map_or(1, |n| n as usize + 2),
                        )
                        .saturating_mul(12),
                    e.span,
                )?;
                self.resolve_callee(callee).ok().map(|s| &s.result)
            }
            _ => None,
        })
    }
}
