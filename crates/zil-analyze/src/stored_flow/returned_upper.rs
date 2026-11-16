//! Discharge the mutable factory boundary from converged actual memory, after
//! the separate claim/type checks. An advertised lifetime supplies no target.
use super::*;

fn project<'a>(
    module: &'a TypedModule,
    mut ty: &'a Type,
    step: PlaceProjection,
    b: &mut Budget,
) -> CompileResult<(&'a Type, ContractProjection)> {
    fee(b, 1)?;
    if let Type::Address { inner, .. } = ty {
        ty = inner;
    }
    if let Type::BorrowedStorage { storage, .. } = ty {
        ty = storage;
    }
    if let Type::Closure(id) = ty {
        let PlaceProjection::Field(index) = step else {
            return Err(failure("terminal source capture is not a field"));
        };
        let child = module
            .closures
            .get(id.0 as usize)
            .and_then(|d| d.captures.get(index as usize))
            .ok_or_else(|| failure("terminal source capture absent"))?;
        return Ok((child, ContractProjection::Capture(index)));
    }
    Ok((slot(ty, &[step], b)?, ContractProjection::Place(step)))
}

fn input(
    module: &TypedModule,
    f: &TypedFunction,
    target: &Target,
    b: &mut Budget,
) -> CompileResult<StoragePosition> {
    let HoldRoot::Parameter(index) = target.base else {
        return Err(failure("terminal capture does not retain an actual input"));
    };
    if target.heap_element.is_some() || !target.heap_parents.is_empty() {
        return Err(failure("terminal capture source needs its explicit actual heap selector"));
    }
    if let Some(port) = &target.retained_port {
        let (carrier, port, task) = match port {
            PortOrigin::Dyn { carrier, port } => (carrier, *port, false),
            PortOrigin::Task { carrier, port } => (carrier, *port, true),
            PortOrigin::MutexPayload { .. } => return Err(failure("terminal source needs its actual Mutex payload relation")),
        };
        let parameter = f.parameters.get(index as usize)
            .and_then(|id| f.locals.get(id.0 as usize))
            .ok_or_else(|| failure("terminal opaque input has no actual parameter"))?;
        let ty = crate::retained::source_leaf(module, &parameter.ty, carrier, b)?;
        let ty = match ty { Type::Address { inner, .. } => inner.as_ref(), ty => ty };
        let ty = match ty { Type::BorrowedStorage { storage, .. } => storage.as_ref(), ty => ty };
        let valid = if task {
            matches!(ty, Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }))
        } else { matches!(ty, Type::OwnedDyn { .. } | Type::Dyn { .. }) };
        fee(b, carrier.len().saturating_add(target_width(target)))?;
        let physical = carrier.iter().take_while(|step| matches!(step, ContractProjection::Place(_)))
            .map(|step| { let ContractProjection::Place(step) = step else { unreachable!() }; *step })
            .collect::<Vec<_>>();
        if !valid || target.stored != physical || !target.place.is_empty() {
            return Err(failure("terminal opaque source changes its actual carrier namespace or physical prefix"));
        }
        let mut path = carrier.clone();
        path.push(ContractProjection::RetainedPort(port));
        crate::retained::source_leaf(module, &parameter.ty, &path, b)?;
        return Ok(StoragePosition { base: StorageBase::Parameter(index), path });
    }
    let mut ty = &f.locals[f
        .parameters
        .get(index as usize)
        .ok_or_else(|| failure("terminal source input absent"))?
        .0 as usize]
        .ty;
    fee(b, target_width(target))?;
    let mut path = Vec::new();
    for step in target.stored.iter().chain(&target.place) {
        let (child, projection) = project(module, ty, *step, b)?;
        ty = child;
        path.push(projection);
    }
    Ok(StoragePosition {
        base: StorageBase::Parameter(index),
        path,
    })
}

pub(super) fn check(
    module: &TypedModule,
    f: &TypedFunction,
    facts: &Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    if b.pending_returned_source_proofs.is_empty() {
        return Ok(());
    }
    ordered(b, b.pending_returned_source_proofs.len(), 0)?;
    if !b.pending_returned_source_proofs.contains(&f.id) {
        return Ok(());
    }
    let c = f
        .callable_return_contract
        .as_ref()
        .ok_or_else(|| failure("pending source factory contract absent"))?;
    let proposed = c
        .capture_sources
        .as_ref()
        .ok_or_else(|| failure("terminal capture sources unknown"))?;
    for row in proposed {
        let StorageBase::Capture(index) = row.position.base else {
            return Err(failure("terminal source destination is not a capture"));
        };
        let mut logical = vec![ContractProjection::Capture(index)];
        logical.extend_from_slice(&row.position.path);
        let opaque = row.position.path.iter().any(|step| matches!(step, ContractProjection::RetainedPort(_)));
        let leaf = crate::retained::source_leaf(module, &f.return_type, &logical, b)?;
        let mut declared_static = None;
        for atom in &row.sources {
            if let StorageSource::Static { leaf: upper, mode } = atom {
                let target = dyn_return::static_target(upper, *mode, b)?;
                b.type_work(leaf, false, "terminal Static upper complete leaf")?;
                if upper != leaf { return Err(failure("terminal Static upper changes its complete captured leaf")); }
                declared_static = Some(target);
            }
        }
        let mut physical = vec![PlaceProjection::Field(index)];
        fee(b, row.position.path.len().saturating_add(1))?;
        for step in &row.position.path {
            if opaque { break; }
            physical.push(match step {
                ContractProjection::Capture(index) => PlaceProjection::Field(*index),
                ContractProjection::Place(part) => *part,
                _ => {
                    return Err(failure(
                        "terminal capture path has no supported actual selector",
                    ));
                }
            });
        }
        let actual = if opaque {
            ordered(b, facts.returned_dyn_ports.len(), logical.len())?;
            facts.returned_dyn_ports.get(&logical)
                .ok_or_else(|| failure("terminal opaque capture has no completed actual port row"))?
        } else {
            ordered(b, facts.returned_environment.len(), physical.len())?;
            facts.returned_environment.get(&physical)
                .ok_or_else(|| failure("terminal captured leaf has no actual memory row"))?
        };
        if !facts.complete || actual.is_empty() && !opaque {
            return Err(failure("terminal captured source has no complete grounded actual proof"));
        }
        // An explicit empty opaque row is a checked unused upper. A missing
        // row above is Unknown, never an empty witness.
        for target in actual {
            if target.base == HoldRoot::Static {
                let Some(upper) = &declared_static else {
                    return Err(failure("actual terminal Static source is absent from its complete declared upper"));
                };
                fee(b, target_width(target).saturating_add(target_width(upper)))?;
                if target != upper {
                    return Err(failure("terminal Static capture hides an actual nonstatic selector"));
                }
                continue;
            }
            let source = input(module, f, target, b)?;
            fee(
                b,
                row.sources
                    .len()
                    .saturating_mul(source.path.len().saturating_add(1)),
            )?;
            let mut covered = false;
            for atom in &row.sources {
                if matches!(atom, StorageSource::Static { .. }) { continue; }
                covered |= crate::exact_storage_source(atom)? == &source;
            }
            if !covered {
                return Err(failure("actual terminal capture source exceeds its declared upper"));
            }
        }
    }
    ordered(b, b.pending_returned_source_proofs.len(), 0)?;
    b.pending_returned_source_proofs.remove(&f.id);
    Ok(())
}
