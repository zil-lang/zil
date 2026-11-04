//! Source preparation for the sealed scalar cooperative task ABI.
use super::*;

pub(super) fn resource(ty: &Type) -> bool {
    matches!(ty, Type::Managed(inner) | Type::Desc(inner) if **inner == Type::TaskI32)
}
pub(super) fn primitive(expression: &CoreExpr) -> Option<&str> {
    let CoreExprKind::Name(name) = &expression.kind else { return None; };
    matches!(name.as_str(), "task_start" | "task_yield" | "task_valid" | "task_poll" | "task_request_cancel" | "task_poll_close" | "task_finish_destroy").then_some(name)
}
pub(super) fn owner() -> Type { Type::Managed(Box::new(Type::TaskI32)) }
pub(super) fn head() -> Type { Type::Address { mutable: true, inner: Box::new(owner()) } }
pub(super) fn parameters(name: &str) -> Vec<Type> {
    match name {
        "task_yield" => vec![],
        "task_start" => vec![Type::I32], // Static worker is resolved separately.
        "task_valid" => vec![Type::Desc(Box::new(Type::TaskI32))],
        "task_finish_destroy" => vec![owner()],
        _ => vec![head()],
    }
}
pub(super) fn result(name: &str) -> Type {
    match name {
        "task_start" => owner(),
        "task_valid" | "task_poll_close" => Type::Bool,
        "task_poll" => Type::Tuple(vec![Type::I32, Type::I32]),
        _ => Type::Unit,
    }
}
pub(super) fn add_definition(functions: &[TypedFunction], definitions: &mut Vec<ResourceDefinition>, limits: &Limits) -> R<()> {
    let mut left = limits.max_analysis_steps;
    let mut pending = Vec::new();
    for f in functions {
        let n = f.values.len().saturating_add(f.locals.len()).saturating_add(1);
        left = left.checked_sub(n).ok_or_else(|| Diagnostic::limit(Phase::Types, "task resource scan budget exceeded", None))?;
        pending.extend(f.values.iter().chain(f.locals.iter().map(|l| &l.ty)).chain(std::iter::once(&f.return_type)));
    }
    while let Some(ty) = pending.pop() {
        if resource(ty) {
            definitions.push(ResourceDefinition { name: TASK_I32_RESOURCE_NAME.into(), payload: Type::TaskI32, teardown: None });
            return Ok(());
        }
        let children: &[Type] = match ty { Type::Tuple(fields) => fields, _ => &[] };
        let child = match ty { Type::Address { inner, .. } | Type::Managed(inner) | Type::Desc(inner) => Some(inner.as_ref()), _ => None };
        left = left.checked_sub(children.len().saturating_add(usize::from(child.is_some()))).ok_or_else(|| Diagnostic::limit(Phase::Types, "task resource scan budget exceeded", None))?;
        pending.extend(children);
        pending.extend(child);
    }
    Ok(())
}
impl Builder<'_, '_> {
    pub(super) fn task_call(&mut self, name: &str, arguments: &[CoreExpr], expected: Option<&Type>, span: Span) -> R<(ValueId, Type)> {
        let parameters = parameters(name);
        let offset = usize::from(name == "task_start");
        if arguments.len() != parameters.len() + offset { return Err(error("E-CALL-ARITY", span, "task primitive argument count mismatch")); }
        let kind = if name == "task_start" {
            let worker_expr = &arguments[0];
            if !matches!(worker_expr.kind, CoreExprKind::Name(_) | CoreExprKind::Member { .. }) { return Err(error("E-TASK-WORKER", worker_expr.span, "task worker must be a static concrete function")); }
            let signature = self.resolve_callee(worker_expr)?.clone();
            if !signature.scalar_thread_worker() || signature.parameters.as_slice() != [Type::I32] { return Err(error("E-TASK-WORKER", worker_expr.span, "task worker requires exact ordinary fn(i32)->i32")); }
            let worker = signature.concrete_id(worker_expr.span)?;
            let (argument, ty) = self.expr(&arguments[1], Some(&Type::I32))?;
            self.same(&ty, &Type::I32, arguments[1].span)?;
            InstructionKind::TaskStart { worker, argument }
        } else if name == "task_yield" { InstructionKind::TaskYield } else {
            let (task, ty) = self.argument_expr(&arguments[0], &parameters[0])?;
            self.same(&ty, &parameters[0], arguments[0].span)?;
            match name {
                "task_valid" => InstructionKind::TaskValid { task },
                "task_poll" => InstructionKind::TaskPoll { task },
                "task_request_cancel" => InstructionKind::TaskRequestCancel { task },
                "task_poll_close" => InstructionKind::TaskPollClose { task },
                "task_finish_destroy" => InstructionKind::TaskFinishDestroy { task },
                _ => unreachable!(),
            }
        };
        let ty = result(name);
        if let Some(expected) = expected { self.same(&ty, expected, span)?; }
        Ok((self.value(kind, ty.clone(), span)?, ty))
    }
}
