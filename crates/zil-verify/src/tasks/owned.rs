//! Independent closed inline-resource continuation profile. This checks actual
//! input storage AND every ordinary teardown's reachable code before allowing
//! synchronous close. A result-only Task type never proves either fact.
use super::*;
fn scalar(t: &Type) -> bool {
    matches!(
        t,
        Type::Unit
            | Type::Bool
            | Type::U8
            | Type::U16
            | Type::I32
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::F32
            | Type::F64
    )
}
pub(super) fn retained_pointee(ty: &Type) -> bool {
    match ty {
        ty if scalar(ty) => true,
        Type::Record { fields, .. } => fields.iter().all(|(_, field)| retained_pointee(field)),
        Type::Tuple(fields) => fields.iter().all(retained_pointee),
        Type::Array { element, .. } | Type::Slice { element } => retained_pointee(element),
        _ => false,
    }
}
pub(super) fn retained_result(ty: &Type) -> bool {
    match ty {
        ty if scalar(ty) => true,
        Type::Managed(inner) => retained_result(inner),
        Type::Record { fields, .. } => fields.iter().all(|(_, field)| retained_result(field)),
        Type::Tuple(fields) => fields.iter().all(retained_result),
        Type::Enum { variants, .. } => variants
            .iter()
            .all(|(_, fields)| fields.iter().all(retained_result)),
        Type::ErrorUnion { error, result } => retained_result(error) && retained_result(result),
        _ => false,
    }
}
pub(super) fn borrowed_shape(
    f: &TypedFunction,
    budget: &mut Budget,
) -> Check<zil_ir::RetainedLifeShape> {
    let mut ports = Vec::new();
    let mut classes = std::collections::BTreeMap::new();
    let mut next_binder = 0u32;
    for (position, parameter) in f.parameters.iter().enumerate() {
        budget.spend(1)?;
        let ty = local_type(f, *parameter, Some(f.span))?;
        resources::charge_payload(ty, budget)?;
        if let Type::Address { mutable, inner } = ty
            && retained_pointee(inner)
        {
            let source = f.storage_life.as_ref().and_then(|life|
                super::super::signature_life::outer(life,
                    zil_ir::StorageBase::Parameter(position as u32)));
            let binder = if let Some(source) = source {
                if let Some(existing) = classes.get(&source) {
                    *existing
                } else {
                    let next = next_binder;
                    classes.insert(source, next);
                    next_binder += 1;
                    next
                }
            } else {
                let next = next_binder;
                next_binder += 1;
                next
            };
            ports.push(zil_ir::RetainedBorrowPort {
                leaf: ty.clone(),
                mode: if *mutable {
                    zil_ir::StoredBorrowMode::ExclusiveAddress
                } else {
                    zil_ir::StoredBorrowMode::SharedAddress
                },
                binder,
            });
        }
    }
    Ok(zil_ir::RetainedLifeShape {
        binder_count: next_binder,
        ports,
        retained: vec![],
        nominal_arguments: vec![],
    })
}
fn record(t: &Type, budget: &mut Budget) -> Check<bool> {
    resources::charge_payload(t, budget)?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        budget.spend(1)?;
        match t {
            t if scalar(t) => {}
            Type::Record { fields, .. } => {
                budget.spend(fields.len())?;
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
pub(super) fn result_type(t: &Type, budget: &mut Budget) -> Check<bool> {
    if *t == Type::I32 {
        return Ok(true);
    }
    if let Type::Managed(payload) = t {
        return record(payload, budget);
    }
    record(t, budget)
}
fn package(t: &Type, budget: &mut Budget) -> Check<bool> {
    let Type::Package { fields, .. } = t else {
        return Ok(false);
    };
    budget.spend(fields.len())?;
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
fn tuple(t: &Type, budget: &mut Budget) -> Check<bool> {
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
fn storage(t: &Type, budget: &mut Budget) -> Check<bool> {
    budget.spend(1)?;
    match t {
        Type::Managed(p) | Type::Desc(p) => record(p, budget),
        Type::Record { .. } | Type::Enum { .. } | Type::ErrorUnion { .. } => record(t, budget),
        Type::Package { .. } => package(t, budget),
        Type::Tuple(_) => tuple(t, budget),
        Type::Address { inner, .. } if retained_pointee(inner) => Ok(true),
        Type::Address { inner, .. } => storage(inner, budget),
        t => Ok(scalar(t)),
    }
}
// A private continuation profile, independently reconstructed from actual
// concrete captures. It grants no Closed/no-future state and admits no borrow.
fn pollable_storage(module: &TypedModule, t: &Type, budget: &mut Budget) -> Check<bool> {
    resources::charge_payload(t, budget)?;
    budget.spend(1)?;
    match t {
        Type::Managed(p) | Type::Desc(p)
            if matches!(p.as_ref(), Type::AsyncRegistration { .. }) =>
        {
            let Type::AsyncRegistration { environment } = p.as_ref() else {
                unreachable!()
            };
            pollable_storage(module, environment, budget)
        }
        Type::Address { inner, .. } if retained_pointee(inner) => Ok(true),
        Type::Address { inner, .. } => pollable_storage(module, inner, budget),
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
                    budget.spend(module.resources.len())?;
                    for _ in &module.resources {
                        resources::charge_payload(payload, budget)?;
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
// This is a whole root slot profile, not a recursive addition to storage():
// Address<Enum>, a Product containing an owning Enum and pollable captures
// must not inherit the new frame qualification.
fn complete_enum(t: &Type, budget: &mut Budget) -> Check<bool> {
    if !matches!(t, Type::Enum { .. }) {
        return Ok(false);
    }
    resources::charge_payload(t, budget)?;
    budget.spend(1)?;
    let mut pending = vec![t];
    while let Some(t) = pending.pop() {
        budget.spend(1)?;
        match t {
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::Managed(payload) => {
                if !record(payload, budget)? {
                    return Ok(false);
                }
            }
            _ if record(t, budget)? => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn enum_resources<'a>(t: &'a Type, budget: &mut Budget) -> Check<Vec<&'a Type>> {
    budget.spend(1)?;
    let mut pending = vec![t];
    let mut leaves = Vec::new();
    while let Some(t) = pending.pop() {
        budget.spend(1)?;
        match t {
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::Managed(_) => {
                budget.spend(1)?;
                leaves.push(t);
            }
            _ => {}
        }
    }
    Ok(leaves)
}
fn cleanup(
    module: &TypedModule,
    first: FunctionId,
    budget: &mut Budget,
    span: Option<Span>,
) -> Check<()> {
    let mut pending = vec![first];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.spend(seen.len().saturating_add(1))?;
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
                    resources::charge_payload(payload, budget)?;
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
                budget.spend(1)?;
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
    span: Option<Span>,
) -> Check<Vec<FunctionId>> {
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
            budget.spend(1)?;
            match t {
                Type::AsyncRegistration { .. } => {
                    return Err(reject(
                        "E-VERIFY-TASK",
                        span,
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
            && !matches!(local_type(f, *parameter, span)?, Type::Address { inner, .. } if retained_pointee(inner))
        {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "task initial frame cannot retain borrowed input storage",
            ));
        }
    }
    let mut checked = BTreeSet::new();
    let mut enum_frame = false;
    for t in f.locals.iter().map(|l| &l.ty).chain(&f.values) {
        // Preserve the exact former path for every already supported type.
        let new_enum = !pollable_storage(module, t, budget)?;
        if new_enum && !complete_enum(t, budget)? {
            return Err(reject(
                "E-VERIFY-TASK",
                span,
                "task owner needs a complete canonical inline resource frame slot",
            ));
        }
        let leaves = if new_enum {
            enum_frame = true;
            enum_resources(t, budget)?
        } else {
            match t {
                Type::Package { fields, .. } => fields.iter().map(|(_, t)| t).collect::<Vec<_>>(),
                Type::Tuple(fields) => fields.iter().collect(),
                _ => vec![t],
            }
        };
        for leaf in leaves {
            if let Type::Managed(payload) = leaf {
                for _ in &module.resources {
                    resources::charge_payload(payload, budget)?;
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
    if enum_frame && f.return_type != Type::I32 {
        return Err(unsupported(
            "complete Enum continuation slots currently require the existing i32 task result",
            span,
        ));
    }
    let mut children = vec![];
    for b in &f.blocks {
        for i in &b.instructions {
            budget.spend(1)?;
            match &i.kind {
                InstructionKind::PlaceTagLocal { local, path } if enum_frame => {
                    let ty = local_type(f, *local, Some(i.span))?;
                    resources::charge_payload(ty, budget)?;
                    if !path.is_empty() || !sums::owned_enum(ty) {
                        return Err(unsupported(
                            "Enum continuation tag view requires a complete owning Enum local and empty path",
                            Some(i.span),
                        ));
                    }
                    // Actual current-tag, initialization and loan permission
                    // remain obligations of the ordinary independent kernels.
                }
                InstructionKind::VariantTagLocal { .. }
                | InstructionKind::ReadVariantLocal { .. }
                | InstructionKind::UnpackVariant { .. }
                | InstructionKind::DropEnum { .. }
                    if enum_frame => {}
                InstructionKind::Call { function, .. } => {
                    cleanup(module, *function, budget, Some(i.span))?
                }
                InstructionKind::SuspendCall {
                    function,
                    arguments,
                } => {
                    budget.spend(arguments.len().saturating_add(1))?;
                    let target = module.functions.get(function.0 as usize).ok_or_else(|| {
                        reject("E-VERIFY-TASK", Some(i.span), "child target absent")
                    })?;
                    if arguments.len() != target.parameters.len() {
                        return Err(reject(
                            "E-VERIFY-TASK",
                            Some(i.span),
                            "child actual arity differs",
                        ));
                    }
                    for (arg, param) in arguments.iter().zip(&target.parameters) {
                        let actual = value_type(f, *arg, Some(i.span))?;
                        let wanted = local_type(target, *param, Some(i.span))?;
                        resources::charge_payload(actual, budget)?;
                        resources::charge_payload(wanted, budget)?;
                        if actual != wanted {
                            return Err(reject(
                                "E-VERIFY-TASK",
                                Some(i.span),
                                "child actual input type differs",
                            ));
                        }
                    }
                    children.push(*function);
                }
                InstructionKind::AsyncRegistrationStart { .. }
                | InstructionKind::AsyncRegistrationValid { .. }
                | InstructionKind::AsyncRegistrationRequestClose { .. }
                | InstructionKind::AsyncRegistrationPollClose { .. }
                | InstructionKind::AsyncRegistrationFinishDestroy { .. }
                | InstructionKind::AsyncRegistrationFinishDestroyChecked { .. }
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
                | InstructionKind::WriteAddress { .. }
                | InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::AddressField { .. }
                | InstructionKind::AddressIndex { .. }
                | InstructionKind::AddressSlice { .. }
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
