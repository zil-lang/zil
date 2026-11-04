//! Source preparation for the sealed scalar cooperative task ABI.
use super::*;
pub(super) fn empty_context() -> zil_ir::RetainedLifeShape {
    zil_ir::RetainedLifeShape {
        binder_count: 0,
        ports: vec![],
        retained: vec![],
        nominal_arguments: vec![],
    }
}
pub(super) fn require_closed_context(ty: &Type, span: Span) -> R<()> {
    if let Type::Task { retained_life, .. } = ty
        && (retained_life.binder_count != 0
            || !retained_life.ports.is_empty()
            || !retained_life.retained.is_empty()
            || !retained_life.nominal_arguments.is_empty())
    {
        return Err(unsupported(
            span,
            "borrowed Task retained classes require actual implemented frame port mappings",
        ));
    }
    Ok(())
}

mod hint;

pub(super) fn suspends(f: &TypedFunction) -> bool {
    f.call_kind == CallKind::Suspend
}
pub(super) fn resource(ty: &Type) -> bool {
    matches!(ty, Type::Managed(inner) | Type::Desc(inner) if matches!(inner.as_ref(), Type::TaskI32 | Type::Task { .. }))
}

pub(super) fn payload(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Managed(t) | Type::Desc(t) | Type::Address { inner: t, .. } => payload(t),
        Type::TaskI32 | Type::Task { .. } => Some(ty),
        _ => None,
    }
}
pub(super) fn create_error() -> Type {
    Type::Enum {
        name: "@std.task:v1:TaskCreateError".into(),
        variants: vec![("Allocation".into(), vec![])],
    }
}
pub(super) fn poll_state() -> Type {
    Type::Enum {
        name: "@std.task:v1:TaskPollState".into(),
        variants: ["Complete", "Cancelled", "Invalid"]
            .into_iter()
            .map(|s| (s.into(), vec![]))
            .collect(),
    }
}
// Callers prepay a complete actual-T walk before cloning/hash generation.
pub(super) fn poll_type(result: &Type) -> Type {
    Type::Enum {
        name: format!("@std.task:v1:Poll:{}", result.fingerprint()),
        variants: vec![
            ("Pending".into(), vec![]),
            ("Ready".into(), vec![result.clone()]),
        ],
    }
}
pub(super) fn create_type(result: &Type) -> Type {
    Type::ErrorUnion {
        error: Box::new(create_error()),
        result: Box::new(Type::Managed(Box::new(Type::Task {
            result: Box::new(result.clone()),
            retained_life: empty_context(),
        }))),
    }
}
pub(super) fn result_for(name: &str, task: &Type) -> Type {
    let Type::Task { result: t, .. } = task else {
        return result(name);
    };
    match name {
        "task_poll" => Type::ErrorUnion {
            error: Box::new(poll_state()),
            result: Box::new(poll_type(t)),
        },
        "task_poll_close" => poll_type(&Type::Unit),
        _ => result(name),
    }
}
pub(super) fn parameter_for(name: &str, task: &Type) -> Type {
    match name {
        "task_valid" => Type::Desc(Box::new(task.clone())),
        "task_finish_destroy" => Type::Managed(Box::new(task.clone())),
        _ => Type::Address {
            mutable: true,
            inner: Box::new(Type::Managed(Box::new(task.clone()))),
        },
    }
}

