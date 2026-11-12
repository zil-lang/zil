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
fn numeric_lookup(left: &mut usize, n: usize, span: Span) -> R<()> {
    pay(
        left,
        n.checked_ilog2()
            .map_or(1, |h| h as usize + 2)
            .saturating_mul(12),
        span,
    )
}
fn string_lookup(left: &mut usize, key: &str, n: usize, span: Span) -> R<()> {
    pay(
        left,
        key.len()
            .saturating_add(1)
            .saturating_mul(n.checked_ilog2().map_or(1, |h| h as usize + 2))
            .saturating_mul(12),
        span,
    )
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
    fn hint_local(&self, name: &str, left: &mut usize, span: Span) -> R<Option<LocalId>> {
        for scope in self.scopes.iter().rev() {
            string_lookup(left, name, scope.len(), span)?;
            if let Some(local) = scope.get(name) {
                return Ok(Some(*local));
            }
        }
        Ok(None)
    }
    fn hint_static(&self, callee: &CoreExpr, left: &mut usize) -> R<Option<&Signature>> {
        let span = callee.span;
        match &callee.kind {
            CoreExprKind::Name(name) => {
                if self.hint_local(name, left, span)?.is_some() {
                    return Ok(None);
                }
                string_lookup(left, name, self.signatures.len(), span)?;
                Ok(self.signatures.get(name))
            }
            CoreExprKind::Member {
                object,
                member,
                owner,
            } if owner.is_none() => {
                let CoreExprKind::Name(alias) = &object.kind else {
                    return Ok(None);
                };
                if self.hint_local(alias, left, span)?.is_some() {
                    return Ok(None);
                }
                pay(
                    left,
                    alias.len().saturating_add(member.len()).saturating_add(1),
                    span,
                )?;
                let key = format!("{alias}.{member}");
                string_lookup(left, &key, self.signatures.len(), span)?;
                if let Some(signature) = self.signatures.get(&key) {
                    return Ok(Some(signature));
                }
                let Some(imports) = self.imports else {
                    return Ok(None);
                };
                string_lookup(left, alias, imports.aliases.len(), span)?;
                let Some(module) = imports.aliases.get(alias) else {
                    return Ok(None);
                };
                numeric_lookup(left, imports.signatures.len(), span)?;
                let Some(table) = imports.signatures.get(module) else {
                    return Ok(None);
                };
                string_lookup(left, member, table.len(), span)?;
                Ok(table.get(member).filter(|s| s.public))
            }
            _ => Ok(None),
        }
    }
    fn hint_method(
        &self,
        ty: &Type,
        member: &str,
        left: &mut usize,
        span: Span,
    ) -> R<Option<&Signature>> {
        let (name, nominal) = match ty {
            Type::Record { name, fields } | Type::Package { name, fields } => {
                pay(
                    left,
                    fields
                        .len()
                        .saturating_mul(member.len().saturating_add(1))
                        .saturating_add(name.len()),
                    span,
                )?;
                if fields.iter().any(|(n, _)| n == member) {
                    return Ok(None);
                }
                let Some((owner, nominal)) = name.split_once(':') else {
                    return Ok(None);
                };
                (Some(owner), nominal)
            }
            Type::ByteBuffer => (None, "ByteBuffer"),
            _ => return Ok(None),
        };
        pay(
            left,
            nominal.len().saturating_add(member.len()).saturating_add(1),
            span,
        )?;
        let key = format!("{nominal}.{member}");
        string_lookup(left, &key, self.signatures.len(), span)?;
        if let Some(signature) = self
            .signatures
            .get(&key)
            .filter(|s| s.receiver.is_some() && (name.is_some() || !s.cleanup))
        {
            return Ok(Some(signature));
        }
        let Some(owner) = name else {
            return Ok(None);
        };
        let Some(id) = owner
            .strip_prefix("@module")
            .and_then(|s| s.parse().ok())
            .map(zil_base::ModuleId)
        else {
            return Ok(None);
        };
        let Some(imports) = self.imports else {
            return Ok(None);
        };
        pay(left, imports.aliases.len(), span)?;
        if !imports.aliases.values().any(|m| *m == id) {
            return Ok(None);
        }
        numeric_lookup(left, imports.signatures.len(), span)?;
        let Some(table) = imports.signatures.get(&id) else {
            return Ok(None);
        };
        string_lookup(left, &key, table.len(), span)?;
        Ok(table
            .get(&key)
            .filter(|s| s.public && s.receiver.is_some() && !s.cleanup))
    }
    pub(crate) fn borrowed_task_hint<'b>(
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
                if let Some(id) = self.hint_local(name, left, e.span)? {
                    numeric_lookup(left, self.borrowed_captures.len(), e.span)?;
                    if let Some(t) = self.borrowed_captures.get(&id) {
                        Some(t)
                    } else {
                        numeric_lookup(left, self.closure_slots.len(), e.span)?;
                        if self.closure_slots.contains_key(&id) {
                            None
                        } else {
                            numeric_lookup(left, self.sum_slots.len(), e.span)?;
                            self.sum_slots
                                .get(&id)
                                .or_else(|| self.function.locals.get(id.0 as usize).map(|l| &l.ty))
                        }
                    }
                } else {
                    None
                }
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
                    && let Some(signature) =
                        self.hint_method(payload_view(t), member, left, e.span)?
                {
                    return Ok(Some(&signature.result));
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
                    && let Some(local) = self.hint_local(name, left, e.span)?
                {
                    numeric_lookup(left, self.closure_slots.len(), e.span)?;
                    if let Some(id) = self.closure_slots.get(&local).copied() {
                        return self.closures.borrowed_task_result(id, left, e.span);
                    }
                }
                self.hint_static(callee, left)?.map(|s| &s.result)
            }
            _ => None,
        })
    }
}

