//! Complete local Package endpoints. The single leaf carries provenance; the
//! original shell retains its cleanup obligation and initialization/order.
use super::*;

pub(super) fn field(ty: &Type, b: &mut Budget) -> CompileResult<Option<u32>> {
    let Type::Package { fields, .. } = ty else {
        return Ok(None);
    };
    b.type_work(ty, true, "complete Package replacement qualification")?;
    fee(b, fields.len())?;
    let mut owner = None;
    for (index, (_, child)) in fields.iter().enumerate() {
        if let Type::Managed(payload) = child {
            if owner.is_some()
                || !matches!(payload.as_ref(), Type::Record { .. })
                || !payload.is_copy_value()
            {
                return Ok(None);
            }
            owner = Some(index as u32);
        } else if !child.is_b0_scalar() {
            return Ok(None);
        }
    }
    Ok(owner)
}
fn unsupported() -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(
        Phase::Analyze,
        "complete Package replacement requires this body's exact whole local owner",
        None,
    )]
}
pub(super) fn local(target: &Target) -> CompileResult<LocalId> {
    match target.root {
        origins::Root::Local(local) if target.path.is_empty() && target.mutable => Ok(local),
        _ => Err(unsupported()),
    }
}
pub(super) fn complete(
    f: &TypedFunction,
    state: &State,
    local: LocalId,
    b: &mut Budget,
) -> CompileResult<u32> {
    let index = field(&f.locals[local.0 as usize].ty, b)?.ok_or_else(unsupported)?;
    if !exact(&copied(state, Place::Local(local), b)?, Life::Available)
        || !exact(
            &copied(state, Place::PackageField { local, index }, b)?,
            Life::Available,
        )
    {
        return Err(error(
            "complete Package replacement cannot repair a partial or dead old owner",
        ));
    }
    state.sums.require_place(local, &[], b)?;
    Ok(index)
}
pub(super) fn apply(
    flow: &Flow<'_>,
    state: &mut State,
    target: &Target,
    rhs: ValueId,
    point: CheckPoint<'_>,
    b: &mut Budget,
) -> CompileResult<()> {
    let local = local(target)?;
    let index = field(&flow.function.locals[local.0 as usize].ty, b)?.ok_or_else(unsupported)?;
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
    // The generic Package Fact is Incoming. Recover the actual *owned field*
    // of this fully prepared RHS instead of inventing a fresh leaf from it.
    let incoming = product::birth(state, product::Carrier::Value(rhs), &[index], b)?;
    let place = Place::PackageField { local, index };
    let mut old = copied(state, place, b)?;
    fee(b, old.birth.len().saturating_add(incoming.len()))?;
    old.birth.extend(&incoming);
    replace(state, place, old, b)?;
    state.sums.replace_birth(
        local,
        &[PlaceProjection::Field(index)],
        &incoming,
        point.validate,
        b,
    )?;
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
pub(super) fn definitions(module: &TypedModule, b: &mut Budget) -> CompileResult<()> {
    fee(b, module.resources.len())?;
    for (index, r) in module.resources.iter().enumerate() {
        if let Type::Record { name, .. } = &r.payload {
            fee(b, name.len().saturating_add(r.name.len()))?;
            if *name != r.name {
                return Err(error(
                    "Package resource definition name differs from actual payload",
                ));
            }
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
