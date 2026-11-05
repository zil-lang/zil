//! Producer reconstruction of closed inline-resource continuation profile. This checks actual
//! input storage AND every ordinary teardown's reachable code before allowing
//! synchronous close. A result-only Task type never proves either fact.
use super::*;
fn local_type(
    f: &TypedFunction,
    local: LocalId,
    span: Option<zil_base::Span>,
) -> CompileResult<&Type> {
    f.locals
        .get(local.0 as usize)
        .map(|l| &l.ty)
        .ok_or_else(|| fail(span.unwrap_or(f.span), "owned task local absent"))
}
fn value_type(
    f: &TypedFunction,
    value: ValueId,
    span: Option<zil_base::Span>,
) -> CompileResult<&Type> {
    f.values
        .get(value.0 as usize)
        .ok_or_else(|| fail(span.unwrap_or(f.span), "owned task value absent"))
}
fn reject(_: &str, span: Option<zil_base::Span>, message: &str) -> Vec<Diagnostic> {
    fail(span.unwrap_or_default(), message)
}
fn scalar(t: &Type) -> bool {
    matches!(
        t,
        Type::Unit | Type::Bool | Type::U8 | Type::U16 | Type::I32 | Type::I64
    )
}
fn record(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(t, false, "actual owned task type")?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        budget.spend(1, "actual owning task profile")?;
        match t {
            t if scalar(t) => {}
            Type::Record { fields, .. } => {
                budget.spend(fields.len(), "actual owning task profile")?;
                pending.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Tuple(fields) => pending.extend(fields),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                pending.push(error);
                pending.push(result);
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}
pub(super) fn result_type(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    if *t == Type::I32 {
        return Ok(true);
    }
    if let Type::Managed(payload) = t {
        return record(payload, budget);
    }
    record(t, budget)
}
fn package(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    let Type::Package { fields, .. } = t else {
        return Ok(false);
    };
    budget.spend(fields.len(), "actual single-owner package")?;
    let mut owners = 0;
    for (_, field) in fields {
        if let Type::Managed(payload) = field {
            owners += 1;
            if !record(payload, budget)? {
                return Ok(false);
            }
        } else if !record(field, budget)? {
            return Ok(false);
        }
    }
    Ok(owners == 1)
}
fn tuple(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    let Type::Tuple(fields) = t else {
        return Ok(false);
    };
    for field in fields {
        if let Type::Managed(payload) = field {
            if !record(payload, budget)? {
                return Ok(false);
            }
        } else if !record(field, budget)? {
            return Ok(false);
        }
    }
    Ok(true)
}
// Whole Enum roots retain one canonical obligation. Their alternatives are
// not simultaneous owners; ordinary Sum transfer checks the active tag.
// Keep the existing Copy classification first so old frame costs stay exact.
fn complete_enum(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.type_work(t, false, "complete Enum task storage")?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        budget.spend(1, "complete Enum task storage")?;
        match t {
            Type::Enum { variants, .. } => {
                budget.spend(variants.len(), "complete Enum task variants")?;
                for (_, fields) in variants {
                    budget.spend(fields.len(), "complete Enum task fields")?;
                    pending.extend(fields);
                }
            }
            Type::Managed(payload) => {
                if !record(payload, budget)? {
                    return Ok(false);
                }
            }
            t if record(t, budget)? => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn storage(t: &Type, budget: &mut Budget) -> CompileResult<bool> {
    budget.spend(1, "actual frame storage constructor")?;
    match t {
        Type::Managed(p) | Type::Desc(p) => record(p, budget),
        Type::Record { .. } | Type::Enum { .. } | Type::ErrorUnion { .. } => record(t, budget),
        Type::Package { .. } => package(t, budget),
        Type::Tuple(_) => tuple(t, budget),
        Type::Address { inner, .. } => storage(inner, budget),
        t => Ok(scalar(t)),
    }
}
// A private continuation profile, independently reconstructed from actual
// concrete captures. It grants no Closed/no-future state and admits no borrow.
fn pollable_storage(
    module: &TypedModule,
    t: &Type,
    budget: &mut Budget,
    enum_storage: &mut bool,
) -> CompileResult<bool> {
    budget.type_work(t, false, "pollable frame actual type")?;
    budget.spend(1, "pollable frame actual storage")?;
    match t {
        Type::Managed(p) | Type::Desc(p)
            if matches!(p.as_ref(), Type::AsyncRegistration { .. }) =>
        {
            let Type::AsyncRegistration { environment } = p.as_ref() else {
                unreachable!()
            };
            pollable_storage(module, environment, budget, enum_storage)
        }
        Type::Address { inner, .. } => pollable_storage(module, inner, budget, enum_storage),
        Type::Enum { .. } => {
            if storage(t, budget)? {
                Ok(true)
            } else if complete_enum(t, budget)? {
                *enum_storage = true;
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Type::Closure(id) => {
            let Some(c) = module.closures.get(id.0 as usize) else {
                return Ok(false);
            };
            if !c.lending_captures.is_empty() {
                return Ok(false);
            }
            for capture in &c.captures {
                if let Type::Managed(payload) = capture {
                    if !record(payload, budget)? {
                        return Ok(false);
                    }
                    budget.spend(module.resources.len(), "actual pollable capture resource")?;
                    for _ in &module.resources {
                        budget.type_work(payload, false, "actual pollable capture cleanup")?;
                    }
                    let Some(resource) = module.resources.iter().find(|r| r.payload == **payload)
                    else {
                        return Ok(false);
                    };
                    if let Some(drop) = resource.teardown {
                        cleanup(module, drop, budget, None)?;
                    }
                } else if !record(capture, budget)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => storage(t, budget),
    }
}
fn cleanup(
    module: &TypedModule,
    first: FunctionId,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<()> {
    let mut pending = vec![first];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.spend(seen.len().saturating_add(1), "actual owning task profile")?;
        if !seen.insert(id) {
            continue;
        }
        let f = module
            .functions
            .get(id.0 as usize)
            .ok_or_else(|| reject("E-VERIFY-TASK", span, "task cleanup target absent"))?;
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(f.parameters.len()),
            "actual owning task profile",
        )?;
        if f.call_kind != CallKind::Direct
            || f.environment.is_some()
            || f.callable_contract.is_some()
            || f.callable_return_contract.is_some()
            || !record(&f.return_type, budget)?
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "task cleanup must have an actual closed synchronous Direct graph",
            ));
        }
        if f.foreign.is_some() {
            if !scalar(&f.return_type)
                || f.parameters
                    .iter()
                    .any(|p| !scalar(&f.locals[p.0 as usize].ty))
                || f.foreign
                    .as_ref()
                    .is_some_and(|x| x.scoped_callback.is_some())
            {
                return Err(reject(
                    "E-VERIFY-TASK",
                    span,
                    "task cleanup cannot publish frame storage to a foreign entry",
                ));
            }
            continue;
        }
        for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            if !storage(t, budget)? {
                return Err(reject(
                    "E-VERIFY-TASK",
                    span,
                    "task cleanup retains unsupported owned or borrowed storage",
                ));
            }
        }
        for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
            if let Type::Managed(payload) = t {
                for _ in &module.resources {
                    budget.type_work(payload, false, "actual cleanup resource equality")?;
                }
                if let Some(drop) = module
                    .resources
                    .iter()
                    .find(|r| r.payload == **payload)
                    .and_then(|r| r.teardown)
                {
                    pending.push(drop);
                }
            }
        }
        for b in &f.blocks {
            for i in &b.instructions {
                budget.spend(1, "actual owning task profile")?;
                match &i.kind {
                    InstructionKind::Call { function, .. }
                    | InstructionKind::ForeignCall { function, .. } => pending.push(*function),
                    InstructionKind::Constant(_)
                    | InstructionKind::Load(_)
                    | InstructionKind::Store { .. }
                    | InstructionKind::StorageLive(_)
                    | InstructionKind::StorageDead(_)
                    | InstructionKind::ScopeEnter { .. }
                    | InstructionKind::ScopeExit { .. }
                    | InstructionKind::ReadDesc { .. }
                    | InstructionKind::WriteDesc { .. }
                    | InstructionKind::UnsafeEnter { .. }
                    | InstructionKind::UnsafeExit { .. }
                    | InstructionKind::MakeManaged { .. }
                    | InstructionKind::DropManaged { .. }
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::DescribeLocal { .. }
                    | InstructionKind::Unary { .. }
                    | InstructionKind::Binary { .. }
                    | InstructionKind::ConvertInteger { .. }
                    | InstructionKind::MakeAggregate { .. }
                    | InstructionKind::MakeVariant { .. }
                    | InstructionKind::VariantTag { .. }
                    | InstructionKind::ReadVariant { .. }
                    | InstructionKind::MakeErrorUnion { .. }
                    | InstructionKind::ErrorTag { .. }
                    | InstructionKind::ReadErrorUnion { .. } => {}
                    _ => {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            Some(i.span),
                            "task cleanup contains an asynchronous or external-user operation",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
pub(super) fn profile(
    module: &TypedModule,
    f: &TypedFunction,
    budget: &mut Budget,
    span: Option<zil_base::Span>,
) -> CompileResult<Vec<FunctionId>> {
    if f.foreign.is_some()
        || f.environment.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || !(f.result_mode == PassingMode::Infer && record(&f.return_type, budget)?
            || matches!(f.result_mode, PassingMode::Give | PassingMode::Create)
                && matches!(f.return_type, Type::Managed(_))
                && result_type(&f.return_type, budget)?)
        || module.resources.iter().any(|r| r.teardown == Some(f.id))
    {
        return Err(reject(
            "E-VERIFY-TASK",
            span,
            "owned task worker requires a closed actual frame and complete supported result",
        ));
    }
    // No asynchronous rollback path exists when allocating a new Task fails.
    // This first slice constructs pollable owners inside the stable worker.
    for parameter in &f.parameters {
        let mut pending = vec![local_type(f, *parameter, span)?];
        while let Some(t) = pending.pop() {
            budget.spend(1, "pollable frame actual storage")?;
            match t {
                Type::AsyncRegistration { .. } => {
                    return Err(fail(
                        f.span,
                        "live pollable Task input requires allocation-failure asynchronous drain",
                    ));
                }
                Type::Managed(p) | Type::Desc(p) | Type::Address { inner: p, .. } => {
                    pending.push(p)
                }
                Type::Tuple(ts) => pending.extend(ts),
                Type::Package { fields, .. } | Type::Record { fields, .. } => {
                    pending.extend(fields.iter().map(|(_, t)| t))
                }
                _ => {}
            }
        }
    }
    for parameter in &f.parameters {
        if !matches!(local_type(f, *parameter, span)?, Type::Managed(_))
            && !record(local_type(f, *parameter, span)?, budget)?
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "task initial frame cannot retain borrowed input storage",
            ));
        }
    }
    let mut checked = BTreeSet::new();
    for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
        let mut enum_storage = false;
        if !pollable_storage(module, t, budget, &mut enum_storage)? {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "task owner needs a complete canonical inline resource frame slot",
            ));
        }
        if enum_storage && f.return_type != Type::I32 {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "complete Enum frame storage currently requires the existing i32 task result",
            ));
        }
        let mut leaves = match t {
            Type::Package { fields, .. } => fields.iter().map(|(_, t)| t).collect::<Vec<_>>(),
            Type::Tuple(fields) => fields.iter().collect(),
            _ => vec![t],
        };
        if enum_storage {
            // Only newly admitted Enum storage needs transitive leaf discovery.
            // Keep old Copy/Managed/tuple/package classification and fees intact.
            let mut pending = leaves;
            leaves = Vec::new();
            while let Some(leaf) = pending.pop() {
                budget.spend(1, "complete Enum task cleanup leaf")?;
                match leaf {
                    Type::Enum { variants, .. } => {
                        budget.spend(variants.len(), "complete Enum task cleanup variants")?;
                        for (_, fields) in variants {
                            budget.spend(fields.len(), "complete Enum task cleanup fields")?;
                            pending.extend(fields);
                        }
                    }
                    Type::Address { inner, .. } => pending.push(inner),
                    Type::Managed(_) => leaves.push(leaf),
                    _ => {}
                }
            }
        }
        for leaf in leaves {
            if let Type::Managed(payload) = leaf {
                for _ in &module.resources {
                    budget.type_work(payload, false, "actual task resource equality")?;
                }
                let resource = module
                    .resources
                    .iter()
                    .find(|r| r.payload == **payload)
                    .ok_or_else(|| {
                        reject(
                            "E-VERIFY-TASK",
                            span,
                            "owned task slot has no actual resource",
                        )
                    })?;
                if let Some(drop) = resource.teardown
                    && checked.insert(drop)
                {
                    cleanup(module, drop, budget, span)?;
                }
            }
        }
    }
    let mut children = vec![];
    for b in &f.blocks {
        for i in &b.instructions {
            budget.spend(1, "actual owning task profile")?;
            match &i.kind {
                InstructionKind::Call { function, .. } => {
                    cleanup(module, *function, budget, Some(i.span))?
                }
                InstructionKind::SuspendCall {
                    function,
                    arguments,
                } => {
                    budget.spend(arguments.len().saturating_add(1), "child actual signature")?;
                    let target = module
                        .functions
                        .get(function.0 as usize)
                        .ok_or_else(|| fail(i.span, "child target absent"))?;
                    if arguments.len() != target.parameters.len() {
                        return Err(fail(i.span, "child actual arity differs"));
                    }
                    for (arg, param) in arguments.iter().zip(&target.parameters) {
                        let actual = value_type(f, *arg, Some(i.span))?;
                        let wanted = local_type(target, *param, Some(i.span))?;
                        budget.type_work(actual, false, "child actual input")?;
                        budget.type_work(wanted, false, "child signature input")?;
                        if actual != wanted {
                            return Err(fail(i.span, "child actual input type differs"));
                        }
                    }
                    children.push(*function);
                }
                InstructionKind::AsyncRegistrationStart { .. }
                | InstructionKind::AsyncRegistrationValid { .. }
                | InstructionKind::AsyncRegistrationRequestClose { .. }
                | InstructionKind::AsyncRegistrationPollClose { .. }
                | InstructionKind::AsyncRegistrationFinishDestroy { .. }
                | InstructionKind::MakeClosure { .. }
                | InstructionKind::DropClosure { .. }
                | InstructionKind::UnsafeEnter { .. }
                | InstructionKind::UnsafeExit { .. }
                | InstructionKind::Constant(_)
                | InstructionKind::Load(_)
                | InstructionKind::Store { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::Unary { .. }
                | InstructionKind::Binary { .. }
                | InstructionKind::ConvertInteger { .. }
                | InstructionKind::TaskYield
                | InstructionKind::MakeManaged { .. }
                | InstructionKind::TakeLocal { .. }
                | InstructionKind::DescribeLocal { .. }
                | InstructionKind::ReadDesc { .. }
                | InstructionKind::WriteDesc { .. }
                | InstructionKind::DropManaged { .. }
                | InstructionKind::MakeOwnedTuple { .. }
                | InstructionKind::DropTuple { .. }
                | InstructionKind::ExtractField { .. }
                | InstructionKind::BorrowFieldLocal { .. }
                | InstructionKind::BorrowPlaceLocal { .. }
                | InstructionKind::ReadAddress { .. }
                | InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::AddressField { .. }
                | InstructionKind::ReborrowAddress { .. }
                | InstructionKind::MakePackage { .. }
                | InstructionKind::DropPackage { .. }
                | InstructionKind::TakePackageField { .. }
                | InstructionKind::StorePackageField { .. }
                | InstructionKind::DropPackageField { .. }
                | InstructionKind::MakeAggregate { .. }
                | InstructionKind::MakeVariant { .. }
                | InstructionKind::VariantTag { .. }
                | InstructionKind::ReadVariant { .. }
                | InstructionKind::VariantTagLocal { .. }
                | InstructionKind::ReadVariantLocal { .. }
                | InstructionKind::UnpackVariant { .. }
                | InstructionKind::DropEnum { .. }
                | InstructionKind::MakeErrorUnion { .. }
                | InstructionKind::ErrorTag { .. }
                | InstructionKind::ReadErrorUnion { .. } => {}
                _ => {
                    return Err(reject(
                        "E-VERIFY-TASK",
                        Some(i.span),
                        "task owned frame operation requires complete saved responsibility lowering",
                    ));
                }
            }
        }
    }
    Ok(children)
}
