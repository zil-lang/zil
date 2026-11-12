//! Kernel reconstruction of the stable owning retained callback boundary.
use super::*;
mod phase;
mod threaded;
pub(super) fn check_phase(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    if budget.protocol_contracts || budget.protocol_checked {
        phase::check(module, budget)
    } else {
        Ok(())
    }
}
pub(super) const PREFIX: &str = "@builtin:Registration:";
pub(super) const ASYNC_PREFIX: &str = "@builtin:AsyncRegistration:";
fn error(span: Option<Span>, s: &str) -> Diagnostic {
    reject("E-VERIFY-REGISTRATION", span, s)
}
pub(super) fn carrier(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    budget.registrations = true;
    let (Type::Managed(p) | Type::Desc(p)) = ty else {
        return Err(error(span, "registration requires an owner or descriptor"));
    };
    let (Type::Registration { environment } | Type::AsyncRegistration { environment }) = p.as_ref()
    else {
        return Err(error(span, "sealed registration payload is absent"));
    };
    let Type::Closure(id) = environment.as_ref() else {
        return Err(error(
            span,
            "registration environment must be an actual concrete closure",
        ));
    };
    budget.spend(module.resources.len().saturating_add(32))?;
    let prefix = if matches!(p.as_ref(), Type::AsyncRegistration { .. }) {
        ASYNC_PREFIX
    } else {
        PREFIX
    };
    let name = format!("{prefix}{}", id.0);
    if !module.target.is_supported_x64_llvm()
        || (module.resources.iter().filter(|r| r.payload == **p).count() != 1
            || !module
                .resources
                .iter()
                .any(|r| r.name == name && r.payload == **p && r.teardown.is_none()))
    {
        return Err(error(
            span,
            "registration lacks its exact canonical owning definition",
        ));
    }
    closed(
        module,
        *id,
        span,
        budget,
        matches!(p.as_ref(), Type::AsyncRegistration { .. }),
    )
}
fn closed(
    module: &TypedModule,
    root: ClosureId,
    span: Option<Span>,
    budget: &mut Budget,
    asynchronous: bool,
) -> Check<()> {
    let mut closure_queue = vec![root];
    let mut closures_seen = BTreeSet::new();
    let mut code_queue = Vec::new();
    while let Some(id) = closure_queue.pop() {
        budget.spend(closures_seen.len().saturating_add(1))?;
        if !closures_seen.insert(id) {
            continue;
        }
        let c = closures::definition(module, id, span)?;
        budget.spend(
            c.captures
                .len()
                .saturating_add(c.lending_captures.len())
                .saturating_add(1),
        )?;
        if !c.lending_captures.is_empty() {
            return Err(error(
                span,
                "ClosedEnvironment cannot hold borrowed capture positions",
            ));
        }
        resources::charge_payload(&c.result, budget)?;
        for parameter in &c.parameters {
            resources::charge_payload(parameter, budget)?;
        }
        let entry = module
            .functions
            .get(c.function.0 as usize)
            .ok_or_else(|| error(span, "registration entry is absent"))?;
        budget.spend(c.parameters.len().saturating_add(1))?;
        if entry.environment != Some(id)
            || entry.foreign.is_some()
            || entry.parameters.len() != c.parameters.len()
            || entry.return_type != c.result
            || entry.result_mode != PassingMode::Infer
        {
            return Err(error(
                span,
                "registration closure differs from its actual entry",
            ));
        }
        for (local, t) in entry.parameters.iter().zip(&c.parameters) {
            if local_type(entry, *local, span)? != t {
                return Err(error(span, "registration actual entry parameter differs"));
            }
        }
        code_queue.push(c.function);
        let mut fields = c.captures.iter().collect::<Vec<_>>();
        while let Some(t) = fields.pop() {
            budget.spend(1)?;
            match t {
                Type::AsyncRegistration { .. } => {
                    return Err(unsupported(
                        "ClosedEnvironment cannot retain a pollable registration with asynchronous teardown",
                        span,
                    ));
                }
                Type::Address { .. }
                | Type::Dyn { .. }
                | Type::Desc(_)
                | Type::Slice { .. }
                | Type::Named(_)
                | Type::Unsupported(_)
                | Type::SyncCell(_) => {
                    return Err(error(
                        span,
                        "ClosedEnvironment contains an external loan or unresolved position",
                    ));
                }
                Type::TaskI32 if asynchronous => {
                    return Err(error(
                        span,
                        "AsyncRegistration environment lacks an actual synchronous legacy Task frame cleanup proof",
                    ));
                }
                Type::Task { .. } => {
                    return Err(error(
                        span,
                        "a task result type does not prove its hidden frame ClosedEnvironment",
                    ));
                }
                Type::Closure(id) => {
                    budget.spend(1)?;
                    closure_queue.push(*id)
                }
                Type::Managed(p) => {
                    budget.spend(module.resources.len())?;
                    for resource in &module.resources {
                        resources::charge_payload(&resource.payload, budget)?;
                        if resource.payload == **p
                            && let Some(teardown) = resource.teardown
                        {
                            code_queue.push(teardown);
                        }
                    }
                    budget.spend(1)?;
                    fields.push(p);
                }
                Type::BorrowedStorage { storage: p, .. }
                | Type::Opaque { hidden: p, .. }
                | Type::OwnedStore { element: p }
                | Type::Registration { environment: p }
                | Type::Array { element: p, .. } => {
                    budget.spend(1)?;
                    fields.push(p)
                }
                Type::Tuple(ts) => {
                    budget.spend(ts.len())?;
                    fields.extend(ts)
                }
                Type::Package { fields: fs, .. } | Type::Record { fields: fs, .. } => {
                    budget.spend(fs.len())?;
                    fields.extend(fs.iter().map(|(_, t)| t))
                }
                Type::Enum { variants, .. } => {
                    budget.spend(variants.len())?;
                    for (_, ts) in variants {
                        budget.spend(ts.len())?;
                        fields.extend(ts)
                    }
                }
                Type::ErrorUnion { error, result } => {
                    budget.spend(2)?;
                    fields.extend([error.as_ref(), result.as_ref()])
                }
                Type::BorrowedFunctionPointer { .. }
                | Type::FunctionPointer { .. }
                | Type::ForeignCallback { .. }
                | Type::Unit
                | Type::Bool
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::I32
                | Type::I8
                | Type::I16
                | Type::Isize
                | Type::I64
                | Type::U64
                | Type::Usize
                | Type::F32
                | Type::F64
                | Type::ForeignHandle
                | Type::CWideString
                | Type::ByteBuffer
                | Type::TaskI32 => {}
                Type::Mutex { .. } | Type::MutexGuard { .. } | Type::OwnedDyn { .. } => {
                    return Err(unsupported(
                        "mutex capture needs a complete thread and cleanup contract",
                        span,
                    ));
                }
            }
        }
    }
    let mut code_seen = BTreeSet::new();
    while let Some(id) = code_queue.pop() {
        budget.spend(code_seen.len().saturating_add(1))?;
        if !code_seen.insert(id) {
            continue;
        }
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| error(span, "registration Direct dependency is absent"))?;
        if f.call_kind != CallKind::Direct {
            return Err(error(span, "retained callback target selects Suspend ABI"));
        }
        for b in &f.blocks {
            budget.spend(1)?;
            for i in &b.instructions {
                budget.spend(1)?;
                match i.kind {
                    InstructionKind::TaskYield | InstructionKind::SuspendCall { .. } => {
                        return Err(error(
                            Some(i.span),
                            "registration callback may only execute through Direct ABI",
                        ));
                    }
                    InstructionKind::Call { function, .. }
                    | InstructionKind::FunctionRef { function }
                    | InstructionKind::ForeignFunctionRef { function } => {
                        budget.spend(1)?;
                        code_queue.push(function)
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
pub(super) fn instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(i.span);
    budget.registrations = true;
    budget.head_descriptions = true;
    budget.protocol_checked |= matches!(
        i.kind,
        InstructionKind::AsyncRegistrationFinishDestroyChecked { .. }
    );
    budget.spend(1)?;
    Ok(Some(match i.kind {
        InstructionKind::AsyncRegistrationStart {
            register,
            request_close,
            poll_drain,
            callback,
            ..
        } => {
            let is_threaded = module
                .functions
                .get(register.0 as usize)
                .and_then(|f| f.foreign.as_ref())
                .and_then(|f| f.scoped_callback)
                == Some(ScopedCallbackRole::ThreadAsyncRegister);
            for (id, role) in if is_threaded {
                [
                    (register, ScopedCallbackRole::ThreadAsyncRegister),
                    (request_close, ScopedCallbackRole::ThreadAsyncRequestClose),
                    (poll_drain, ScopedCallbackRole::ThreadAsyncPollDrain),
                ]
            } else {
                [
                    (register, ScopedCallbackRole::AsyncRegister),
                    (request_close, ScopedCallbackRole::AsyncRequestClose),
                    (poll_drain, ScopedCallbackRole::AsyncPollDrain),
                ]
            } {
                let provider = module
                    .functions
                    .get(id.0 as usize)
                    .ok_or_else(|| error(span, "async provider absent"))?;
                if provider.foreign.as_ref().and_then(|f| f.scoped_callback) != Some(role) {
                    return Err(error(span, "async provider has wrong actual role"));
                }
                scoped_callback::declaration(provider, budget)?;
            }
            let Type::Closure(id) = value_type(f, callback, span)? else {
                return Err(error(
                    span,
                    "async callback requires actual consumed concrete F",
                ));
            };
            let closure = closures::definition(module, *id, span)?;
            budget.spend(closure.parameters.len().saturating_add(1))?;
            if !matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
                || closure.parameters != [Type::I32]
                || closure.result != Type::I32
            {
                return Err(error(
                    span,
                    "async callback requires actual FnMut(i32)->i32",
                ));
            }
            if is_threaded {
                threaded::actual(module, *id, span, budget)?;
            }
            let ty = Type::Managed(Box::new(Type::AsyncRegistration {
                environment: Box::new(Type::Closure(*id)),
            }));
            carrier(module, &ty, span, budget)?;
            ty
        }
        InstructionKind::AsyncRegistrationValid { registration } => {
            let ty = value_type(f, registration, span)?;
            if !matches!(ty,Type::Desc(p) if matches!(p.as_ref(),Type::AsyncRegistration{..})) {
                return Err(error(span, "async validity requires actual descriptor"));
            }
            carrier(module, ty, span, budget)?;
            Type::Bool
        }
        InstructionKind::AsyncRegistrationRequestClose { registration }
        | InstructionKind::AsyncRegistrationPollClose { registration } => {
            let Type::Address {
                mutable: true,
                inner,
            } = value_type(f, registration, span)?
            else {
                return Err(error(
                    span,
                    "async close requires exclusive actual head address",
                ));
            };
            if !matches!(inner.as_ref(),Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
            {
                return Err(error(
                    span,
                    "async close requires complete pollable owning head",
                ));
            }
            carrier(module, inner, span, budget)?;
            if matches!(i.kind, InstructionKind::AsyncRegistrationPollClose { .. }) {
                Type::Enum {
                    name: format!("@std.task:v1:Poll:{}", Type::Unit.fingerprint()),
                    variants: vec![
                        ("Pending".into(), vec![]),
                        ("Ready".into(), vec![Type::Unit]),
                    ],
                }
            } else {
                Type::Unit
            }
        }
        InstructionKind::AsyncRegistrationFinishDestroy { registration }
        | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
            let ty = value_type(f, registration, span)?;
            if !matches!(ty,Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..})) {
                return Err(error(span, "async finish requires actual owning head"));
            }
            carrier(module, ty, span, budget)?;
            Type::Unit
        }
        InstructionKind::RegistrationStart {
            register,
            close,
            callback,
            ..
        } => {
            for (id, role) in [
                (register, ScopedCallbackRole::Register),
                (close, ScopedCallbackRole::Close),
            ] {
                let target = module
                    .functions
                    .get(id.0 as usize)
                    .ok_or_else(|| error(span, "registration static role absent"))?;
                if target.foreign.as_ref().and_then(|f| f.scoped_callback) != Some(role) {
                    return Err(error(span, "registration static target has wrong role"));
                }
                scoped_callback::declaration(target, budget)?;
            }
            tasks::require_synchronous_value(module, f, callback, budget, span)?;
            let Type::Closure(id) = value_type(f, callback, span)? else {
                return Err(error(
                    span,
                    "registration requires actual consumed concrete F",
                ));
            };
            let c = closures::definition(module, *id, span)?;
            budget.spend(c.parameters.len().saturating_add(1))?;
            if !matches!(c.kind, CallableKind::Fn | CallableKind::FnMut)
                || c.parameters != [Type::I32]
                || c.result != Type::I32
            {
                return Err(error(
                    span,
                    "registration requires actual Fn/FnMut(i32)->i32",
                ));
            }
            let t = Type::Managed(Box::new(Type::Registration {
                environment: Box::new(Type::Closure(*id)),
            }));
            carrier(module, &t, span, budget)?;
            t
        }
        InstructionKind::RegistrationValid { registration } => {
            let t = value_type(f, registration, span)?;
            if !matches!(t, Type::Desc(p) if matches!(p.as_ref(),Type::Registration{..})) {
                return Err(error(
                    span,
                    "registration validity requires actual descriptor",
                ));
            };
            carrier(module, t, span, budget)?;
            Type::Bool
        }
        InstructionKind::RegistrationClose { registration } => {
            let t = value_type(f, registration, span)?;
            if !matches!(t, Type::Managed(p) if matches!(p.as_ref(),Type::Registration{..})) {
                return Err(error(
                    span,
                    "registration close consumes actual owning head",
                ));
            };
            carrier(module, t, span, budget)?;
            Type::Unit
        }
        _ => unreachable!(),
    }))
}

pub(super) fn factory(module: &TypedModule, f: &TypedFunction, budget: &mut Budget) -> Check<bool> {
    if f.callable_contract.is_some() {
        return Ok(false);
    }
    let Type::Managed(payload) = &f.return_type else {
        return Ok(false);
    };
    let (Type::Registration { environment } | Type::AsyncRegistration { environment }) =
        payload.as_ref()
    else {
        return Ok(false);
    };
    let Type::Closure(id) = environment.as_ref() else {
        return Err(error(
            Some(f.span),
            "registration factory environment absent",
        ));
    };
    budget.spend(f.parameters.len().saturating_add(1))?;
    let mut input = None;
    for (index, local) in f.parameters.iter().enumerate() {
        let t = local_type(f, *local, Some(f.span))?;
        if t == environment.as_ref() {
            if input.replace(index).is_some() {
                return Err(error(
                    Some(f.span),
                    "registration factory requires one actual F input",
                ));
            }
        } else {
            resources::charge_payload(t, budget)?;
            if !t.is_copy_value() {
                return Err(error(
                    Some(f.span),
                    "registration factory ordinary input is outside finite Copy profile",
                ));
            }
        }
    }
    if input.is_none()
        || f.foreign.is_some()
        || f.environment.is_some()
        || !matches!(f.result_mode, PassingMode::Create | PassingMode::Give)
    {
        return Err(error(
            Some(f.span),
            "registration factory must transfer actual F into create/give owner result",
        ));
    }
    closed(
        module,
        *id,
        Some(f.span),
        budget,
        matches!(payload.as_ref(), Type::AsyncRegistration { .. }),
    )?;
    Ok(true)
}

/// Inventory every actual shape, including unused declarations. No transported
/// Closed/producer bit grants ordinary nested storage permission.
pub(super) fn storage(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    if !budget.async_registrations {
        return Ok(());
    }
    let mut actual_creations = Vec::new();
    if budget.owned_task_input_inventory {
        budget.spend(module.functions.len())?;
        for f in &module.functions {
            budget.spend(f.blocks.len())?;
            for i in f.blocks.iter().flat_map(|b| &b.instructions) {
                budget.spend(1)?;
                if let InstructionKind::TaskCreateOwned { arguments, .. } = &i.kind {
                    budget.spend(1)?;
                    actual_creations.push((f, arguments.as_slice()));
                }
            }
        }
    }
    let roots = module
        .resources
        .iter()
        .map(|r| (&r.payload, false))
        .chain(
            module
                .functions
                .iter()
                .flat_map(|f| {
                    f.values
                        .iter()
                        .chain(f.locals.iter().map(|l| &l.ty))
                        .chain(std::iter::once(&f.return_type))
                })
                .map(|t| (t, false)),
        )
        .chain(module.closures.iter().flat_map(|c| {
            c.captures.iter().map(|t| (t, true)).chain(
                c.parameters
                    .iter()
                    .chain(std::iter::once(&c.result))
                    .map(|t| (t, false)),
            )
        }));
    for (root, blocked) in roots {
        let mut pending = vec![(root, blocked)];
        while let Some((ty, blocked)) = pending.pop() {
            budget.spend(1)?;
            if blocked && matches!(ty, Type::AsyncRegistration { .. }) {
                return Err(error(
                    None,
                    "pollable registration roots in ordinary aggregate storage or closure captures require an asynchronous ownership consumer",
                ));
            }
            let count = match ty {
                Type::Record { fields, .. } | Type::Package { fields, .. } => fields.len(),
                Type::Tuple(fields) => fields.len(),
                Type::Enum { variants, .. } => {
                    budget.spend(variants.len())?;
                    variants
                        .iter()
                        .fold(0usize, |n, (_, fields)| n.saturating_add(fields.len()))
                }
                Type::ErrorUnion { .. } => 2,
                Type::AsyncRegistration { .. }
                | Type::Registration { .. }
                | Type::BorrowedStorage { .. }
                | Type::Task { .. }
                | Type::Address { .. }
                | Type::Managed(_)
                | Type::Desc(_)
                | Type::Opaque { .. }
                | Type::Array { .. }
                | Type::Slice { .. }
                | Type::OwnedStore { .. } => 1,
                Type::FunctionPointer { parameters, .. }
                | Type::BorrowedFunctionPointer { parameters, .. }
                | Type::ForeignCallback { parameters, .. } => parameters.len().saturating_add(1),
                _ => 0,
            };
            budget.spend(count)?;
            let mut children = Vec::with_capacity(count);
            let mut blocked = blocked;
            match ty {
                Type::Package { name, fields } => {
                    blocked |= !name.starts_with("@std.task:v1:TaskCreateFailure:");
                    children.extend(fields.iter().map(|(_, t)| t));
                }
                Type::OwnedStore { element } => {
                    blocked = true;
                    children.push(element.as_ref());
                }
                Type::Record { fields, .. } => children.extend(fields.iter().map(|(_, t)| t)),
                Type::Tuple(fields) => {
                    let mut present = false;
                    for (f, args) in &actual_creations {
                        budget.spend(fields.len().saturating_add(args.len()))?;
                        if fields.len() != args.len() {
                            continue;
                        }
                        let mut exact = true;
                        for (field, id) in fields.iter().zip(*args) {
                            let actual = f
                                .values
                                .get(id.0 as usize)
                                .ok_or_else(|| error(None, "owned input SSA absent"))?;
                            resources::charge_payload(field, budget)?;
                            resources::charge_payload(actual, budget)?;
                            exact &= field == actual;
                        }
                        present |= exact;
                    }
                    blocked |= !present;
                    children.extend(fields);
                }
                Type::Enum { variants, .. } => {
                    blocked = true;
                    for (_, fields) in variants {
                        children.extend(fields);
                    }
                }
                Type::ErrorUnion { error, result } => {
                    let admitted = matches!(error.as_ref(),Type::Package{name,..}|Type::Record{name,..} if name.starts_with("@std.task:v1:TaskCreateFailure:"))
                        && matches!(result.as_ref(),Type::Managed(t) if matches!(t.as_ref(),Type::Task{..}));
                    blocked |= !admitted;
                    children.extend([error.as_ref(), result.as_ref()])
                }
                Type::AsyncRegistration { environment: inner }
                | Type::Registration { environment: inner }
                | Type::BorrowedStorage { storage: inner, .. }
                | Type::Task { result: inner, .. }
                | Type::Address { inner, .. }
                | Type::Managed(inner)
                | Type::Desc(inner)
                | Type::Opaque { hidden: inner, .. }
                | Type::Array { element: inner, .. }
                | Type::Slice { element: inner } => children.push(inner.as_ref()),
                Type::FunctionPointer { parameters, result }
                | Type::BorrowedFunctionPointer {
                    parameters, result, ..
                }
                | Type::ForeignCallback { parameters, result } => {
                    children.extend(parameters);
                    children.push(result.as_ref());
                }
                _ => {}
            }
            pending.extend(children.into_iter().map(|t| (t, blocked)));
        }
    }
    Ok(())
}

pub(crate) fn transfer_callback(
    module: &TypedModule,
    id: ClosureId,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    threaded::actual(module, id, span, budget)
}
