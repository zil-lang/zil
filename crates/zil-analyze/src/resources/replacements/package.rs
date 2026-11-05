//! Complete local Package endpoints. The single leaf carries provenance; the
//! original shell retains its cleanup obligation and initialization/order.
use super::*;

pub(super) fn field(ty: &Type, b: &mut Budget) -> CompileResult<Option<Vec<u32>>> {
    let Type::Package { fields, .. } = ty else {
        return Ok(None);
    };
    b.type_work(ty, true, "complete Package replacement qualification")?;
    fee(b, fields.len())?;
    let mut owners = Vec::new();
    for (index, (_, child)) in fields.iter().enumerate() {
        if let Type::Managed(payload) = child {
            if !matches!(payload.as_ref(), Type::Record { .. }) || !payload.is_copy_value() {
                return Ok(None);
            }
            owners.push(index as u32);
        } else if matches!(child, Type::Package { .. }) {
            if field(child, b)?.is_none() {
                return Ok(None);
            }
            owners.push(index as u32);
        } else if !child.is_b0_scalar() {
            return Ok(None);
        }
    }
    Ok((!owners.is_empty()).then_some(owners))
}
pub(super) fn leaves(ty: &Type, b: &mut Budget) -> CompileResult<Vec<Vec<u32>>> {
    let mut result = Vec::new();
    let mut pending = vec![(ty, Vec::new())];
    while let Some((ty, path)) = pending.pop() {
        fee(b, path.len().saturating_add(1))?;
        match ty {
            Type::Managed(_) => result.push(path),
            Type::Package { fields, .. } => {
                fee(b, fields.len())?;
                for (index, (_, child)) in fields.iter().enumerate().rev() {
                    if matches!(child, Type::Managed(_) | Type::Package { .. }) {
                        fee(b, path.len().saturating_add(1))?;
                        let mut next = path.clone();
                        next.push(index as u32);
                        pending.push((child, next));
                    }
                }
            }
            _ => {
                return Err(error(
                    "nested replacement leaf route crosses a nonowning tree",
                ));
            }
        }
    }
    Ok(result)
}
fn unsupported() -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(
        Phase::Analyze,
        "complete Package replacement requires this body's exact whole local owner",
        None,
    )]
}
pub(super) fn local(f: &TypedFunction, target: &Target, b: &mut Budget) -> CompileResult<LocalId> {
    fee(b, f.parameters.len())?;
    match target.root {
        origins::Root::Local(local)
            if target.path.is_empty() && target.mutable && !f.parameters.contains(&local) =>
        {
            Ok(local)
        }
        _ => Err(unsupported()),
    }
}
// Copy alone includes code/foreign carriers. This result profile permits only
// ordinary finite value trees, so it introduces no borrowed or owning output.
pub(super) fn copy_result(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    if *ty == Type::Unit {
        return Ok(true);
    }
    b.type_work(
        ty,
        true,
        "whole Package helper finite Copy result qualification",
    )?;
    fee(b, 1)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        fee(b, 1)?;
        match ty {
            ty if ty.is_b0_scalar() => {}
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                pending.extend(fields);
            }
            Type::Array { element, .. } => {
                fee(b, 1)?;
                pending.push(element);
            }
            Type::Record { fields, .. } => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|(_, child)| child));
            }
            Type::Enum { variants, .. } => {
                fee(b, variants.len())?;
                for (_, fields) in variants {
                    fee(b, fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}

pub(super) struct Admission {
    pub(super) owners: Vec<u32>,
    pub(super) legacy_must: bool,
}
pub(super) fn ordinary_result(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<bool> {
    if copy_result(ty, b)? {
        return Ok(true);
    }
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        fee(b, 1)?;
        match ty {
            Type::Managed(payload)
                if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value() => {}
            Type::Managed(_) if dynamic_store::is_owned_carrier(ty) => {
                dynamic_store::ordinary_owned(module, ty, b)?;
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                pending.extend(fields);
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|(_, field)| field));
            }
            Type::Enum { variants, .. } => {
                fee(b, variants.len())?;
                for (_, fields) in variants {
                    fee(b, fields.len())?;
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            Type::Array { element, .. } => {
                fee(b, 1)?;
                pending.push(element);
            }
            ty if ty.is_b0_scalar() => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}
// The borrowed shell stays the caller's responsibility. Admission binds the
// actual mutable parameter and complete endpoint, independently of the ticket.
pub(super) fn borrowed(
    module: &TypedModule,
    f: &TypedFunction,
    target: &Target,
    b: &mut Budget,
) -> CompileResult<Admission> {
    if f.foreign.is_some()
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.storage_life.is_some()
        || !target.path.is_empty()
        || !target.mutable
    {
        return Err(unsupported());
    }
    if !(if f.environment.is_some() {
        copy_result(&f.return_type, b)?
    } else {
        ordinary_result(module, &f.return_type, b)?
    }) {
        return Err(unsupported());
    }
    let (parameter, root) = match target.root {
        origins::Root::Parameter(parameter) => {
            let local = f
                .parameters
                .get(parameter as usize)
                .ok_or_else(unsupported)?;
            (Some(parameter), &f.locals[local.0 as usize].ty)
        }
        origins::Root::Capture(capture) => {
            let environment = f.environment.ok_or_else(unsupported)?;
            let closure = module
                .closures
                .get(environment.0 as usize)
                .ok_or_else(unsupported)?;
            let ty = closure
                .captures
                .get(capture as usize)
                .ok_or_else(unsupported)?;
            (None, ty)
        }
        origins::Root::StoreElement(value) => (None, &f.values[value.0 as usize]),
        _ => return Err(unsupported()),
    };
    let Type::Address {
        mutable: true,
        inner,
    } = root
    else {
        return Err(unsupported());
    };
    let owners = field(inner, b)?.ok_or_else(unsupported)?;
    let mut legacy_must = f.environment.is_none()
        && f.call_kind == CallKind::Direct
        && parameter.is_some()
        && owners.len() == 1
        && !product::nested(inner);
    fee(b, f.parameters.len())?;
    for (p, local) in f.parameters.iter().enumerate() {
        if parameter == Some(p as u32) {
            continue;
        }
        let ty = &f.locals[local.0 as usize].ty;
        b.type_work(ty, true, "borrowed whole Package companion qualification")?;
        let managed = matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Record { .. }) && payload.is_copy_value());
        let address =
            matches!(ty, Type::Address { inner: other, .. } if other.as_ref() == inner.as_ref());
        let store = matches!(ty, Type::Desc(payload) | Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { element } if element.as_ref() == inner.as_ref()));
        legacy_must &= !address && !store;
        if !(ty.is_b0_scalar() || managed || ty == inner.as_ref() || address || store) {
            return Err(unsupported());
        }
    }
    Ok(Admission {
        owners,
        legacy_must,
    })
}

// Only the proved normal-return Commit replaces provenance. It leaves the
// caller's existing shell/leaf life, cleanup age and order untouched.
pub(super) fn postcondition(
    flow: &Flow<'_>,
    state: &mut State,
    target: &Target,
    birth: &BTreeSet<Birth>,
    check: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    let origins::Root::Local(local) = target.root else {
        return Ok(());
    };
    let [PlaceProjection::Field(index)] = target.path.as_slice() else {
        return Err(unsupported());
    };
    if !field(&flow.function.locals[local.0 as usize].ty, b)?
        .is_some_and(|owners| owners.contains(index))
    {
        return Err(unsupported());
    }
    if check {
        complete(flow.function, state, local, b)?;
    }
    let place = Place::PackageField {
        local,
        index: *index,
    };
    let mut old = copied(state, place, b)?;
    fee(b, old.birth.len().saturating_add(birth.len()))?;
    old.birth = birth.clone();
    replace(state, place, old, b)?;
    state
        .sums
        .replace_package_birth(local, *index, birth, check, b)?;
    Ok(())
}

pub(super) fn complete(
    f: &TypedFunction,
    state: &State,
    local: LocalId,
    b: &mut Budget,
) -> CompileResult<Vec<u32>> {
    let owners = field(&f.locals[local.0 as usize].ty, b)?.ok_or_else(unsupported)?;
    if !exact(&copied(state, Place::Local(local), b)?, Life::Available) {
        return Err(error(
            "complete Package replacement needs its live old shell",
        ));
    }
    fee(b, owners.len().saturating_sub(1))?;
    for &index in &owners {
        if !exact(
            &copied(state, Place::PackageField { local, index }, b)?,
            Life::Available,
        ) {
            return Err(error(
                "complete Package replacement cannot repair a partial or dead old owner",
            ));
        }
    }
    state.sums.require_place(local, &[], b)?;
    Ok(owners)
}
pub(super) fn apply(
    flow: &Flow<'_>,
    state: &mut State,
    target: &Target,
    rhs: ValueId,
    point: CheckPoint<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    let local = local(flow.function, target, b)?;
    let owners = field(&flow.function.locals[local.0 as usize].ty, b)?.ok_or_else(unsupported)?;
    if point.validate {
        complete(flow.function, state, local, b)?;
        let live = point
            .live
            .ok_or_else(|| error("Package replacement lacks old-instance liveness"))?;
        // The parent Address<Package> stays valid. Only weak descriptions and
        // their concrete closure carriers may retain the retired resource.
        let mut descriptions = Live::new();
        fee(b, live.len())?;
        for place in live {
            let ty = match place {
                Place::Local(l) => &flow.function.locals[l.0 as usize].ty,
                Place::Value(v) => &flow.function.values[v.0 as usize],
                _ => continue,
            };
            if matches!(ty, Type::Desc(_) | Type::Closure(_))
                || matches!(ty, Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Closure(_)))
            {
                b.lookup(
                    descriptions.len(),
                    "Package old description carrier insertion",
                )?;
                descriptions.insert(*place);
            }
        }
        no_later_use(state, &descriptions, Source::Head(local), b)?;
    }
    if product::nested(&flow.function.locals[local.0 as usize].ty) {
        let ty = &flow.function.locals[local.0 as usize].ty;
        product::transfer(
            state,
            (product::Carrier::Value(rhs), &[]),
            (product::Carrier::Local(local), &[]),
            ty,
            b,
        )?;
        state.sums.replace_product(local, rhs, point.validate, b)?;
        return Ok(());
    }
    // The generic Package Fact is Incoming. Recover the actual *owned field*
    // of this fully prepared RHS instead of inventing a fresh leaf from it.
    fee(b, owners.len().saturating_sub(1))?;
    for index in owners {
        let incoming = product::birth(state, product::Carrier::Value(rhs), &[index], b)?;
        let place = Place::PackageField { local, index };
        let mut old = copied(state, place, b)?;
        fee(b, old.birth.len().saturating_add(incoming.len()))?;
        // This is a definite write to one actual local, unlike a may-write
        // summary. Retiring the old leaf does not retain its origin alternatives.
        old.birth = incoming.clone();
        replace(state, place, old, b)?;
        state
            .sums
            .replace_package_birth(local, index, &incoming, point.validate, b)?;
    }
    Ok(())
}

