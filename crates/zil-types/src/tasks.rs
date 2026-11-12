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
pub(super) fn borrowed_context_with_life(
    parameters: &[Type],
    life: Option<&zil_ir::FunctionLifeContract>,
    result: &Type,
) -> zil_ir::RetainedLifeShape {
    let mut ports = Vec::new();
    let mut classes = std::collections::BTreeMap::new();
    let mut next_binder = 0u32;
    for (position, parameter) in parameters.iter().enumerate() {
        if let Type::Address { mutable, inner } = parameter
            && retained_pointee(inner)
        {
            let source =
                life.and_then(|life| life.parameter_lives.get(position).copied().flatten());
            let binder = if let Some(source) = source {
                if let Some(existing) = classes.get(&source) {
                    *existing
                } else {
                    let binder = next_binder;
                    classes.insert(source, binder);
                    next_binder += 1;
                    binder
                }
            } else {
                let binder = next_binder;
                next_binder += 1;
                binder
            };
            ports.push(zil_ir::RetainedBorrowPort {
                leaf: parameter.clone(),
                mode: if *mutable {
                    zil_ir::StoredBorrowMode::ExclusiveAddress
                } else {
                    zil_ir::StoredBorrowMode::SharedAddress
                },
                binder,
            });
        }
    }
    let mut retained = Vec::new();
    if matches!(result, Type::Address { mutable: false, .. })
        && let Some(binder) = life
            .and_then(|l| l.result_life)
            .and_then(|source| classes.get(&source))
    {
        retained.push(zil_ir::StorageRetainedBinding {
            path: vec![zil_ir::LifeTypeProjection::Result],
            class: 0,
            binder: *binder,
        });
    }
    zil_ir::RetainedLifeShape {
        binder_count: next_binder,
        ports,
        retained,
        nominal_arguments: vec![],
    }
}
pub(super) fn retained_pointee(ty: &Type) -> bool {
    retained_pointee_with_named(ty, false)
}
fn retained_pointee_with_named(ty: &Type, allow_named: bool) -> bool {
    match ty {
        Type::Unit
        | Type::Bool
        | Type::I32
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::U8
        | Type::U16
        | Type::F32
        | Type::F64 => true,
        Type::Named(_) if allow_named => true,
        Type::Record { fields, .. } => fields
            .iter()
            .all(|(_, ty)| retained_pointee_with_named(ty, allow_named)),
        Type::Tuple(fields) => fields
            .iter()
            .all(|ty| retained_pointee_with_named(ty, allow_named)),
        Type::Array { element, .. } | Type::Slice { element } => {
            retained_pointee_with_named(element, allow_named)
        }
        _ => false,
    }
}
fn retained_result(ty: &Type) -> bool {
    retained_result_with_named(ty, false)
}
fn retained_result_with_named(ty: &Type, allow_named: bool) -> bool {
    match ty {
        Type::Unit
        | Type::Bool
        | Type::U8
        | Type::U16
        | Type::I32
        | Type::I64
        | Type::U64
        | Type::Usize
        | Type::F32
        | Type::F64 => true,
        Type::Named(_) if allow_named => true,
        Type::Address {
            mutable: false,
            inner,
        } => retained_pointee_with_named(inner, allow_named),
        Type::Managed(payload) => retained_result_with_named(payload, allow_named),
        Type::Record { fields, .. } => fields
            .iter()
            .all(|(_, ty)| retained_result_with_named(ty, allow_named)),
        Type::Tuple(fields) => fields
            .iter()
            .all(|ty| retained_result_with_named(ty, allow_named)),
        Type::Enum { variants, .. } => variants.iter().all(|(_, fields)| {
            fields
                .iter()
                .all(|ty| retained_result_with_named(ty, allow_named))
        }),
        Type::ErrorUnion { error, result } => {
            retained_result_with_named(error, allow_named)
                && retained_result_with_named(result, allow_named)
        }
        _ => false,
    }
}
fn retained_bindings(parameters: &[Type]) -> Vec<zil_ir::RetainedPortBinding> {
    parameters
        .iter()
        .enumerate()
        .filter_map(|(parameter, ty)| {
            let Type::Address { inner, .. } = ty else {
                return None;
            };
            retained_pointee(inner).then_some(parameter as u32)
        })
        .enumerate()
        .map(|(port, parameter)| zil_ir::RetainedPortBinding {
            port: port as u32,
            source: zil_ir::StoragePosition {
                base: zil_ir::StorageBase::Parameter(parameter),
                path: vec![],
            },
        })
        .collect()
}
pub(super) fn require_closed_context(ty: &Type, span: Span) -> R<()> {
    check_context(ty, span, false)
}
pub(super) fn pre_resolution_context(ty: &Type, span: Span) -> R<()> {
    // A symbolic name is a placeholder during Core inference only. Complete
    // aggregate resolution must repeat the strict check on each resolved port.
    check_context(ty, span, true)
}
fn check_context(ty: &Type, span: Span, allow_named: bool) -> R<()> {
    let mut next = 0;
    if let Type::Task {
        result,
        retained_life,
    } = ty
    {
        let shared = matches!(result.as_ref(), Type::Address { mutable: false, .. });
        if shared || !retained_life.retained.is_empty() {
            let valid = shared
                && retained_result_with_named(result, allow_named)
                && retained_life.binder_count > 0
                && retained_life.nominal_arguments.is_empty()
                && matches!(retained_life.retained.as_slice(), [r]
                    if r.path == [zil_ir::LifeTypeProjection::Result] && r.class == 0
                    && r.binder < retained_life.binder_count
                    && retained_life.ports.iter().any(|p| p.binder == r.binder && p.leaf == **result)
                    && retained_life.ports.iter().filter(|p| p.binder == r.binder).all(|p|
                        p.mode == zil_ir::StoredBorrowMode::SharedAddress));
            if !valid {
                return Err(error(
                    "E-TASK-RESULT-CONTEXT",
                    span,
                    "Task shared result requires its actual shared input class and complete Result mapping",
                ));
            }
        }
    }
    if let Type::Task {
        result,
        retained_life,
    } = ty
        && *retained_life != empty_context()
        && (!retained_life.nominal_arguments.is_empty()
            || retained_life.ports.iter().any(|port| {
                let dense = if port.binder == next { next += 1; true } else { port.binder < next };
                !dense
                    || !matches!(&port.leaf, Type::Address { mutable, inner }
                        if retained_pointee_with_named(inner, allow_named)
                            && port.mode == if *mutable { zil_ir::StoredBorrowMode::ExclusiveAddress } else { zil_ir::StoredBorrowMode::SharedAddress })
            })
            || next != retained_life.binder_count
            || !retained_result_with_named(result, allow_named))
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
pub(super) fn poll_name(result: &Type) -> String {
    format!("@std.task:v1:Poll:{}", result.fingerprint())
}
pub(super) fn poll_type(result: &Type) -> Type {
    let storage = Type::Enum {
        name: poll_name(result),
        variants: vec![
            ("Pending".into(), vec![]),
            ("Ready".into(), vec![result.clone()]),
        ],
    };
    if matches!(result, Type::Address { mutable: false, .. }) {
        Type::BorrowedStorage {
            storage: Box::new(storage),
            life: zil_ir::StorageLifeShape {
                binder_count: 1,
                slots: vec![zil_ir::StorageLifeSlot {
                    path: vec![zil_ir::PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: 1,
                            index: 0,
                        },
                    )],
                    binder: 0,
                    mode: zil_ir::StoredBorrowMode::SharedAddress,
                }],
                retained: vec![zil_ir::StorageRetainedBinding {
                    path: vec![zil_ir::LifeTypeProjection::TypeArgument(0)],
                    class: 0,
                    binder: 0,
                }],
                nominal_arguments: vec![],
            },
        }
    } else {
        storage
    }
}
pub(super) fn create_type(result: &Type, context: zil_ir::RetainedLifeShape) -> Type {
    Type::ErrorUnion {
        error: Box::new(create_error()),
        result: Box::new(Type::Managed(Box::new(Type::Task {
            result: Box::new(result.clone()),
            retained_life: context,
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
) -> R<bool> {
    let mut has_mutex = false;
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
        // Reuse the paid complete-type inventory; this flag grants no permission.
        has_mutex |= matches!(ty, Type::Mutex { .. } | Type::MutexGuard { .. });
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
    Ok(has_mutex)
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
            let prepared = (|| -> R<(FunctionId, Vec<Type>, Type, Option<zil_ir::FunctionLifeContract>)> {
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
                    signature.life.clone().or_else(|| {
                        matches!(signature.result, Type::Address {mutable:false, ..})
                            .then(|| signature.storage_life.as_ref().and_then(|life| life.to_flat(signature.parameters.len())))
                            .flatten()
                    }),
                ))
            })();
            self.closures.snapshot(before - remaining, span)?;
            let (worker, parameters, worker_result, worker_life) = prepared?;
            if name == "task_create"
                && !borrowed_context_with_life(&parameters, worker_life.as_ref(), &worker_result)
                    .ports
                    .is_empty()
                && !retained_result(&worker_result)
            {
                return Err(unsupported(
                    span,
                    "retained Task result requires an actual supported complete result",
                ));
            }
            if arguments.len() != parameters.len() + 1 {
                return Err(error(
                    "E-CALL-ARITY",
                    span,
                    "task_create input count differs from actual worker",
                ));
            }
            let mut prepared_inputs = Vec::new();
            for (argument, wanted) in arguments[1..].iter().zip(&parameters) {
                if name == "task_create" && registration::asynchronous(wanted) {
                    return Err(error(
                        "E-ASYNC-REGISTRATION-TASK-INPUT",
                        argument.span,
                        "a live pollable TaskCreate input has no synchronous allocation-failure cleanup; create it inside the Suspend worker",
                    ));
                }
                let (v, t) = self.argument_expr(argument, wanted)?;
                self.same(&t, wanted, argument.span)?;
                prepared_inputs.push(self.stage_argument(v, &t, argument.span)?);
            }
            let inputs = self.commit_arguments(prepared_inputs, span)?;
            self.closures.snapshot_type(&worker_result, 3, 0, span)?;
            let ty = if name == "task_await" {
                worker_result
            } else {
                create_type(
                    &worker_result,
                    borrowed_context_with_life(&parameters, worker_life.as_ref(), &worker_result),
                )
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
                        port_bindings: retained_bindings(&parameters),
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
mod snapshot_budget {
    use super::*;
    fn function(ty: Type) -> TypedFunction {
        TypedFunction {
            storage_life: None,
            call_kind: CallKind::Direct,
            resume_bound: zil_ir::TaskResumeBound::Local,
            reference_return_bound: None,
            callable_return_contract: None,
            callable_contract: None,
            protocol_contract: None,
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
        eprintln!("exact task definition ledger N={minimum}");
    }
}