#[cfg(test)]
mod hint_budget {
    use super::*;
    fn signature(result: Type, receiver: Option<Type>) -> Signature {
        Signature {
            result_mode:PassingMode::Infer,
            storage_life: None,
            life: None,
            id: Some(FunctionId(1)),
            callable: vec![],
            state_parameter: None,
            callable_return: None,
            type_parameter: None,
            template: None,
            foreign: false,
            scoped_callback: None,
            cleanup: false,
            parameters: vec![],
            result,
            public: true,
            receiver,
        }
    }
    fn function() -> CoreFunction {
        CoreFunction {
            storage_life: None,
            call_kind: None,
            resume_bound: zil_ir::TaskResumeBound::Local,
            life: None,
            type_parameters: vec![],
            state_parameters: vec![],
            protocol_contract: None,
            callable_return: None,
            id: zil_base::NodeId(0),
            name: "hint".into(),
            callable_parameters: vec![],
            foreign: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            body: CoreBlock {
                statements: vec![],
                span: Span::default(),
            },
            receiver: None,
            span: Span::default(),
        }
    }
    fn expr(kind: CoreExprKind) -> CoreExpr {
        CoreExpr {
            id: zil_base::NodeId(0),
            kind,
            span: Span::default(),
        }
    }
    // Exercise the actual borrowed resolver in isolation, so the boundary
    // belongs to its own live ledger rather than an unrelated earlier pass.
    fn run(case: usize, prefix: usize, amount: usize) -> R<(Type, usize)> {
        let limits = Limits::default();
        let task = Type::Desc(Box::new(Type::Task {
            result: Box::new(Type::I32),
            retained_life: super::super::empty_context(),
        }));
        let common = "long_common_prefix_".repeat(prefix);
        let member = format!("{common}method");
        let nominal = format!("{common}Payload");
        let alias = format!("{common}library");
        let key = if case == 2 {
            format!("{nominal}.{member}")
        } else {
            member.clone()
        };
        let mut table = BTreeMap::new();
        for index in 0..32 {
            table.insert(
                format!("{common}method_{index:03}"),
                signature(Type::I32, None),
            );
        }
        table.insert(
            key,
            signature(task.clone(), (case == 2).then_some(Type::I32)),
        );
        let tables = BTreeMap::from([(zil_base::ModuleId(1), table)]);
        let imports = program::Imports {
            aliases: BTreeMap::from([(alias.clone(), zil_base::ModuleId(1))]),
            signatures: &tables,
        };
        let local_signatures = (0..32)
            .map(|i| (format!("{common}local_{i:03}"), signature(Type::I32, None)))
            .collect();
        let mut state = closures::State::new(0, &limits);
        let f = function();
        let mut builder = Builder::new(&f, FunctionId(0), &local_signatures, &limits, &mut state)?;
        builder.imports = Some(&imports);
        let local_ty = if case == 2 {
            Type::Record {
                name: format!("@module1:{nominal}"),
                fields: vec![],
            }
        } else {
            task.clone()
        };
        builder.function.locals.push(LocalDecl {
            id: LocalId(0),
            name: "root".into(),
            ty: local_ty,
            mutable: false,
            span: Span::default(),
        });
        builder.scopes[0].insert("root".into(), LocalId(0));
        if case == 0 {
            for i in 0..64 {
                builder
                    .scopes
                    .push(BTreeMap::from([(format!("{common}scope{i}"), LocalId(0))]));
            }
        }
        let e = if case == 0 {
            expr(CoreExprKind::Name("root".into()))
        } else {
            expr(CoreExprKind::Call {
                callee: Box::new(expr(CoreExprKind::Member {
                    object: Box::new(expr(CoreExprKind::Name(if case == 2 {
                        "root".into()
                    } else {
                        alias
                    }))),
                    member,
                    owner: None,
                })),
                arguments: vec![],
            })
        };
        let mut left = amount;
        let found = builder
            .borrowed_task_hint(&e, &mut left, 0)?
            .expect("actual scoped/imported task result");
        assert_eq!(found, &task);
        Ok((found.clone(), left))
    }
    #[test]
    fn scoped_imported_long_prefix_lookups_have_exact_live_budget_boundaries() {
        for case in 0..3 {
            let mut growth = vec![];
            for prefix in [1, 32] {
                let (_, left) = run(case, prefix, 1_000_000).unwrap();
                let n = 1_000_000 - left;
                for (amount, success) in [(n, true), (n - 1, false), (n, true)] {
                    let result = run(case, prefix, amount);
                    assert_eq!(
                        result.is_ok(),
                        success,
                        "case={case} prefix={prefix} n={n}: {result:?}"
                    );
                    if !success {
                        let error = result.unwrap_err();
                        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
                        assert!(error.message.contains("task type hint work"));
                    }
                }
                growth.push(n);
                eprintln!("hint exact case={case} prefix={prefix} N={n}");
            }
            if case != 0 {
                assert!(
                    growth[1] > growth[0] * 8,
                    "long keys must increase comparison reservation: {growth:?}"
                );
            }
        }
    }
}
