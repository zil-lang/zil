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
    let local = local(flow.function, target, b)?;
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