fn registered(module: &TypedModule, payload: &Type, b: &mut Budget) -> CompileResult<()> {
    let Type::Record { name, .. } = payload else {
        return Ok(());
    };
    fee(b, module.resources.len())?;
    for r in &module.resources {
        fee(b, name.len().saturating_add(r.name.len()))?;
        if *name == r.name {
            b.type_work(payload, false, "Package actual resource payload identity")?;
            b.type_work(
                &r.payload,
                false,
                "Package resource definition payload identity",
            )?;
            return if *payload == r.payload {
                Ok(())
            } else {
                Err(error(
                    "Package replacement resource head differs from its actual definition",
                ))
            };
        }
    }
    Err(error(
        "Package replacement resource head has no actual definition",
    ))
}
fn heads(module: &TypedModule, ty: &Type, b: &mut Budget) -> CompileResult<()> {
    fee(b, 1)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        fee(b, 1)?;
        match ty {
            Type::Managed(payload) | Type::Desc(payload) => {
                registered(module, payload, b)?;
                fee(b, 1)?;
                pending.push(payload);
            }
            Type::Address { inner, .. }
            | Type::Opaque { hidden: inner, .. }
            | Type::BorrowedStorage { storage: inner, .. }
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::OwnedStore { element: inner }
            | Type::Registration { environment: inner }
            | Type::AsyncRegistration { environment: inner }
            | Type::Task { result: inner, .. } => {
                fee(b, 1)?;
                pending.push(inner);
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fee(b, fields.len())?;
                pending.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                pending.extend(fields);
            }
            Type::Enum { variants, .. } => {
                fee(b, variants.len())?;
                for (_, fs) in variants {
                    fee(b, fs.len())?;
                    pending.extend(fs);
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                fee(b, parameters.len().saturating_add(1))?;
                pending.extend(parameters);
                pending.push(result);
            }
            _ => {}
        }
    }
    Ok(())
}
fn teardown(
    module: &TypedModule,
    r: &ResourceDefinition,
    b: &mut Budget,
) -> CompileResult<Option<FunctionId>> {
    let Some(id) = r.teardown else {
        return Ok(None);
    };
    let f = module
        .functions
        .get(id.0 as usize)
        .ok_or_else(|| error("Package resource teardown entry is absent"))?;
    fee(b, 1)?;
    if f.id != id
        || f.foreign.is_some()
        || f.environment.is_some()
        || f.call_kind != CallKind::Direct
        || f.callable_contract.is_some()
        || f.callable_return_contract.is_some()
        || f.reference_return_bound.is_some()
        || f.storage_life.is_some()
        || f.parameters.len() != 1
        || f.return_type != Type::Unit
        || f.result_mode != PassingMode::Infer
    {
        return Err(error(
            "Package resource teardown requires its exact direct Desc-to-Unit entry",
        ));
    }
    let ty = f
        .parameters
        .first()
        .and_then(|p| f.locals.get(p.0 as usize))
        .map(|l| &l.ty);
    let Some(Type::Desc(actual)) = ty else {
        return Err(error(
            "Package resource teardown requires its actual Desc parameter",
        ));
    };
    b.type_work(actual, false, "Package teardown actual payload")?;
    b.type_work(&r.payload, false, "Package teardown bound payload")?;
    if actual.as_ref() != &r.payload {
        return Err(error(
            "Package resource teardown belongs to a different nominal payload",
        ));
    }
    Ok(Some(id))
}

