//! A pointer slot and the saved pointer's referent are different actual cells.
//! Only real incoming reference layers seed this namespace. Reads copy the
//! saved permission; they do not borrow the slot for the result's lifetime.
use super::*;

pub(super) fn shared(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, .. })
}

pub(super) fn seed(ty: &Type, base: HoldRoot, state: &mut State,
    loans: &mut Vec<Loan>, b: &mut Budget) -> CompileResult<()> {
    let mut ty = ty;
    let mut depth = 0u32;
    loop {
        let Type::Address { inner, .. } = ty else { break; };
        let Type::Address { mutable, .. } = inner.as_ref() else { break; };
        depth = depth.checked_add(1).ok_or_else(|| failure("raw reference input depth overflows"))?;
        if depth as usize > b.partial_depth { return Err(failure("raw reference input chain exceeds its complete depth")); }
        fee(b, 4)?;
        let target = Target { base, retained_port: Some(PortOrigin::RawReference { depth }),
            stored: Vec::new(), place: Vec::new(), heap_element: None, heap_parents: Vec::new() };
        let cell = Target { base,
            retained_port: if depth == 1 { None } else { Some(PortOrigin::RawReference { depth: depth - 1 }) },
            stored: Vec::new(), place: Vec::new(), heap_element: None, heap_parents: Vec::new() };
        let id = loans.len();
        loans.push(Loan { targets: Targets::from([target.clone()]), parents: Loans::new(),
            mutable: *mutable, receiver_head: false });
        b.stored_exclusive_loans |= *mutable;
        ordered(b, state.raw_cells.len(), target_width(&cell))?;
        fee(b, target_width(&cell).saturating_add(target_width(&target)).saturating_add(2))?;
        // Symbolic input dependence only: caller substitution selects its
        // actual extra categories. This set creates no permission or loan.
        let upper = Targets::from([target.clone()]);
        if state.raw_cells.insert(cell.clone(), Permission { targets: Targets::from([target]), loans: Loans::from([id]) }).is_some() {
            return Err(failure("raw reference input repeats its actual pointer cell"));
        }
        ordered(b, state.raw_upper.len(), target_width(&cell))?;
        state.raw_upper.insert(cell, upper);
        ty = inner;
    }
    Ok(())
}

/// During fixed-point construction a missing predecessor is bottom. At an
/// actual read/call every alternative must resolve to its real current cell.
pub(super) fn read(state: &State, address: &Permission, complete: bool,
    b: &mut Budget) -> CompileResult<Permission> {
    let mut out = Permission::default();
    for target in &address.targets {
        fee(b, target_width(target).saturating_add(1))?;
        let permission = if target.retained_port.is_none() && target.stored.is_empty()
            && target.heap_element.is_none() && target.heap_parents.is_empty() {
            match target.base {
                HoldRoot::Local(local) => {
                    ordered(b, state.locals.len(), 0)?;
                    match state.locals.get(&local) {
                        Some(value) if target.place.is_empty() => Some(&value.pointer),
                        Some(value) => { ordered(b, value.slots.len(), target.place.len())?; value.slots.get(&target.place) }
                        None => None,
                    }
                }
                HoldRoot::Parameter(_) | HoldRoot::Capture(_) if !target.place.is_empty() => {
                    ordered(b, state.external.len(), target.place.len())?;
                    match state.external.get(&target.base) {
                        Some(rows) => { ordered(b, rows.len(), target.place.len())?; rows.get(&target.place) }
                        None => None,
                    }
                }
                _ => { ordered(b, state.raw_cells.len(), target_width(target))?; state.raw_cells.get(target) }
            }
        } else {
            ordered(b, state.raw_cells.len(), target_width(target))?;
            state.raw_cells.get(target)
        };
        match permission {
            Some(value) if !complete || !value.targets.is_empty() => { union(&mut out, value, b)?; }
            _ if !complete => {},
            _ => return Err(failure("raw pointer read has an unresolved or vacant actual incoming cell")),
        }
    }
    if complete && address.targets.is_empty() { return Err(failure("raw pointer read has no actual slot address")); }
    Ok(out)
}

pub(super) fn substitute(state: &State, address: &Permission, depth: u32,
    b: &mut Budget) -> CompileResult<Permission> {
    if depth == 0 || depth as usize > b.partial_depth { return Err(failure("raw reference source has an invalid actual indirection depth")); }
    let mut value = copy_permission(address, b)?;
    for _ in 0..depth { value = read(state, &value, false, b)?; }
    Ok(value)
}

