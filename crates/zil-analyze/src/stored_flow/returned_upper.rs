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
    if target.retained_port.is_some() || target.heap_element.is_some() {
        return Err(failure(
            "terminal capture source needs an explicit opaque or heap selector",
        ));
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
        let mut physical = vec![PlaceProjection::Field(index)];
        fee(b, row.position.path.len().saturating_add(1))?;
        for step in &row.position.path {
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
        ordered(b, facts.returned_environment.len(), physical.len())?;
        let actual = facts
            .returned_environment
            .get(&physical)
            .ok_or_else(|| failure("terminal captured leaf has no actual memory row"))?;
        if actual.is_empty() {
            return Err(failure("terminal captured leaf has no grounded input"));
        }
        for target in actual {
            let source = input(module, f, target, b)?;
            fee(
                b,
                row.sources
                    .len()
                    .saturating_mul(source.path.len().saturating_add(1)),
            )?;
            if !crate::exact_storage_sources_contain(&row.sources, &source)? {
                return Err(failure(
                    "actual terminal capture source exceeds its declared upper",
                ));
            }
        }
    }
    ordered(b, b.pending_returned_source_proofs.len(), 0)?;
    b.pending_returned_source_proofs.remove(&f.id);
    Ok(())
}
