//! Source preparation for the sealed scalar cooperative task ABI.
use super::*;

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
    Type::Enum { name: "@std.task:v1:TaskCreateError".into(), variants: vec![("Allocation".into(), vec![])] }
}
pub(super) fn poll_state() -> Type {
    Type::Enum { name: "@std.task:v1:TaskPollState".into(), variants: ["Complete", "Cancelled", "Invalid"].into_iter().map(|s|(s.into(),vec![])).collect() }
}
// Callers prepay a complete actual-T walk before cloning/hash generation.
pub(super) fn poll_type(result: &Type) -> Type {
    Type::Enum { name: format!("@std.task:v1:Poll:{}", result.fingerprint()), variants: vec![("Pending".into(),vec![]),("Ready".into(),vec![result.clone()])] }
}
pub(super) fn create_type(result: &Type) -> Type {
    Type::ErrorUnion { error: Box::new(create_error()), result: Box::new(Type::Managed(Box::new(Type::Task { result: Box::new(result.clone()) }))) }
}
pub(super) fn result_for(name: &str, task: &Type) -> Type {
    let Type::Task { result: t } = task else { return result(name); };
    match name {
        "task_poll" => Type::ErrorUnion { error: Box::new(poll_state()), result: Box::new(poll_type(t)) },
        "task_poll_close" => poll_type(&Type::Unit),
        _ => result(name),
    }
}
pub(super) fn parameter_for(name: &str, task: &Type) -> Type {
    match name {
        "task_valid" => Type::Desc(Box::new(task.clone())),
        "task_finish_destroy" => Type::Managed(Box::new(task.clone())),
        _ => Type::Address { mutable: true, inner: Box::new(Type::Managed(Box::new(task.clone()))) },
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
            let work = inference::type_snapshot_work(payload, functions[0].span, limits)?;
            *left = left.checked_sub(work.saturating_mul(definitions.len().saturating_add(4))).ok_or_else(||Diagnostic::limit(Phase::Types,"task definition identity budget exceeded",None))?;
            if !definitions.iter().any(|d|d.payload == *payload) {
                let name = match payload { Type::TaskI32 => TASK_I32_RESOURCE_NAME.into(), Type::Task { result } => format!("{TASK_RESOURCE_NAME_PREFIX}{}",result.fingerprint()), _=>unreachable!() };
                definitions.push(ResourceDefinition { name, payload: payload.clone(), teardown: None });
            }
        }
        let count=match ty {
            Type::Tuple(fields)=>fields.len(),
            Type::Record{fields,..}|Type::Package{fields,..}=>fields.len(),
            Type::Enum{variants,..}=>{*left=left.checked_sub(variants.len()).ok_or_else(||Diagnostic::limit(Phase::Types,"task variant scan budget exceeded",None))?; variants.iter().fold(0usize,|n,(_,fields)|n.saturating_add(fields.len()))},
            Type::ErrorUnion{..}=>2,
            Type::Task{..}|Type::Address{..}|Type::Managed(_)|Type::Desc(_)|Type::Array{..}|Type::Slice{..}|Type::OwnedStore{..}|Type::Registration{..}|Type::Opaque{..}=>1,
            Type::FunctionPointer{parameters,..}|Type::BorrowedFunctionPointer{parameters,..}|Type::ForeignCallback{parameters,..}=>parameters.len().saturating_add(1),
            _=>0,
        };
        *left=left.checked_sub(count).ok_or_else(||Diagnostic::limit(Phase::Types,"task resource scan budget exceeded",None))?;
        let mut children=Vec::with_capacity(count);
        match ty {
            Type::Tuple(fields)=>children.extend(fields),
            Type::Record{fields,..}|Type::Package{fields,..}=>children.extend(fields.iter().map(|(_,t)|t)),
            Type::Enum{variants,..}=>{for (_,fields) in variants{children.extend(fields);}},
            Type::ErrorUnion{error,result}=>children.extend([error.as_ref(),result.as_ref()]),
            Type::Task{result:inner}|Type::Address{inner,..}|Type::Managed(inner)|Type::Desc(inner)|Type::Array{element:inner,..}|Type::Slice{element:inner}|Type::OwnedStore{element:inner}|Type::Registration{environment:inner}|Type::Opaque{hidden:inner,..}=>children.push(inner.as_ref()),
            Type::FunctionPointer{parameters,result}|Type::BorrowedFunctionPointer{parameters,result,..}|Type::ForeignCallback{parameters,result}=>{children.extend(parameters);children.push(result);},
            _=>{}
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
        if name == "task_create" {
            let Some(worker_expr) = arguments.first() else { return Err(error("E-CALL-ARITY",span,"task_create requires a static worker")); };
            let signature = self.resolve_callee(worker_expr)?.clone();
            if signature.foreign || signature.cleanup || !signature.callable.is_empty() || signature.template.is_some() || signature.type_parameter.is_some() || signature.receiver.is_some() {
                return Err(error("E-TASK-WORKER",worker_expr.span,"task_create requires an ordinary concrete static worker"));
            }
            if arguments.len() != signature.parameters.len()+1 { return Err(error("E-CALL-ARITY",span,"task_create input count differs from actual worker")); }
            let mut inputs=Vec::new();
            for (argument,wanted) in arguments[1..].iter().zip(&signature.parameters) {
                let (v,t)=self.argument_expr(argument,wanted)?; self.same(&t,wanted,argument.span)?; inputs.push(v);
            }
            let work=inference::type_snapshot_work(&signature.result,span,self.limits)?;
            self.closures.snapshot(work.saturating_mul(4),span)?;
            let ty=create_type(&signature.result);
            if let Some(wanted)=expected { self.same(&ty,wanted,span)?; }
            let v=self.value(InstructionKind::TaskCreate {worker:signature.concrete_id(worker_expr.span)?, arguments:inputs},ty.clone(),span)?;
            return Ok((v,ty));
        }
        if !matches!(name,"task_start"|"task_await"|"task_yield") && arguments.len()==1 {
            let mut expression=&arguments[0];
            while let CoreExprKind::Unary{operand,..}=&expression.kind { expression=operand; }
            if let Some(actual)=self.known_type(expression)
                && let Some(task @ Type::Task { .. })=payload(&actual)
            {
                let work=inference::type_snapshot_work(task,span,self.limits)?;
                self.closures.snapshot(work.saturating_mul(6),span)?;
                let parameter=parameter_for(name,task);
                let ty=result_for(name,task);
                let (task,t)=self.argument_expr(&arguments[0],&parameter)?; self.same(&t,&parameter,arguments[0].span)?;
                let kind=match name {"task_valid"=>InstructionKind::TaskValid{task},"task_poll"=>InstructionKind::TaskPoll{task},"task_request_cancel"=>InstructionKind::TaskRequestCancel{task},"task_poll_close"=>InstructionKind::TaskPollClose{task},"task_finish_destroy"=>InstructionKind::TaskFinishDestroy{task},_=>unreachable!()};
                if let Some(wanted)=expected { self.same(&ty,wanted,span)?; }
                let v=self.value(kind,ty.clone(),span)?; return Ok((v,ty));
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
