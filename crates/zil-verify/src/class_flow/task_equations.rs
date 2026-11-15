//! Task-local equations over complete child domains. No target or loan is
//! inferred from a class equation, a port ordinal, or equal outside images.
use super::*;

fn key(ports: &Ports, path: &[LifeTypeProjection], class: u32, b: &mut Budget) -> Check<usize> {
    b.spend(
        ports
            .keys
            .len()
            .saturating_mul(path.len().saturating_add(1)),
    )?;
    let mut rows = ports
        .keys
        .iter()
        .enumerate()
        .filter(|(_, (p, c))| p == path && *c == class);
    let (index, _) = rows
        .next()
        .ok_or_else(|| bad("Task equation has no exact complete class occurrence"))?;
    if rows.next().is_some() {
        return Err(bad("Task equation repeats a complete class occurrence"));
    }
    ports
        .classes
        .get(index)
        .copied()
        .ok_or_else(|| bad("Task equation class variable is absent"))
}

fn task_path(ty: &Type, b: &mut Budget) -> Check<Vec<LifeTypeProjection>> {
    let mut current = ty;
    let mut path = Vec::new();
    loop {
        b.spend(1)?;
        if path.len() > b.limits.max_call_depth.min(128) {
            return Err(limit("Task class path depth exceeded"));
        }
        match current {
            Type::Task { .. } => return Ok(path),
            Type::ErrorUnion { result, .. } => {
                path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)));
                current = result;
            }
            Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => {
                path.push(LifeTypeProjection::Pointee);
                current = inner;
            }
            _ => return Err(bad("Task class path crosses another complete boundary")),
        }
    }
}

fn task_variable(ty: &Type, ports: &Ports, class: u32, b: &mut Budget) -> Check<usize> {
    let mut current = ty;
    let mut prefix = Vec::new();
    loop {
        b.spend(prefix.len().saturating_add(1))?;
        if prefix.len() > b.limits.max_call_depth.min(128) {
            return Err(limit("Task class boundary depth exceeded"));
        }
        match current {
            Type::Task { .. } => return key(ports, &prefix, class, b),
            Type::BorrowedStorage { storage, life } => {
                let path = task_path(storage, b)?;
                if path.is_empty() {
                    return key(ports, &prefix, class, b);
                }
                b.spend(
                    life.retained
                        .len()
                        .saturating_mul(path.len().saturating_add(1)),
                )?;
                let mut rows = life
                    .retained
                    .iter()
                    .filter(|row| row.path == path && row.class == class);
                let row = rows
                    .next()
                    .ok_or_else(|| bad("Task full boundary omits a class equation"))?;
                if rows.next().is_some() {
                    return Err(bad("Task full boundary repeats a class equation"));
                }
                return key(ports, &prefix, row.binder, b);
            }
            Type::ErrorUnion { result, .. } => {
                prefix.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)));
                current = result;
            }
            Type::Managed(inner) | Type::Desc(inner) | Type::Address { inner, .. } => {
                prefix.push(LifeTypeProjection::Pointee);
                current = inner;
            }
            _ => return Err(bad("Task equation has another actual carrier")),
        }
    }
}

fn whole_input(
    module: &TypedModule,
    ty: &Type,
    ports: &Ports,
    class: u32,
    b: &mut Budget,
) -> Check<usize> {
    let life = crate::tasks::frame_shape::declared_life(module, ty, b)?;
    if class >= life.binder_count {
        return Err(bad(
            "Task input equation selects outside its complete domain",
        ));
    }
    let mut current = ty;
    let mut path = Vec::new();
    let mut class = class;
    loop {
        b.spend(path.len().saturating_add(1))?;
        if path.len() > b.limits.max_call_depth.min(128) {
            return Err(limit("Task whole input equation depth exceeded"));
        }
        match current {
            Type::Address { inner, .. } => {
                if class == 0 && path.is_empty() {
                    return ports
                        .outer
                        .ok_or_else(|| bad("Task input outer occurrence is absent"));
                }
                class = class
                    .checked_sub(1)
                    .ok_or_else(|| bad("Task nested outer input lacks a complete boundary"))?;
                path.push(LifeTypeProjection::Pointee);
                current = inner;
            }
            Type::Managed(inner) => {
                path.push(LifeTypeProjection::Pointee);
                current = inner;
            }
            Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Task { .. }
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. } => return key(ports, &path, class, b),
            _ => {
                return Err(bad(
                    "Task whole input has no complete invariant class consumer",
                ));
            }
        }
    }
}

