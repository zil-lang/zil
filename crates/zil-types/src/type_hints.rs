//! Owned type hints use the request's Array snapshot ledger. Hints never grant
//! source, Copy or address authority; ordinary expression checking still runs.
use super::*;
impl<'a> Builder<'a, '_> {
    pub(super) fn known_type_checked(&mut self, e: &CoreExpr) -> R<Option<Type>> {
        let related = match &e.kind {
            CoreExprKind::Ascribed { ty, .. }
            | CoreExprKind::Record { ty, .. }
            | CoreExprKind::IntegerConvert { target: ty, .. }
            | CoreExprKind::NumericConvert { target: ty, .. } => stored::shared_array_related(ty),
            CoreExprKind::RecordUpdate { ty: Some(ty), .. } => stored::shared_array_related(ty),
            _ => false,
        };
        if !self.closures.array_billing && !related {
            return Ok(self.known_type(e));
        }
        self.closures.array_billing = true;
        if let Some(actual) = self.known_type_ref(e) {
            let work = inference::type_snapshot_work(actual, e.span, self.limits)?;
            self.closures.snapshot(work, e.span)?;
            return Ok(self.known_type_ref(e).cloned());
        }
        if let CoreExprKind::Call { callee, arguments } = &e.kind {
            if let Some(name) = dynamic_store::primitive(callee)
                && let Some(first) = arguments.first()
                && let Some(ty) = self.known_type_checked(first)?
                && let Some(element) = dynamic_store::element(&ty)
            {
                self.closures.array_snapshot(element, 1, e.span)?;
                return Ok(Some(dynamic_store::result_type(name, element)));
            }
            if let Some(name) = byte_buffer::primitive(callee) {
                return Ok(byte_buffer::result_type(name));
            }
        }
        if let Ok(Some((ty, _))) = self.enum_constructor(e)
            && !matches!(ty, Type::BorrowedStorage { .. })
        {
            self.closures.array_snapshot(ty, 1, e.span)?;
            return Ok(Some(ty.clone()));
        }
        if let CoreExprKind::Call { callee, .. } = &e.kind
            && let Ok(Some((ty, _))) = self.enum_constructor(callee)
            && !matches!(ty, Type::BorrowedStorage { .. })
        {
            self.closures.array_snapshot(ty, 1, e.span)?;
            return Ok(Some(ty.clone()));
        }
        Ok(match &e.kind {
            CoreExprKind::IntegerConvert { target, .. } | CoreExprKind::NumericConvert { target, .. } => {
                self.closures.array_snapshot(target, 1, e.span)?;
                Some(target.clone())
            }
            CoreExprKind::Name(name) => {
                if let Some(local) = self.lookup(name) {
                    self.closure_slots.get(&local).copied().map(Type::Closure)
                } else if let Some(signature) = self.signatures.get(name) {
                    self.hint_pointer(signature, e.span)?
                } else { None }
            }
            CoreExprKind::Try(value) => match self.known_type_checked(value)? {
                Some(Type::ErrorUnion { result, .. }) => Some(*result), _ => None,
            },
            CoreExprKind::Index { object, .. } => match self.known_type_checked(object)? {
                Some(Type::Array { element, .. }) => Some(*element),
                Some(Type::Address { inner, .. }) => match *inner { Type::Array { element, .. } | Type::Slice { element } => Some(*element), _ => None }, _ => None,
            },
            CoreExprKind::Slice { object, .. } => {
                let Some(base) = self.known_type_checked(object)? else { return Ok(None) };
                let inner = if let Type::Address { inner, .. } = base { *inner } else { base };
                match inner { Type::Array { element, .. } | Type::Slice { element } if element.is_copy_value() => Some(Type::Slice { element }), _ => None }
            }
            CoreExprKind::Member { object, member, .. } => {
                if matches!(&object.kind, CoreExprKind::Name(name) if self.lookup(name).is_none()) {
                    match self.resolve_callee(e).ok() { Some(s) => self.hint_pointer(s, e.span)?, None => None }
                } else {
                    let Some(base) = self.known_type_checked(object)? else { return Ok(None) };
                    let base = match &base { Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner) => inner.as_ref(), other => other };
                    match aggregates::field_ref(base, member, e.span).ok() {
                        Some((_, field)) => { self.closures.array_snapshot(field, 1, e.span)?; let field = field.clone(); Some(if matches!(base, Type::Package { .. }) { packages::projection_type(field) } else { field }) }, None => None,
                    }
                }
            }
            CoreExprKind::RecordUpdate { ty, base, .. } => if let Some(ty) = ty { self.closures.array_snapshot(ty, 1, e.span)?; Some(ty.clone()) } else { self.known_type_checked(base)? },
            CoreExprKind::Sequence { value, .. } => self.known_type_checked(value)?,
            CoreExprKind::Tuple(values) => {
                self.closures.snapshot(values.len(), e.span)?;
                let mut types = Vec::new();
                for value in values { let Some(ty) = self.known_type_checked(value)? else { return Ok(None) }; types.push(ty); }
                Some(Type::Tuple(types))
            }
            CoreExprKind::Unary { op: UnaryOp::Not, .. } => Some(Type::Bool),
            CoreExprKind::Unary { op: op @ (UnaryOp::Borrow | UnaryOp::BorrowMut), operand } => self.known_type_checked(operand)?.filter(|ty| ty.is_address_pointee() || stored::shared_value(ty) || (*op == UnaryOp::BorrowMut && matches!(ty, Type::Managed(_)) && tasks::resource(ty)) || owned_shapes::sum(ty) || matches!(ty,Type::Package { .. }) || (*op == UnaryOp::Borrow && matches!(ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::Record { .. }) && payload.is_copy_value()))).map(|inner|Type::Address { mutable:*op == UnaryOp::BorrowMut, inner:Box::new(inner) }),
            CoreExprKind::Unary { op: UnaryOp::Dereference, operand } => match self.known_type_checked(operand)? { Some(Type::Address { inner, .. }) if inner.is_address_pointee() || stored::shared_value(&inner) || owned_shapes::sum(&inner) || matches!(inner.as_ref(),Type::Package { .. }) => Some(*inner), _ => None },
            CoreExprKind::Unary { op: UnaryOp::Give, operand } => {
                if let CoreExprKind::Member { object, member, .. } = &operand.kind
                    && let CoreExprKind::Name(name) = &object.kind
                    && let Some(local) = self.lookup(name)
                    && matches!(stored::physical(&self.function.locals[local.0 as usize].ty),Type::Package { .. })
                    && let Ok((_, actual)) = aggregates::field_ref(&self.function.locals[local.0 as usize].ty, member, operand.span)
                { self.closures.array_snapshot(actual, 1, e.span)?; Some(actual.clone()) } else { self.known_type_checked(operand)? }
            }
            CoreExprKind::Unary { operand, .. } => self.known_type_checked(operand)?,
            CoreExprKind::Call { callee, .. } => {
                if let CoreExprKind::Member { object, member, .. } = &callee.kind {
                    if member == "len" && self.known_type_checked(object)?.is_some_and(|ty|ty.is_slice_address()) { return Ok(Some(Type::I64)) }
                    if let Some(signature) = self.method_signature_checked(object, member)? { self.closures.array_snapshot(&signature.result, 1, e.span)?; return Ok(Some(signature.result.clone())) }
                }
                match self.known_type_checked(callee)? {
                    Some(Type::Closure(id)) => self.closures.pending[id.0 as usize].inference.result().ok().flatten(),
                    Some(Type::FunctionPointer { result, .. } | Type::BorrowedFunctionPointer { result, .. }) => Some(*result),
                    _ => if let Ok(signature) = self.resolve_callee(callee) { self.closures.array_snapshot(&signature.result, 1, e.span)?; Some(signature.result.clone()) } else { None },
                }
            }
            CoreExprKind::If { then_value, else_value, .. } => match self.known_type_checked(then_value)? { Some(ty) => Some(ty), None => self.known_type_checked(else_value)? },
            // Literal/scalar fallbacks retain no variable-sized target type.
            _ => self.known_type(e),
        })
    }
    fn hint_pointer(&mut self, signature: &Signature, span: Span) -> R<Option<Type>> {
        for ty in &signature.parameters {
            self.closures.array_snapshot(ty, 4, span)?;
        }
        self.closures.array_snapshot(&signature.result, 5, span)?;
        borrowed_fp::static_pointer_slots(signature, |work| self.closures.snapshot(work, span))?;
        self.closures
            .snapshot(borrowed_fp::pointer_work(signature), span)?;
        Ok(signature.pointer_type())
    }
    pub(super) fn method_signature_checked(
        &mut self,
        object: &CoreExpr,
        member: &str,
    ) -> R<Option<&'a Signature>> {
        if !self.closures.array_billing || self.known_type_ref(object).is_some() {
            return Ok(self.method_signature(object, member));
        }
        let Some(ty) = self.known_type_checked(object)? else {
            return Ok(None);
        };
        let ty = match ty {
            Type::Address { inner, .. } => match *inner {
                Type::Managed(payload) => *payload,
                other => other,
            },
            Type::Managed(inner) | Type::Desc(inner) => *inner,
            other => other,
        };
        Ok(aggregates::resolve_type_method(
            &ty,
            member,
            self.signatures,
            self.imports,
        ))
    }
    pub(super) fn range_hint_checked(&mut self, e: &CoreExpr, depth: usize) -> R<Option<Type>> {
        if !self.closures.array_billing {
            return Ok(self.range_hint(e, depth));
        }
        if depth > self.limits.max_call_depth {
            return Ok(None);
        }
        match &e.kind {
            CoreExprKind::Unary {
                op: UnaryOp::Not, ..
            } => Ok(Some(Type::Bool)),
            CoreExprKind::Unary {
                op: UnaryOp::Borrow | UnaryOp::Dereference,
                ..
            } => self.known_type_checked(e),
            CoreExprKind::Unary { operand, .. } | CoreExprKind::Sequence { value: operand, .. } => {
                self.range_hint_checked(operand, depth + 1)
            }
            CoreExprKind::Binary {
                op:
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::And
                    | BinaryOp::Or,
                ..
            } => Ok(Some(Type::Bool)),
            CoreExprKind::Binary { left, right, .. }
            | CoreExprKind::If {
                then_value: left,
                else_value: right,
                ..
            } => match self.range_hint_checked(left, depth + 1)? {
                Some(ty) => Ok(Some(ty)),
                None => self.range_hint_checked(right, depth + 1),
            },
            _ => self.known_type_checked(e),
        }
    }
    pub(super) fn known_type_is_checked(
        &mut self,
        e: &CoreExpr,
        predicate: impl Fn(&Type) -> bool,
    ) -> R<bool> {
        if let Some(actual) = self.known_type_ref(e) {
            return Ok(predicate(actual));
        }
        Ok(self.known_type_checked(e)?.as_ref().is_some_and(predicate))
    }
    pub(super) fn known_address_checked(&mut self, e: &CoreExpr) -> R<bool> {
        self.known_type_is_checked(e, |ty| matches!(ty, Type::Address { .. }))
    }
    pub(super) fn sequence_hints_checked(&mut self, value: &CoreExpr) -> R<BTreeMap<String, Type>> {
        if !self.closures.array_billing {
            return Ok(self.sequence_hints(value));
        }
        let owned;
        let (ty, fields) = match &value.kind {
            CoreExprKind::Record { ty, fields } => (Some(ty), fields),
            CoreExprKind::RecordUpdate { ty, base, fields } => {
                owned = if ty.is_none() {
                    self.known_type_checked(base)?
                } else {
                    None
                };
                (ty.as_ref().or(owned.as_ref()), fields)
            }
            _ => return Ok(BTreeMap::new()),
        };
        let mut hints = BTreeMap::new();
        if let Some(ty) = ty {
            let payload = if let Type::Managed(payload) = ty {
                payload.as_ref()
            } else {
                ty
            };
            for (name, e) in fields {
                if let CoreExprKind::Name(temp) = &e.kind
                    && let Ok((_, actual)) = aggregates::field_ref(payload, name, e.span)
                {
                    self.closures.array_snapshot(actual, 1, e.span)?;
                    self.closures
                        .snapshot(temp.len().saturating_add(1), e.span)?;
                    hints.insert(temp.clone(), actual.clone());
                }
            }
        }
        Ok(hints)
    }
}
