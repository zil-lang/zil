//! Verify reconstructs an opaque Task selector from the actual module tree.
//! It keeps Capture paths separate from physical addresses and class ordinals.
use super::*;

fn unwrap(ty: &Type) -> &Type {
    match ty {
        Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => inner,
        _ => ty,
    }
}
pub(super) fn endpoint<'a>(
    module: &'a TypedModule,
    root: &'a Type,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<&'a Type> {
    b.spend(path.len().saturating_add(1))?;
    if path.len() > b.limits.max_call_depth {
        return Err(limit("Task metadata carrier path exceeds depth"));
    }
    let mut current = root;
    for step in path {
        while let Type::Managed(inner)
        | Type::Desc(inner)
        | Type::Address { inner, .. }
        | Type::BorrowedStorage { storage: inner, .. } = current
        {
            b.spend(1)?;
            current = inner;
        }
        current = match (current, step) {
            (Type::Closure(id), ContractProjection::Capture(index)) => module
                .closures
                .get(id.0 as usize)
                .and_then(|d| d.captures.get(*index as usize))
                .ok_or_else(|| err("Task carrier capture is absent"))?,
            (Type::Tuple(fields), ContractProjection::Place(PlaceProjection::Field(index))) => {
                fields
                    .get(*index as usize)
                    .ok_or_else(|| err("Task tuple carrier field absent"))?
            }
            (
                Type::Package { fields, .. } | Type::Record { fields, .. },
                ContractProjection::Place(PlaceProjection::Field(index)),
            ) => {
                &fields
                    .get(*index as usize)
                    .ok_or_else(|| err("Task Product carrier field absent"))?
                    .1
            }
            (
                Type::ErrorUnion { error, result },
                ContractProjection::Place(PlaceProjection::ErrorArm(arm)),
            ) => {
                if *arm {
                    error
                } else {
                    result
                }
            }
            _ => {
                return Err(err(
                    "Task opaque selector does not follow its actual carrier",
                ));
            }
        };
    }
    while let Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } = current {
        b.spend(1)?;
        current = inner;
    }
    Ok(current)
}
pub(super) fn paths(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> Check<Vec<(Vec<ContractProjection>, u32, StoredBorrowMode)>> {
    let mut out = Vec::new();
    let mut pending = vec![(ty, Vec::new())];
    while let Some((ty, path)) = pending.pop() {
        b.spend(path.len().saturating_add(1))?;
        if path.len() > b.limits.max_call_depth {
            return Err(limit("Task owning carrier graph exceeds depth"));
        }
        let current = unwrap(ty);
        match current {
            Type::Task { retained_life, .. } => {
                b.spend(
                    retained_life
                        .ports
                        .len()
                        .saturating_mul(path.len().saturating_add(1)),
                )?;
                for (index, row) in retained_life.ports.iter().enumerate() {
                    out.push((path.clone(), index as u32, row.mode));
                }
            }
            Type::Closure(id) => {
                let actual = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| err("Task carrier closure definition absent"))?;
                for (index, ty) in actual.captures.iter().enumerate() {
                    b.spend(path.len().saturating_add(2))?;
                    let mut child = path.clone();
                    child.push(ContractProjection::Capture(index as u32));
                    pending.push((ty, child));
                }
            }
            Type::BorrowedStorage { storage, .. } => pending.push((storage, path)),
            Type::Managed(inner) | Type::Address { inner, .. } | Type::Desc(inner) => {
                pending.push((inner, path))
            }
            Type::Tuple(fields) => {
                for (index, ty) in fields.iter().enumerate() {
                    b.spend(path.len().saturating_add(2))?;
                    let mut child = path.clone();
                    child.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    pending.push((ty, child));
                }
            }
            Type::Package { fields, .. } | Type::Record { fields, .. } => {
                for (index, (_, ty)) in fields.iter().enumerate() {
                    b.spend(path.len().saturating_add(2))?;
                    let mut child = path.clone();
                    child.push(ContractProjection::Place(PlaceProjection::Field(
                        index as u32,
                    )));
                    pending.push((ty, child));
                }
            }
            Type::ErrorUnion { error, result } => {
                for (arm, ty) in [(true, error.as_ref()), (false, result.as_ref())] {
                    b.spend(path.len().saturating_add(2))?;
                    let mut child = path.clone();
                    child.push(ContractProjection::Place(PlaceProjection::ErrorArm(arm)));
                    pending.push((ty, child));
                }
            }
            _ => {}
        }
    }
    Ok(out)
}
pub(super) fn relation(
    module: &TypedModule,
    f: &TypedFunction,
    base: StorageBase,
    path: &[ContractProjection],
    b: &mut Budget,
) -> Check<StorageBinder> {
    let root = match base {
        StorageBase::Receiver => {
            return Err(err(
                "Task receiver source requires its actual head and retained-port consumer",
            ));
        }
        StorageBase::Result => &f.return_type,
        StorageBase::Parameter(index) => {
            &f.locals[f
                .parameters
                .get(index as usize)
                .ok_or_else(|| err("Task boundary parameter absent"))?
                .0 as usize]
                .ty
        }
        StorageBase::Capture(index) => f
            .environment
            .and_then(|id| module.closures.get(id.0 as usize))
            .and_then(|d| d.captures.get(index as usize))
            .ok_or_else(|| err("Task captured input is absent"))?,
    };
    let header = f
        .storage_life
        .as_ref()
        .ok_or_else(|| err("Task boundary class domain is Unknown"))?;
    let (root, pointed) = if let Type::Address { inner, .. } = root {
        (inner.as_ref(), true)
    } else {
        (root, false)
    };
    if !matches!(root, Type::Closure(_)) {
        b.spend(
            header
                .bindings
                .len()
                .saturating_mul(path.len().saturating_add(1)),
        )?;
        return header
            .bindings
            .iter()
            .find(|row| row.position.base == base && row.position.path == path)
            .map(|row| row.binder)
            .ok_or_else(|| err("Task boundary omits actual retained position"));
    }
    let mut current = root;
    let mut ancestors = Vec::new();
    let mut remaining = path;
    while let Some((ContractProjection::Capture(index), tail)) = remaining.split_first() {
        b.spend(remaining.len().saturating_add(1))?;
        let Type::Closure(id) = current else {
            return Err(err("Task class Capture selector has non-Closure endpoint"));
        };
        let d = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| err("Task class capture entry absent"))?;
        let child = d
            .captures
            .get(*index as usize)
            .ok_or_else(|| err("Task class capture index absent"))?;
        ancestors.push((d, *index));
        current = child;
        remaining = tail;
    }
    let [ContractProjection::RetainedPort(port)] = remaining else {
        return Err(err("Task captured boundary omits complete port selector"));
    };
    let Type::Task { retained_life, .. } = unwrap(current) else {
        return Err(err("Task class port selector endpoint is not a Task"));
    };
    b.spend(retained_life.ports.len())?;
    let mut class = retained_life
        .ports
        .get(*port as usize)
        .ok_or_else(|| err("Task captured port absent"))?
        .binder;
    let mut first = true;
    for (d, capture) in ancestors.into_iter().rev() {
        let life = d
            .storage_life
            .as_ref()
            .ok_or_else(|| err("Task captured class scope is Unknown"))?;
        b.spend(life.classes.len().saturating_add(life.bindings.len()))?;
        let expected = if first {
            vec![zil_ir::LifeTypeProjection::Pointee]
        } else {
            Vec::new()
        };
        let binder = life
            .classes
            .iter()
            .find(|row| {
                row.position.base == StorageBase::Capture(capture)
                    && row.position.path == expected
                    && row.position.class == class
            })
            .map(|row| row.binder)
            .ok_or_else(|| err("Task captured class lacks complete explicit map"))?;
        if first
            && !life.bindings.iter().any(|row| {
                row.position.base == StorageBase::Capture(capture)
                    && row.position.path == remaining
                    && row.binder == binder
            })
        {
            return Err(err(
                "Task captured data port disagrees with actual class map",
            ));
        }
        let StorageBinder::Fixed(index) = binder else {
            return Err(err("captured Task cannot use invocation-local scope"));
        };
        class = index;
        first = false;
    }
    b.spend(header.classes.len())?;
    header
        .classes
        .iter()
        .find(|row| {
            row.position.base == base
                && row.position.class == class
                && if pointed {
                    row.position.path == [zil_ir::LifeTypeProjection::Pointee]
                } else {
                    row.position.path.is_empty()
                }
        })
        .map(|row| row.binder)
        .ok_or_else(|| err("Task callable boundary omits complete Fixed substitution"))
}
