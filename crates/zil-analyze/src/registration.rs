//! Independent actual retained-owner and closed-environment admission.
use super::*;
use zil_base::Span;
const PREFIX: &str = "@builtin:Registration:";
fn fail(span: Span, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-REGISTRATION",
        Some(span),
        message,
    )]
}
pub(super) fn resource(ty: &Type) -> bool {
    matches!(ty,Type::Managed(p)|Type::Desc(p) if matches!(p.as_ref(),Type::Registration{..}))
}
fn definition(
    module: &TypedModule,
    payload: &Type,
    budget: &mut Budget,
    span: Span,
) -> CompileResult<()> {
    let Type::Registration { environment } = payload else {
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
    let name = format!("{PREFIX}{}", id.0);
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
    closed(module, *id, budget, span)
}
fn closed(
    module: &TypedModule,
    root: ClosureId,
    budget: &mut Budget,
    span: Span,
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
                Type::Address { .. }
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
                Type::BorrowedStorage { storage: t, .. }
                | Type::Managed(t)
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
    for r in &module.resources {
        budget.spend(1, "registration resource inventory")?;
        if matches!(r.payload, Type::Registration { .. }) || r.name.starts_with(PREFIX) {
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
    let Type::Registration { environment } = p.as_ref() else {
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
    closed(module, *id, budget, f.span)?;
    Ok(true)
}