pub(super) fn definitions(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    fee(b, module.resources.len())?;
    let mut teardown_entries = BTreeSet::new();
    for (index, r) in module.resources.iter().enumerate() {
        if r.name.is_empty() {
            return Err(error("Package resource definition has no nominal identity"));
        }
        if let Type::Record { name, .. } = &r.payload {
            fee(b, name.len().saturating_add(r.name.len()))?;
            if *name != r.name {
                return Err(error(
                    "Package resource definition name differs from actual payload",
                ));
            }
        }
        if let Some(id) = teardown(module, r, b)? {
            b.lookup(
                teardown_entries.len(),
                "Package teardown identity insertion",
            )?;
            teardown_entries.insert(id);
        }
        fee(b, index)?;
        for previous in &module.resources[..index] {
            fee(b, r.name.len().saturating_add(previous.name.len()))?;
            if r.name == previous.name {
                return Err(error("Package resource definition identity is duplicated"));
            }
        }
    }
    fee(
        b,
        module.functions.len().saturating_add(module.closures.len()),
    )?;
    for f in &module.functions {
        fee(b, f.blocks.len())?;
        for block in &f.blocks {
            fee(b, block.instructions.len())?;
            for i in &block.instructions {
                if let InstructionKind::Call { function, .. }
                | InstructionKind::FunctionRef { function }
                | InstructionKind::ForeignFunctionRef { function } = i.kind
                {
                    b.lookup(
                        teardown_entries.len(),
                        "Package teardown ordinary target lookup",
                    )?;
                    if teardown_entries.contains(&function) {
                        return Err(error(
                            "resource teardown cannot be an ordinary call or function value",
                        ));
                    }
                }
            }
        }
        fee(
            b,
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(1),
        )?;
        for ty in f
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&f.values)
            .chain(std::iter::once(&f.return_type))
        {
            heads(module, ty, b)?;
        }
    }
    for d in &module.closures {
        b.lookup(
            teardown_entries.len(),
            "Package teardown closure target lookup",
        )?;
        if teardown_entries.contains(&d.function) {
            return Err(error("resource teardown cannot be a closure entry"));
        }
        fee(
            b,
            d.captures
                .len()
                .saturating_add(d.parameters.len())
                .saturating_add(1),
        )?;
        for ty in d
            .captures
            .iter()
            .chain(&d.parameters)
            .chain(std::iter::once(&d.result))
        {
            heads(module, ty, b)?;
        }
    }
    Ok(())
}