pub(super) fn binding(f: &TypedFunction, parameter: u32, depth: u32,
    b: &mut Budget) -> CompileResult<StorageBinder> {
    if depth == 0 || depth as usize > b.partial_depth { return Err(failure("raw reference class has an invalid depth")); }
    let local = f.parameters.get(parameter as usize).and_then(|id| f.locals.get(id.0 as usize))
        .ok_or_else(|| failure("raw reference class has no actual input"))?;
    let mut ty = &local.ty;
    let mut path = Vec::new();
    for _ in 0..depth {
        let Type::Address { inner, .. } = ty else { return Err(failure("raw reference class crosses a nonreference actual boundary")); };
        if !matches!(inner.as_ref(), Type::Address { .. }) { return Err(failure("raw reference source exceeds its actual nested reference chain")); }
        fee(b, 1)?;
        path.push(LifeTypeProjection::Pointee);
        ty = inner;
    }
    let header = f.storage_life.as_ref().ok_or_else(|| failure("raw reference class lacks its actual complete header"))?;
    let mut found = None;
    for row in &header.classes {
        fee(b, row.position.path.len().saturating_add(path.len()).saturating_add(1))?;
        if row.position.base == StorageBase::Parameter(parameter) && row.position.path == path && row.position.class == 0 {
            if found.replace(row.binder).is_some() { return Err(failure("raw reference header repeats its exact pointee class")); }
        }
    }
    found.ok_or_else(|| failure("raw reference source omits its complete pointee class transport"))
}

/// A converged call cannot hide a missing raw cell behind another nonempty
/// alternative. This checks actual sources and effects, never an upper claim.
pub(super) fn call(callee: &TypedFunction, summary: &Facts, arguments: &[ValueId],
    values: &[Value], state: &State, b: &mut Budget) -> CompileResult<()> {
    fee(b, callee.parameters.len())?;
    if !callee.parameters.iter().any(|local| matches!(&callee.locals[local.0 as usize].ty,
        Type::Address { inner, .. } if matches!(inner.as_ref(), Type::Address { .. }))) { return Ok(()); }
    for source in summary.returned.values().flatten()
        .chain(summary.returned_environment.values().flatten())
        .chain(summary.returned_task_ports.values().flatten())
        .chain(summary.returned_dyn_ports.values().flatten())
        .chain(summary.external_after.values().flatten())
        .chain(summary.head_accesses.keys()) {
        fee(b, target_width(source).saturating_add(1))?;
        let Some(PortOrigin::RawReference { depth }) = source.retained_port else { continue; };
        let HoldRoot::Parameter(parameter) = source.base else { return Err(failure("raw call source is outside its original input namespace")); };
        let value = arguments.get(parameter as usize).and_then(|id| values.get(id.0 as usize))
            .ok_or_else(|| failure("raw call source has no actual incoming value"))?;
        if depth == 0 || depth as usize > b.partial_depth || !source.stored.is_empty() {
            return Err(failure("raw call source changes its actual pointer-cell namespace"));
        }
        let mut permission = copy_permission(&value.pointer, b)?;
        for _ in 0..depth { permission = read(state, &permission, true, b)?; }
    }
    Ok(())
}

/// Extra candidate categories belong to the value stored in a real pointer
/// cell. They stay separate from the permission returned by read().
pub(super) fn upper(state: &State, address: &Permission, b: &mut Budget) -> CompileResult<Targets> {
    let mut out = Targets::new();
    for cell in &address.targets {
        fee(b, target_width(cell).saturating_add(1))?;
        let extra = if let HoldRoot::Local(local) = cell.base
            && cell.retained_port.is_none() && cell.stored.is_empty() && cell.place.is_empty()
            && cell.heap_element.is_none() && cell.heap_parents.is_empty() {
            ordered(b, state.locals.len(), 0)?;
            state.locals.get(&local).map(|value| &value.source_upper)
        } else {
            ordered(b, state.raw_upper.len(), target_width(cell))?;
            state.raw_upper.get(cell)
        };
        let Some(extra) = extra else { continue; };
        for target in extra {
            ordered(b, out.len(), target_width(target))?;
            fee(b, target_width(target))?;
            out.insert(target.clone());
        }
    }
    Ok(out)
}

pub(super) fn substitute_upper(source: &Target, arguments: &[ValueId], values: &[Value],
    state: &State, b: &mut Budget) -> CompileResult<Targets> {
    let Some(PortOrigin::RawReference { depth }) = source.retained_port else { return Ok(Targets::new()); };
    if depth == 0 || depth as usize > b.partial_depth || !source.stored.is_empty() {
        return Err(failure("raw category dependence changes its actual pointer-cell namespace"));
    }
    let HoldRoot::Parameter(parameter) = source.base else { return Err(failure("raw category dependence is outside its original input")); };
    let value = arguments.get(parameter as usize).and_then(|value| values.get(value.0 as usize))
        .ok_or_else(|| failure("raw category dependence has no actual argument"))?;
    let mut pointer = copy_permission(&value.pointer, b)?;
    for _ in 1..depth { pointer = read(state, &pointer, false, b)?; }
    upper(state, &pointer, b)
}