pub(super) fn create(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    arguments: &[ValueId],
    values: &[Ports],
    output: &Ports,
    e: &mut Equations,
    b: &mut Budget,
) -> Check<()> {
    let ty = i
        .result
        .and_then(|id| f.values.get(id.0 as usize))
        .ok_or_else(|| bad("Task constructor class result is absent"))?;
    let declaration = crate::semantic::task_domains::constructor_declaration(ty, b)?;
    let Type::Task { retained_life, .. } = declaration else {
        return Err(bad("Task constructor class has another declaration"));
    };
    for row in &retained_life.retained {
        let [LifeTypeProjection::Parameter(parameter)] = row.path.as_slice() else {
            continue;
        };
        b.spend(1)?;
        let argument = *arguments
            .get(*parameter as usize)
            .ok_or_else(|| bad("Task whole class input ordinal is absent"))?;
        let actual = whole_input(
            module,
            &f.values[argument.0 as usize],
            &values[argument.0 as usize],
            row.class,
            b,
        )?;
        e.equal(actual, task_variable(ty, output, row.binder, b)?, b)?;
    }
    if matches!(i.kind, InstructionKind::TaskCreateOwned { .. }) {
        let Type::ErrorUnion { error, .. } = ty else {
            return Err(bad("Task owned class outcome lacks its full error arm"));
        };
        if let Type::BorrowedStorage { life, .. } = error.as_ref() {
            for class in 0..retained_life.binder_count {
                b.spend(life.retained.len())?;
                let mut rows = life.retained.iter().filter(|row| {
                    row.path == [LifeTypeProjection::TypeArgument(1)] && row.class == class
                });
                let row = rows
                    .next()
                    .ok_or_else(|| bad("Task failure phantom Context omits a whole input class"))?;
                if rows.next().is_some() {
                    return Err(bad(
                        "Task failure phantom Context repeats a whole input class",
                    ));
                }
                let failed = key(
                    output,
                    &[LifeTypeProjection::Place(PlaceProjection::ErrorArm(true))],
                    row.binder,
                    b,
                )?;
                e.equal(failed, task_variable(ty, output, class, b)?, b)?;
            }
        } else if retained_life.binder_count != 0 {
            return Err(bad("Task failure erases its complete input Context domain"));
        }
    }
    Ok(())
}

pub(super) fn poll(
    f: &TypedFunction,
    i: &Instruction,
    task: ValueId,
    values: &[Ports],
    output: &Ports,
    e: &mut Equations,
    b: &mut Budget,
) -> Check<()> {
    let ty = &f.values[task.0 as usize];
    let Some(Type::Task { retained_life, .. }) = crate::tasks::task_payload(ty) else {
        return Err(bad("Task poll class input has another carrier"));
    };
    let actual_output = i
        .result
        .and_then(|id| f.values.get(id.0 as usize))
        .ok_or_else(|| bad("Task poll class output is absent"))?;
    for row in &retained_life.retained {
        if row.path != [LifeTypeProjection::Result] {
            continue;
        }
        let local = match actual_output {
            Type::ErrorUnion { .. } => key(
                output,
                &[LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))],
                row.class,
                b,
            )?,
            Type::BorrowedStorage { life, .. } => {
                b.spend(life.retained.len())?;
                let mut rows = life.retained.iter().filter(|r| {
                    r.path == [LifeTypeProjection::Place(PlaceProjection::ErrorArm(false))]
                        && r.class == row.class
                });
                let mapped = rows
                    .next()
                    .ok_or_else(|| bad("Task poll whole outcome omits a complete result class"))?;
                if rows.next().is_some() {
                    return Err(bad("Task poll whole outcome duplicates a result class"));
                }
                key(output, &[], mapped.binder, b)?
            }
            _ => return Err(bad("Task poll class output has another complete outcome")),
        };
        e.equal(
            local,
            task_variable(ty, &values[task.0 as usize], row.binder, b)?,
            b,
        )?;
    }
    Ok(())
}
