//! Actual finite Store manager transitions. Every segment remains in its own
//! physical allocation domain; field paths never flatten across heap heads.
use super::*;

pub(super) fn enter(source: &Source, b: &mut Budget) -> Check<Source> {
    if source.heap_element.is_none() && !source.heap_parents.is_empty() {
        return Err(err("Store source has parents without an element domain"));
    }
    if source.heap_parents.len() >= b.limits.max_call_depth {
        return Err(limit("nested Store manager depth exceeds selected bound"));
    }
    b.spend(width(source).saturating_add(1))?;
    let mut target = source.clone();
    if let Some(parent) = target.heap_element.take() {
        b.spend(target.heap_parents.len().saturating_add(1))?;
        target.heap_parents.push(parent);
    }
    target.heap_element = Some(Vec::new());
    Ok(target)
}

pub(super) fn compose(target: &mut Source, relative: &Source, b: &mut Budget) -> Check<()> {
    if relative.static_dependency == StaticProvenance::UpperOnly {
        // A symbolic upper parameter never becomes actual literal evidence
        // merely because its real caller argument happens to be a literal.
        target.static_dependency = StaticProvenance::UpperOnly;
    }
    if relative.heap_element.is_none() && !relative.heap_parents.is_empty() {
        return Err(err(
            "callee Store source has parents without an element domain",
        ));
    }
    let depth = target
        .heap_parents
        .len()
        .checked_add(relative.heap_parents.len())
        .and_then(|n| {
            n.checked_add(usize::from(
                target.heap_element.is_some() && relative.heap_element.is_some(),
            ))
        })
        .ok_or_else(|| limit("nested Store summary depth overflow"))?;
    if depth >= b.limits.max_call_depth {
        return Err(limit("nested Store summary exceeds selected manager depth"));
    }
    b.spend(relative.inline.len())?;
    if let Some(heap) = &mut target.heap_element {
        heap.extend(&relative.inline);
        if relative.heap_element.is_some() {
            b.spend(
                target
                    .heap_parents
                    .len()
                    .saturating_add(relative.heap_parents.len())
                    .saturating_add(1),
            )?;
            target
                .heap_parents
                .push(target.heap_element.take().unwrap());
        }
    } else {
        target.inline.extend(&relative.inline);
    }
    if let Some(heap) = &relative.heap_element {
        b.spend(relative.heap_parents.len().saturating_add(heap.len()))?;
        for parent in &relative.heap_parents {
            b.spend(parent.len())?;
        }
        target
            .heap_parents
            .extend(relative.heap_parents.iter().cloned());
        target.heap_element = Some(heap.clone());
    }
    Ok(())
}

pub(super) fn project<'t>(
    mut ty: &'t Type,
    path: &[PlaceProjection],
    b: &mut Budget,
) -> Check<&'t Type> {
    for step in path {
        b.spend(1)?;
        ty = match (step, ty) {
            (
                PlaceProjection::Field(index),
                Type::Package { fields, .. } | Type::Record { fields, .. },
            ) => fields.get(*index as usize).map(|(_, field)| field),
            _ => None,
        }
        .ok_or_else(|| err("nested Store transition crosses an invalid actual inline field"))?;
    }
    Ok(ty)
}

pub(super) fn deepest_element<'t>(
    mut element: &'t Type,
    parents: &[Path],
    b: &mut Budget,
) -> Check<&'t Type> {
    b.spend(parents.len())?;
    for path in parents {
        let header = project(element, path, b)?;
        let Type::Managed(payload) = header else {
            return Err(err("nested Store transition lacks an actual owned header"));
        };
        let Type::OwnedStore { element: inner } = payload.as_ref() else {
            return Err(err(
                "nested Store transition selects a different resource head",
            ));
        };
        element = inner;
    }
    Ok(element)
}

pub(super) fn writable(permission: &Permission, loans: &[Loan], b: &mut Budget) -> Check<()> {
    let parents = ancestry(&permission.loans, loans, b)?;
    for id in parents {
        b.spend(1)?;
        let loan = loans
            .get(id)
            .ok_or_else(|| err("Store parent loan is absent"))?;
        if loan.exclusive {
            continue;
        }
        for source in &permission.sources {
            for parent in &loan.sources {
                if overlap(source, parent, b)? {
                    return Err(err(
                        "Store mutation cannot escalate a shared actual parent loan",
                    ));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn record(
    accesses: &mut Vec<(Source, bool)>,
    permission: &Permission,
    write: bool,
    b: &mut Budget,
) -> Check<()> {
    for source in &permission.sources {
        // A complete local owner cannot be aliased by the caller after its
        // transfer. Symbolic inputs retain their precise caller obligations.
        if !matches!(source.base, HoldRoot::Parameter(_) | HoldRoot::Capture(_)) {
            continue;
        }
        b.spend(
            accesses
                .len()
                .saturating_mul(width(source).saturating_add(1)),
        )?;
        if accesses
            .iter()
            .any(|(old, old_write)| old == source && *old_write == write)
        {
            continue;
        }
        b.spend(
            width(source)
                .saturating_add(accesses.len())
                .saturating_add(2),
        )?;
        accesses.push((source.clone(), write));
    }
    Ok(())
}
