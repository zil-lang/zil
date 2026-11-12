//! Actual opaque Task positions. Capture selectors stay in the metadata domain.
use super::*;

pub(super) fn ports<'a>(
    module: &'a TypedModule,
    ty: &'a Type,
    b: &mut Budget,
) -> CompileResult<Vec<(Vec<ContractProjection>, u32, &'a zil_ir::RetainedBorrowPort)>> {
    let mut out = Vec::new();
    let mut todo = vec![(ty, Vec::<ContractProjection>::new(), 0usize)];
    while let Some((ty, path, depth)) = todo.pop() {
        fee(b, path.len().saturating_add(1))?;
        if depth > b.partial_depth {
            return Err(failure("Task carrier traversal exceeds finite depth"));
        }
        match ty {
            Type::Task { retained_life, .. } => {
                fee(
                    b,
                    retained_life
                        .ports
                        .len()
                        .saturating_mul(path.len().saturating_add(2)),
                )?;
                for (port, row) in retained_life.ports.iter().enumerate() {
                    out.push((path.clone(), port as u32, row));
                }
            }
            Type::Managed(inner)
            | Type::Address { inner, .. }
            | Type::Desc(inner)
            | Type::BorrowedStorage { storage: inner, .. } => todo.push((inner, path, depth + 1)),
            Type::Closure(id) => {
                let d = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| failure("Task carrier closure absent"))?;
                fee(
                    b,
                    d.captures
                        .len()
                        .saturating_mul(path.len().saturating_add(2)),
                )?;
                for (index, ty) in d.captures.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(ContractProjection::Capture(index as u32));
                    todo.push((ty, p, depth + 1));
                }
            }
            Type::Tuple(fields) => {
                fee(b, fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (index, ty) in fields.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    todo.push((ty, p, depth + 1));
                }
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                fee(b, fields.len().saturating_mul(path.len().saturating_add(2)))?;
                for (index, (_, ty)) in fields.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    todo.push((ty, p, depth + 1));
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, path.len().saturating_add(2).saturating_mul(2))?;
                for (arm, ty) in [(true, error.as_ref()), (false, result.as_ref())] {
                    let mut p = path.clone();
                    p.push(ContractProjection::Place(PlaceProjection::ErrorArm(arm)));
                    todo.push((ty, p, depth + 1));
                }
            }
            _ => {}
        }
    }
    fee(
        b,
        out.len()
            .saturating_mul(out.len().checked_ilog2().map_or(1, |n| n as usize + 1))
            .saturating_mul(b.partial_depth.saturating_add(1)),
    )?;
    out.sort_by(|a, z| a.0.cmp(&z.0).then(a.1.cmp(&z.1)));
    Ok(out)
}
fn fixed(
    module: &TypedModule,
    id: zil_ir::ClosureId,
    path: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<u32> {
    fee(b, path.len().saturating_add(1))?;
    if path.len() > b.partial_depth {
        return Err(failure("Task captured class exceeds finite depth"));
    }
    let d = module
        .closures
        .get(id.0 as usize)
        .ok_or_else(|| failure("Task captured class definition absent"))?;
    let Some((ContractProjection::Capture(index), tail)) = path.split_first() else {
        return Err(failure("Task captured port omits actual Capture selector"));
    };
    let ty = d
        .captures
        .get(*index as usize)
        .ok_or_else(|| failure("Task captured port capture absent"))?;
    let c = d
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("Task captured class domain is Unknown"))?;
    fee(b, c.bindings.len().saturating_add(c.classes.len()))?;
    let binder = match ty {
        Type::Managed(payload) if matches!(payload.as_ref(), Type::Task { .. }) => {
            let Type::Task { retained_life, .. } = payload.as_ref() else {
                unreachable!()
            };
            let [ContractProjection::RetainedPort(port)] = tail else {
                return Err(failure("captured Task port path is not canonical"));
            };
            let row = retained_life
                .ports
                .get(*port as usize)
                .ok_or_else(|| failure("captured Task port absent"))?;
            let binding = c
                .bindings
                .iter()
                .find(|binding| {
                    binding.position.base == StorageBase::Capture(*index)
                        && binding.position.path == tail
                })
                .ok_or_else(|| failure("Task capture omits actual port class"))?;
            let class = c
                .classes
                .iter()
                .find(|class| {
                    class.position.base == StorageBase::Capture(*index)
                        && class.position.path == [zil_ir::LifeTypeProjection::Pointee]
                        && class.position.class == row.binder
                })
                .ok_or_else(|| failure("Task capture omits complete class map"))?;
            if binding.binder != class.binder {
                return Err(failure("Task capture port and class differ"));
            }
            binding.binder
        }
        Type::Closure(child) => {
            let child = fixed(module, *child, tail, b)?;
            c.classes
                .iter()
                .find(|row| {
                    row.position.base == StorageBase::Capture(*index)
                        && row.position.path.is_empty()
                        && row.position.class == child
                })
                .map(|row| row.binder)
                .ok_or_else(|| failure("nested Task capture class map absent"))?
        }
        _ => {
            return Err(failure(
                "Task captured class endpoint has no actual Task carrier",
            ));
        }
    };
    match binder {
        StorageBinder::Fixed(index) => Ok(index),
        _ => Err(failure("captured Task cannot retain an invocation class")),
    }
}
pub(super) fn relation(
    module: &TypedModule,
    f: &TypedFunction,
    base: StorageBase,
    path: &[ContractProjection],
    b: &mut Budget,
) -> CompileResult<StorageBinder> {
    let ty = match base {
        StorageBase::Receiver => {
            return Err(failure(
                "Task receiver source requires an actual retained head consumer",
            ));
        }
        StorageBase::Result => &f.return_type,
        StorageBase::Parameter(index) => {
            &f.locals[f
                .parameters
                .get(index as usize)
                .ok_or_else(|| failure("Task relation parameter absent"))?
                .0 as usize]
                .ty
        }
        StorageBase::Capture(index) => f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .and_then(|d| d.captures.get(index as usize))
            .ok_or_else(|| failure("Task captured input is absent"))?,
    };
    let (ty, pointee) = if let Type::Address { inner, .. } = ty {
        (inner.as_ref(), true)
    } else {
        (ty, false)
    };
    let c = f
        .storage_life
        .as_ref()
        .ok_or_else(|| failure("Task relation domain is Unknown"))?;
    if let Type::Closure(id) = ty {
        let class = fixed(module, *id, path, b)?;
        fee(b, c.classes.len())?;
        c.classes
            .iter()
            .find(|row| {
                row.position.base == base
                    && row.position.class == class
                    && if pointee {
                        row.position.path == [zil_ir::LifeTypeProjection::Pointee]
                    } else {
                        row.position.path.is_empty()
                    }
            })
            .map(|row| row.binder)
            .ok_or_else(|| failure("Task closure boundary omits actual Fixed class map"))
    } else {
        fee(
            b,
            c.bindings
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        c.bindings
            .iter()
            .find(|row| row.position.base == base && row.position.path == path)
            .map(|row| row.binder)
            .ok_or_else(|| failure("Task boundary omits complete actual retained port"))
    }
}