pub(super) fn primitive(expression: &CoreExpr) -> Option<&str> {
    let CoreExprKind::Name(name) = &expression.kind else {
        return None;
    };
    matches!(
        name.as_str(),
        "task_start"
            | "task_create"
            | "task_yield"
            | "task_await"
            | "task_valid"
            | "task_poll"
            | "task_request_cancel"
            | "task_poll_close"
            | "task_finish_destroy"
    )
    .then_some(name)
}
pub(super) fn owner() -> Type {
    Type::Managed(Box::new(Type::TaskI32))
}
pub(super) fn head() -> Type {
    Type::Address {
        mutable: true,
        inner: Box::new(owner()),
    }
}
pub(super) fn parameters(name: &str) -> Vec<Type> {
    match name {
        "task_yield" => vec![],
        "task_start" | "task_await" => vec![Type::I32], // Static worker is resolved separately.
        "task_valid" => vec![Type::Desc(Box::new(Type::TaskI32))],
        "task_finish_destroy" => vec![owner()],
        _ => vec![head()],
    }
}
pub(super) fn result(name: &str) -> Type {
    match name {
        "task_start" => owner(),
        "task_await" => Type::I32,
        "task_valid" | "task_poll_close" => Type::Bool,
        "task_poll" => Type::Tuple(vec![Type::I32, Type::I32]),
        _ => Type::Unit,
    }
}
pub(super) fn add_definition(
    functions: &[TypedFunction],
    definitions: &mut Vec<ResourceDefinition>,
    left: &mut usize,
    limits: &Limits,
) -> R<()> {
    let mut pending = Vec::new();
    for f in functions {
        let n = f
            .values
            .len()
            .saturating_add(f.locals.len())
            .saturating_add(1);
        *left = left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "task resource scan budget exceeded", None)
        })?;
        pending.extend(
            f.values
                .iter()
                .chain(f.locals.iter().map(|l| &l.ty))
                .chain(std::iter::once(&f.return_type)),
        );
    }
    while let Some(ty) = pending.pop() {
        if let Type::Managed(p) | Type::Desc(p) = ty
            && let Some(payload) = payload(p)
        {
            inference::charge_type_snapshot(
                payload,
                functions[0].span,
                limits,
                left,
                definitions.len().saturating_add(4),
            )?;
            if !definitions.iter().any(|d| d.payload == *payload) {
                let name = match payload {
                    Type::TaskI32 => TASK_I32_RESOURCE_NAME.into(),
                    Type::Task { .. } => {
                        format!("{TASK_RESOURCE_NAME_PREFIX}{}", payload.fingerprint())
                    }
                    _ => unreachable!(),
                };
                definitions.push(ResourceDefinition {
                    name,
                    payload: payload.clone(),
                    teardown: None,
                });
            }
        }
        let count = match ty {
            Type::Tuple(fields) => fields.len(),
            Type::Record { fields, .. } | Type::Package { fields, .. } => fields.len(),
            Type::Enum { variants, .. } => {
                *left = left.checked_sub(variants.len()).ok_or_else(|| {
                    Diagnostic::limit(Phase::Types, "task variant scan budget exceeded", None)
                })?;
                variants
                    .iter()
                    .fold(0usize, |n, (_, fields)| n.saturating_add(fields.len()))
            }
            Type::ErrorUnion { .. } => 2,
            Type::Task { .. }
            | Type::Address { .. }
            | Type::Managed(_)
            | Type::Desc(_)
            | Type::Array { .. }
            | Type::Slice { .. }
            | Type::OwnedStore { .. }
            | Type::Registration { .. }
            | Type::Opaque { .. } => 1,
            Type::FunctionPointer { parameters, .. }
            | Type::BorrowedFunctionPointer { parameters, .. }
            | Type::ForeignCallback { parameters, .. } => parameters.len().saturating_add(1),
            _ => 0,
        };
        *left = left.checked_sub(count).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "task resource scan budget exceeded", None)
        })?;
        let mut children = Vec::with_capacity(count);
        match ty {
            Type::Tuple(fields) => children.extend(fields),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                children.extend(fields.iter().map(|(_, t)| t))
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    children.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                children.extend([error.as_ref(), result.as_ref()])
            }
            Type::Task { result: inner, .. }
            | Type::Address { inner, .. }
            | Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::OwnedStore { element: inner }
            | Type::Registration { environment: inner }
            | Type::Opaque { hidden: inner, .. } => children.push(inner.as_ref()),
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                children.extend(parameters);
                children.push(result);
            }
            _ => {}
        }
        pending.extend(children);
    }
    Ok(())
}
impl Builder<'_, '_> {
    pub(super) fn task_call(
        &mut self,
        name: &str,
        arguments: &[CoreExpr],
        expected: Option<&Type>,
        span: Span,
    ) -> R<(ValueId, Type)> {
        if matches!(name, "task_create" | "task_await") {
            let Some(worker_expr) = arguments.first() else {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "task_create requires a static worker",
                ));
            };
            let before = self.closures.remaining_work();
            let mut remaining = before;
            let prepared = (|| -> R<(FunctionId, Vec<Type>, Type)> {
                let signature = self.resolve_callee(worker_expr)?;
                if signature.foreign
                    || signature.cleanup
                    || !signature.callable.is_empty()
                    || signature.template.is_some()
                    || signature.type_parameter.is_some()
                    || signature.receiver.is_some()
                {
                    return Err(error(
                        "E-TASK-WORKER",
                        worker_expr.span,
                        "task_create requires an ordinary concrete static worker",
                    ));
                }
                for parameter in &signature.parameters {
                    inference::charge_type_snapshot(
                        parameter,
                        span,
                        self.limits,
                        &mut remaining,
                        1,
                    )?;
                }
                inference::charge_type_snapshot(
                    &signature.result,
                    span,
                    self.limits,
                    &mut remaining,
                    1,
                )?;
                Ok((
                    signature.concrete_id(worker_expr.span)?,
                    signature.parameters.clone(),
                    signature.result.clone(),
                ))
            })();
            self.closures.snapshot(before - remaining, span)?;
            let (worker, parameters, worker_result) = prepared?;
            if arguments.len() != parameters.len() + 1 {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "task_create input count differs from actual worker",
                ));
            }
            let mut prepared_inputs = Vec::new();
            for (argument, wanted) in arguments[1..].iter().zip(&parameters) {
                let (v, t) = self.argument_expr(argument, wanted)?;
                self.same(&t, wanted, argument.span)?;
                prepared_inputs.push(self.stage_argument(v, &t, argument.span)?);
            }
            let inputs = self.commit_arguments(prepared_inputs, span)?;
            self.closures.snapshot_type(&worker_result, 3, 0, span)?;
            let ty = if name == "task_await" {
                worker_result
            } else {
                create_type(&worker_result)
            };
            if let Some(wanted) = expected {
                self.same(&ty, wanted, span)?;
            }
            let v = self.value(
                if name == "task_await" {
                    InstructionKind::SuspendCall {
                        function: worker,
                        arguments: inputs,
                    }
                } else {
                    InstructionKind::TaskCreate {
                        worker,
                        arguments: inputs,
                    }
                },
                ty.clone(),
                span,
            )?;
            return Ok((v, ty));
        }
        if !matches!(name, "task_start" | "task_await" | "task_yield") && arguments.len() == 1 {
            let before = self.closures.remaining_work();
            let mut remaining = before;
            let looked = (|| -> R<Option<Type>> {
                let Some(actual) = self.borrowed_task_hint(&arguments[0], &mut remaining, 0)?
                else {
                    return Ok(None);
                };
                let Some(task @ Type::Task { .. }) = payload(actual) else {
                    return Ok(None);
                };
                inference::charge_type_snapshot(task, span, self.limits, &mut remaining, 6)?;
                Ok(Some(task.clone()))
            })();
            self.closures.snapshot(before - remaining, span)?;
            if let Some(task) = looked? {
                let parameter = parameter_for(name, &task);
                let ty = result_for(name, &task);
                let (task, t) = self.argument_expr(&arguments[0], &parameter)?;
                self.same(&t, &parameter, arguments[0].span)?;
                let kind = match name {
                    "task_valid" => InstructionKind::TaskValid { task },
                    "task_poll" => InstructionKind::TaskPoll { task },
                    "task_request_cancel" => InstructionKind::TaskRequestCancel { task },
                    "task_poll_close" => InstructionKind::TaskPollClose { task },
                    "task_finish_destroy" => InstructionKind::TaskFinishDestroy { task },
                    _ => unreachable!(),
                };
                if let Some(wanted) = expected {
                    self.same(&ty, wanted, span)?;
                }
                let v = self.value(kind, ty.clone(), span)?;
                return Ok((v, ty));
            }
        }
        let parameters = parameters(name);
        let offset = usize::from(matches!(name, "task_start" | "task_await"));
        if arguments.len() != parameters.len() + offset {
            return Err(error(
                "E-CALL-ARITY",
                span,
                "task primitive argument count mismatch",
            ));
        }
        let kind = if matches!(name, "task_start" | "task_await") {
            let worker_expr = &arguments[0];
            if !matches!(
                worker_expr.kind,
                CoreExprKind::Name(_) | CoreExprKind::Member { .. }
            ) {
                return Err(error(
                    "E-TASK-WORKER",
                    worker_expr.span,
                    "task worker must be a static concrete function",
                ));
            }
            let signature = self.resolve_callee(worker_expr)?.clone();
            if !signature.scalar_thread_worker() || signature.parameters.as_slice() != [Type::I32] {
                return Err(error(
                    "E-TASK-WORKER",
                    worker_expr.span,
                    "task worker requires exact ordinary fn(i32)->i32",
                ));
            }
            let worker = signature.concrete_id(worker_expr.span)?;
            let (argument, ty) = self.expr(&arguments[1], Some(&Type::I32))?;
            self.same(&ty, &Type::I32, arguments[1].span)?;
            if name == "task_start" {
                InstructionKind::TaskStart { worker, argument }
            } else {
                InstructionKind::SuspendCall {
                    function: worker,
                    arguments: vec![argument],
                }
            }
        } else if name == "task_yield" {
            InstructionKind::TaskYield
        } else {
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
        if let Some(expected) = expected {
            self.same(&ty, expected, span)?;
        }
        Ok((self.value(kind, ty.clone(), span)?, ty))
    }
}

