//! Independent actual retained-owner and closed-environment admission.
use super::*;
use zil_base::Span;
const PREFIX: &str = "@builtin:Registration:";
pub(super) const ASYNC_PREFIX: &str = "@builtin:AsyncRegistration:";
fn fail(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-REGISTRATION",
        Some(span),
        message,
    )]
}
pub(super) fn resource(ty: &Type) -> bool {
    matches!(ty,Type::Managed(p)|Type::Desc(p) if matches!(p.as_ref(),Type::Registration{..}|Type::AsyncRegistration{..}))
}
fn definition(
    module: &TypedModule,
    payload: &Type,
    budget: &mut Budget,
    span: Span,
) -> CompileResult<()> {
    let (Type::Registration { environment } | Type::AsyncRegistration { environment }) = payload
    else {
        return Err(fail(span, "registration payload is absent"));
    };
    let Type::Closure(id) = environment.as_ref() else {
        return Err(fail(
            span,
            "registration requires actual concrete closure type",
        ));
    };
    budget.spend(
        module.resources.len().saturating_add(32),
        "registration sealed resource definition",
    )?;
    let prefix = if matches!(payload, Type::AsyncRegistration { .. }) {
        ASYNC_PREFIX
    } else {
        PREFIX
    };
    let name = format!("{prefix}{}", id.0);
    if !module.target.is_supported_x64_llvm()
        || (module
            .resources
            .iter()
            .filter(|r| r.payload == *payload)
            .count()
            != 1
            || !module
                .resources
                .iter()
                .any(|r| r.payload == *payload && r.name == name && r.teardown.is_none()))
    {
        return Err(fail(
            span,
            "registration requires one canonical sealed owning definition",
        ));
    }
    closed(
        module,
        *id,
        budget,
        span,
        matches!(payload, Type::AsyncRegistration { .. }),
    )
}
fn closed(
    module: &TypedModule,
    root: ClosureId,
    budget: &mut Budget,
    span: Span,
    asynchronous: bool,
) -> CompileResult<()> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root];
    let mut functions = Vec::new();
    while let Some(id) = pending.pop() {
        budget.lookup(seen.len(), "registration actual capture lookup")?;
        if !seen.insert(id) {
            continue;
        }
        let c = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| fail(span, "registration actual closure is absent"))?;
        budget.spend(
            c.captures
                .len()
                .saturating_add(c.lending_captures.len())
                .saturating_add(1),
            "registration actual capture graph",
        )?;
        if !c.lending_captures.is_empty() {
            return Err(fail(
                span,
                "ClosedEnvironment cannot retain borrowed capture positions",
            ));
        }
        budget.type_work(
            &c.result,
            false,
            "registration actual entry result equality",
        )?;
        for parameter in &c.parameters {
            budget.type_work(
                parameter,
                false,
                "registration actual entry parameter equality",
            )?;
        }
        foreign_scoped::entry(module, c, budget)?;
        functions.push(c.function);
        let mut types = c.captures.iter().collect::<Vec<_>>();
        while let Some(t) = types.pop() {
            budget.spend(1, "registration stored environment shape")?;
            match t {
                Type::AsyncRegistration { .. } => {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "ClosedEnvironment cannot retain a pollable registration with asynchronous teardown",
                        Some(span),
                    )]);
                }
                Type::TaskI32 if asynchronous => {
                    return Err(fail(
                        span,
                        "AsyncRegistration environment has no actual legacy Task frame synchronous-cleanup proof",
                    ));
                }
                Type::Task { .. }
                | Type::Dyn { .. }
                | Type::Address { .. }
                | Type::Desc(_)
                | Type::Slice { .. }
                | Type::Named(_)
                | Type::Unsupported(_)
                | Type::SyncCell(_) => {
                    return Err(fail(
                        span,
                        "ClosedEnvironment retains an external loan or unresolved storage",
                    ));
                }
                Type::Closure(id) => {
                    budget.spend(1, "registration nested concrete environment")?;
                    pending.push(*id)
                }
                Type::Managed(t) => {
                    budget.spend(
                        module.resources.len(),
                        "registration actual environment teardown inventory",
                    )?;
                    for resource in &module.resources {
                        budget.type_work(
                            &resource.payload,
                            false,
                            "registration actual teardown payload equality",
                        )?;
                        if resource.payload == **t
                            && let Some(teardown) = resource.teardown
                        {
                            functions.push(teardown);
                        }
                    }
                    budget.spend(1, "registration managed environment payload")?;
                    types.push(t);
                }
                Type::BorrowedStorage { storage: t, .. }
                | Type::Opaque { hidden: t, .. }
                | Type::OwnedStore { element: t }
                | Type::Registration { environment: t }
                | Type::Array { element: t, .. } => {
                    budget.spend(1, "registration stored child")?;
                    types.push(t)
                }
                Type::Tuple(ts) => {
                    budget.spend(ts.len(), "registration stored tuple")?;
                    types.extend(ts)
                }
                Type::Record { fields, .. } | Type::Package { fields, .. } => {
                    budget.spend(fields.len(), "registration stored product")?;
                    types.extend(fields.iter().map(|(_, t)| t))
                }
                Type::Enum { variants, .. } => {
                    budget.spend(variants.len(), "registration stored variants")?;
                    for (_, ts) in variants {
                        budget.spend(ts.len(), "registration stored variant fields")?;
                        types.extend(ts)
                    }
                }
                Type::ErrorUnion { error, result } => {
                    budget.spend(2, "registration stored alternatives")?;
                    types.extend([error.as_ref(), result.as_ref()])
                }
                // Code signatures are not captured data addresses.
                Type::BorrowedFunctionPointer { .. }
                | Type::FunctionPointer { .. }
                | Type::ForeignCallback { .. }
                | Type::Unit
                | Type::Bool
                | Type::U8
                | Type::U16
                | Type::I32
                | Type::I64
                | Type::U64
                | Type::Usize
                | Type::F32
                | Type::F64
                | Type::ForeignHandle
                | Type::CWideString
                | Type::ByteBuffer
                | Type::TaskI32 => {}
            }
        }
    }
    let mut visited = BTreeSet::new();
    while let Some(id) = functions.pop() {
        budget.lookup(visited.len(), "registration Direct graph lookup")?;
        if !visited.insert(id) {
            continue;
        }
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| fail(span, "registration actual code target is absent"))?;
        if f.call_kind != CallKind::Direct {
            return Err(fail(span, "retained callback target selects Suspend ABI"));
        }
        for b in &f.blocks {
            budget.spend(1, "registration Direct block")?;
            for i in &b.instructions {
                budget.spend(1, "registration actual Direct instruction")?;
                match i.kind {
                    InstructionKind::TaskYield | InstructionKind::SuspendCall { .. } => {
                        return Err(fail(
                            i.span,
                            "retained callback has no Suspend invocation authority",
                        ));
                    }
                    InstructionKind::Call { function, .. }
                    | InstructionKind::FunctionRef { function }
                    | InstructionKind::ForeignFunctionRef { function } => {
                        budget.spend(1, "registration Direct dependency")?;
                        functions.push(function)
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}
pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if !budget.registrations {
        return Ok(());
    }
    storage(module, budget)?;
    for r in &module.resources {
        budget.spend(1, "registration resource inventory")?;
        if matches!(
            r.payload,
            Type::Registration { .. } | Type::AsyncRegistration { .. }
        ) || r.name.starts_with(PREFIX)
            || r.name.starts_with(ASYNC_PREFIX)
        {
            definition(module, &r.payload, budget, Span::default())?
        }
    }
    for f in &module.functions {
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(1),
            "registration carrier inventory",
        )?;
        for ty in f
            .values
            .iter()
            .chain(f.locals.iter().map(|l| &l.ty))
            .chain(std::iter::once(&f.return_type))
        {
            if resource(ty) {
                let (Type::Managed(p) | Type::Desc(p)) = ty else {
                    unreachable!()
                };
                definition(module, p, budget, f.span)?
            }
        }
        for b in &f.blocks {
            budget.spend(1, "registration block")?;
            for i in &b.instructions {
                budget.spend(1, "registration instruction")?;
                let value = |id: ValueId| {
                    f.values
                        .get(id.0 as usize)
                        .ok_or_else(|| fail(i.span, "registration operand outside actual values"))
                };
                let expected = match i.kind {
                    InstructionKind::AsyncRegistrationStart {
                        register,
                        request_close,
                        poll_drain,
                        callback,
                        ..
                    } => {
                        for (id, role) in [
                            (register, ScopedCallbackRole::AsyncRegister),
                            (request_close, ScopedCallbackRole::AsyncRequestClose),
                            (poll_drain, ScopedCallbackRole::AsyncPollDrain),
                        ] {
                            let target = module
                                .functions
                                .get(id.0 as usize)
                                .ok_or_else(|| fail(i.span, "async provider target absent"))?;
                            foreign_scoped::declaration(target, role, budget)?;
                        }
                        let Type::Closure(id) = value(callback)? else {
                            return Err(fail(
                                i.span,
                                "async registration requires actual consumed concrete F",
                            ));
                        };
                        let closure = module
                            .closures
                            .get(id.0 as usize)
                            .ok_or_else(|| fail(i.span, "async callback absent"))?;
                        budget.spend(
                            closure.parameters.len().saturating_add(1),
                            "async callback actual signature",
                        )?;
                        if !matches!(closure.kind, CallableKind::Fn | CallableKind::FnMut)
                            || closure.parameters != [Type::I32]
                            || closure.result != Type::I32
                        {
                            return Err(fail(
                                i.span,
                                "async callback requires actual FnMut(i32)->i32",
                            ));
                        }
                        let payload = Type::AsyncRegistration {
                            environment: Box::new(Type::Closure(*id)),
                        };
                        definition(module, &payload, budget, i.span)?;
                        Some(Type::Managed(Box::new(payload)))
                    }
                    InstructionKind::AsyncRegistrationValid { registration } => {
                        if !matches!(value(registration)?,Type::Desc(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
                        {
                            return Err(fail(i.span, "async validity requires actual descriptor"));
                        }
                        Some(Type::Bool)
                    }
                    InstructionKind::AsyncRegistrationRequestClose { registration }
                    | InstructionKind::AsyncRegistrationPollClose { registration } => {
                        let Type::Address {
                            mutable: true,
                            inner,
                        } = value(registration)?
                        else {
                            return Err(fail(
                                i.span,
                                "async close requires exclusive complete head address",
                            ));
                        };
                        let Type::Managed(payload) = inner.as_ref() else {
                            return Err(fail(i.span, "async close requires actual owning head"));
                        };
                        if !matches!(payload.as_ref(), Type::AsyncRegistration { .. }) {
                            return Err(fail(i.span, "async close requires pollable payload"));
                        }
                        definition(module, payload, budget, i.span)?;
                        Some(
                            if matches!(i.kind, InstructionKind::AsyncRegistrationPollClose { .. })
                            {
                                Type::Enum {
                                    name: format!("@std.task:v1:Poll:{}", Type::Unit.fingerprint()),
                                    variants: vec![
                                        ("Pending".into(), vec![]),
                                        ("Ready".into(), vec![Type::Unit]),
                                    ],
                                }
                            } else {
                                Type::Unit
                            },
                        )
                    }
                    InstructionKind::AsyncRegistrationFinishDestroy { registration }
                    | InstructionKind::AsyncRegistrationFinishDestroyChecked { registration } => {
                        if !matches!(value(registration)?,Type::Managed(p) if matches!(p.as_ref(),Type::AsyncRegistration{..}))
                        {
                            return Err(fail(i.span, "async finish consumes actual owning head"));
                        }
                        Some(Type::Unit)
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
                                .ok_or_else(|| fail(i.span, "registration role target absent"))?;
                            foreign_scoped::declaration(target, role, budget)?
                        }
                        tasks::require_synchronous_value(
                            module,
                            f,
                            callback,
                            budget,
                            Some(i.span),
                        )?;
                        let Type::Closure(id) = value(callback)? else {
                            return Err(fail(
                                i.span,
                                "registration must consume an actual concrete F",
                            ));
                        };
                        let c = module
                            .closures
                            .get(id.0 as usize)
                            .ok_or_else(|| fail(i.span, "registration F absent"))?;
                        budget.spend(
                            c.parameters.len().saturating_add(1),
                            "registration actual callback signature",
                        )?;
                        if !matches!(c.kind, CallableKind::Fn | CallableKind::FnMut)
                            || c.parameters != [Type::I32]
                            || c.result != Type::I32
                        {
                            return Err(fail(
                                i.span,
                                "registration requires actual Fn/FnMut(i32)->i32",
                            ));
                        }
                        let p = Type::Registration {
                            environment: Box::new(Type::Closure(*id)),
                        };
                        definition(module, &p, budget, i.span)?;
                        Some(Type::Managed(Box::new(p)))
                    }
                    InstructionKind::RegistrationValid { registration } => {
                        if !matches!(value(registration)?,Type::Desc(p) if matches!(p.as_ref(),Type::Registration{..}))
                        {
                            return Err(fail(
                                i.span,
                                "registration_valid requires actual read descriptor",
                            ));
                        };
                        Some(Type::Bool)
                    }
                    InstructionKind::RegistrationClose { registration } => {
                        if !matches!(value(registration)?,Type::Managed(p) if matches!(p.as_ref(),Type::Registration{..}))
                        {
                            return Err(fail(
                                i.span,
                                "registration_close consumes actual owning carrier",
                            ));
                        };
                        Some(Type::Unit)
                    }
                    InstructionKind::MakeManaged { .. }
                        if i.result
                            .and_then(|v| f.values.get(v.0 as usize))
                            .is_some_and(resource) =>
                    {
                        return Err(fail(
                            i.span,
                            "only c_register constructs the sealed registration",
                        ));
                    }
                    InstructionKind::ReadDesc { desc, .. }
                    | InstructionKind::WriteDesc { desc, .. }
                        if resource(value(desc)?) =>
                    {
                        return Err(fail(
                            i.span,
                            "registration descriptor cannot expose stable bytes",
                        ));
                    }
                    _ => None,
                };
                if expected
                    .is_some_and(|t| i.result.and_then(|v| f.values.get(v.0 as usize)) != Some(&t))
                {
                    return Err(fail(
                        i.span,
                        "registration result differs from actual sealed operation",
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn factory(
    module: &TypedModule,
    f: &TypedFunction,
    budget: &mut Budget,
) -> CompileResult<bool> {
    if f.callable_contract.is_some() {
        return Ok(false);
    }
    let Type::Managed(p) = &f.return_type else {
        return Ok(false);
    };
    let (Type::Registration { environment } | Type::AsyncRegistration { environment }) = p.as_ref()
    else {
        return Ok(false);
    };
    let Type::Closure(id) = environment.as_ref() else {
        return Err(fail(
            f.span,
            "registration factory return lacks actual environment",
        ));
    };
    budget.spend(
        f.parameters.len().saturating_add(1),
        "owning registration factory actual signature",
    )?;
    let mut found = 0;
    for local in &f.parameters {
        let t = &f.locals[local.0 as usize].ty;
        if t == environment.as_ref() {
            found += 1
        } else {
            budget.type_work(t, true, "registration factory finite Copy input")?;
            if !t.is_copy_value() {
                return Err(fail(
                    f.span,
                    "registration factory extra inputs require finite Copy",
                ));
            }
        }
    }
    if found != 1
        || f.environment.is_some()
        || f.foreign.is_some()
        || !matches!(f.result_mode, PassingMode::Create | PassingMode::Give)
    {
        return Err(fail(
            f.span,
            "registration factory must own one actual F and create/give actual returned owner",
        ));
    }
    closed(
        module,
        *id,
        budget,
        f.span,
        matches!(p.as_ref(), Type::AsyncRegistration { .. }),
    )?;
    Ok(true)
}

/// Inventory every actual shape, including unused declarations. No transported
/// Closed/producer bit grants ordinary nested storage permission.
pub(super) fn storage(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    if !budget.async_registrations {
        return Ok(());
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
            budget.spend(1, "async registration storage path")?;
            if blocked && matches!(ty, Type::AsyncRegistration { .. }) {
                return Err(fail(
                    Span::default(),
                    "pollable registration roots in ordinary aggregate storage or closure captures require an asynchronous ownership consumer",
                ));
            }
            let count = match ty {
                Type::Record { fields, .. } | Type::Package { fields, .. } => fields.len(),
                Type::Tuple(fields) => fields.len(),
                Type::Enum { variants, .. } => {
                    budget.spend(variants.len(), "async registration storage variants")?;
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
            budget.spend(count, "async registration storage children")?;
            let mut children = Vec::with_capacity(count);
            let mut blocked = blocked;
            match ty {
                Type::Package { fields, .. } => {
                    blocked = true;
                    children.extend(fields.iter().map(|(_, t)| t));
                }
                Type::OwnedStore { element } => {
                    blocked = true;
                    children.push(element.as_ref());
                }
                Type::Record { fields, .. } => children.extend(fields.iter().map(|(_, t)| t)),
                Type::Tuple(fields) => {
                    blocked = true;
                    children.extend(fields);
                }
                Type::Enum { variants, .. } => {
                    blocked = true;
                    for (_, fields) in variants {
                        children.extend(fields);
                    }
                }
                Type::ErrorUnion { error, result } => {
                    blocked = true;
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