#[cfg(test)]
mod terminal343_snapshot_budget {
    use super::*;
    fn function(ty: Type) -> TypedFunction {
        TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            id: FunctionId(0),
            name: "metadata".into(),
            foreign: None,
            environment: None,
            parameters: vec![],
            return_type: Type::Unit,
            result_mode: PassingMode::Infer,
            locals: vec![],
            values: vec![ty],
            blocks: vec![],
            entry: BlockId(0),
            span: Span::default(),
        }
    }
    #[test]
    fn remaining_work_stops_task_definition_and_result_before_deep_tail() {
        let limits = Limits {
            max_call_depth: 8,
            ..Limits::default()
        };
        let mut ty = Type::I32;
        for _ in 0..32 {
            ty = Type::Record {
                name: "R".into(),
                fields: vec![("x".into(), ty)],
            };
        }
        let task = Type::Task {
            result: Box::new(ty),
            retained_life: empty_context(),
        };
        assert!(
            inference::type_snapshot_work(&task, Span::default(), &limits)
                .unwrap_err()
                .message
                .contains("depth")
        );
        for copies in [4, 6] {
            let mut state = closures::State::new(0, &limits);
            state
                .snapshot(limits.max_analysis_steps - 8, Span::default())
                .unwrap();
            let error = state
                .snapshot_type(&task, copies, 0, Span::default())
                .unwrap_err();
            assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
            assert!(error.message.contains("work"), "{error:?}");
            assert!(
                state.remaining_work() < 8,
                "failed planner refunded consumed visits"
            );
        }
        let functions = [function(Type::Managed(Box::new(task)))];
        let mut left = 10;
        let error = add_definition(&functions, &mut vec![], &mut left, &limits).unwrap_err();
        assert_eq!(error.category, zil_base::ResultCategory::ResourceLimit);
        assert!(error.message.contains("work"), "{error:?}");
        assert_eq!(left, 0, "failed definition planning refunded visits");
    }
    #[test]
    fn task_definition_and_result_exact_remaining_boundary() {
        let limits = Limits::default();
        let task = Type::Task {
            result: Box::new(Type::I32),
            retained_life: empty_context(),
        };
        for copies in [4, 6] {
            let n =
                inference::type_snapshot_work(&task, Span::default(), &limits).unwrap() * copies;
            for (amount, success) in [(n, true), (n - 1, false), (n, true)] {
                let mut state = closures::State::new(0, &limits);
                state
                    .snapshot(limits.max_analysis_steps - amount, Span::default())
                    .unwrap();
                let result = state.snapshot_type(&task, copies, 0, Span::default());
                assert_eq!(result.is_ok(), success, "{result:?}");
                if !success {
                    assert_eq!(
                        result.unwrap_err().category,
                        zil_base::ResultCategory::ResourceLimit
                    );
                }
            }
        }
        let functions = [function(Type::Managed(Box::new(task)))];
        let minimum = (1..128)
            .find(|&n| {
                let mut left = n;
                add_definition(&functions, &mut vec![], &mut left, &limits).is_ok()
            })
            .unwrap();
        for (mut amount, success) in [(minimum, true), (minimum - 1, false), (minimum, true)] {
            let result = add_definition(&functions, &mut vec![], &mut amount, &limits);
            assert_eq!(result.is_ok(), success, "{result:?}");
            if !success {
                assert_eq!(
                    result.unwrap_err().category,
                    zil_base::ResultCategory::ResourceLimit
                );
            }
        }
        eprintln!("task343 exact definition ledger N={minimum}");
    }
}
