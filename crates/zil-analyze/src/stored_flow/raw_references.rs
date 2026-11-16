//! A pointer slot and the saved pointer's referent are different actual cells.
//! Only real incoming reference layers seed this namespace. Reads copy the
//! saved permission; they do not borrow the slot for the result's lifetime.
use super::*;

pub(super) fn shared(ty: &Type) -> bool {
    matches!(ty, Type::Address { mutable: false, .. })
}

/// Qualification is structural only. The complete signature/SSA forest,
/// actual cells and each access are still checked by the following passes.
pub(super) fn admitted(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    let Type::Address { inner, .. } = ty else { return Ok(false); };
    if !matches!(inner.as_ref(), Type::Address { .. }) { return Ok(false); }
    let mut leaf = inner.as_ref();
    let mut depth = 1usize;
    while let Type::Address { inner, .. } = leaf {
        fee(b, 1)?;
        depth = depth.checked_add(1).ok_or_else(|| failure("raw qualification depth overflows"))?;
        if depth > b.partial_depth { return Err(failure("raw qualification exceeds its full reference depth")); }
        leaf = inner;
    }
    Ok(leaf.is_address_pointee() || carrier(leaf).is_some())
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
    for (cell, after) in &summary.raw_after {
        let address = actual_address(cell, arguments, values, state, b)?;
        read(state, &address, true, b)?;
        if !after.complete || after.targets.is_empty() { return Err(failure("raw callee after-state has no grounded output on every Return")); }
        for source in &after.targets {
            if let Some(PortOrigin::RawReference { depth }) = source.retained_port {
                let HoldRoot::Parameter(parameter) = source.base else { return Err(failure("raw output source leaves its real input")); };
                let actual = arguments.get(parameter as usize).and_then(|id| values.get(id.0 as usize))
                    .ok_or_else(|| failure("raw output source omits an argument"))?;
                let mut pointer = copy_permission(&actual.pointer, b)?;
                for _ in 0..depth { pointer = read(state, &pointer, true, b)?; }
            }
        }
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum WriteSite {
    Write(usize),
    Call { site: usize, parameter: u32, depth: u32 },
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct After {
    pub targets: Targets,
    pub upper: Targets,
    pub heads: BTreeSet<u32>,
    pub stable: bool,
    pub complete: bool,
}
pub(super) fn facts_work(rows: &BTreeMap<Target, After>, b: &mut Budget) -> CompileResult<()> {
    for (cell, after) in rows {
        fee(b, target_width(cell).saturating_add(after.heads.len()).saturating_add(1))?;
        target_work(&after.targets, b)?;
        target_work(&after.upper, b)?;
    }
    Ok(())
}
fn reserve(site: WriteSite, mutable: bool, ids: &mut SlotLoanIds,
    loans: &mut Vec<Loan>, b: &mut Budget) -> CompileResult<()> {
    fee(b, 3)?;
    ordered(b, ids.raw.len(), 1)?;
    let id = loans.len();
    loans.push(Loan::default());
    if ids.raw.insert(site, (id, mutable)).is_some() { return Err(failure("raw cell write repeats its actual loan site")); }
    b.stored_exclusive_loans |= mutable;
    Ok(())
}
pub(super) fn sites(module: &TypedModule, f: &TypedFunction, instruction: &Instruction,
    ids: &mut SlotLoanIds, loans: &mut Vec<Loan>, b: &mut Budget) -> CompileResult<()> {
    let site = instruction as *const Instruction as usize;
    match &instruction.kind {
        InstructionKind::WriteAddress { value, .. } => {
            if let Type::Address { mutable, .. } = value_type(f, *value)? {
                reserve(WriteSite::Write(site), *mutable, ids, loans, b)?;
            }
        }
        InstructionKind::Call { function, .. } | InstructionKind::SuspendCall { function, .. }
            if synchronous_abi::ordinary_call(&instruction.kind, b)? => {
            let callee = module.functions.get(function.0 as usize).ok_or_else(|| failure("raw write call has no actual callee"))?;
            for (parameter, local) in callee.parameters.iter().enumerate() {
                let mut ty = &callee.locals[local.0 as usize].ty;
                let mut depth = 0u32;
                while let Type::Address { mutable, inner } = ty {
                    let Type::Address { mutable: child_mutable, .. } = inner.as_ref() else { break; };
                    fee(b, 1)?;
                    if *mutable { reserve(WriteSite::Call { site,
                        parameter: u32::try_from(parameter).map_err(|_| failure("raw write input ordinal overflow"))?, depth },
                        *child_mutable, ids, loans, b)?; }
                    depth = depth.checked_add(1).ok_or_else(|| failure("raw write depth overflow"))?;
                    if depth as usize > b.partial_depth { return Err(failure("raw write call exceeds its complete reference depth")); }
                    ty = inner;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
fn retain(source: &Permission, site: &WriteSite, ids: &SlotLoanIds,
    loans: &mut [Loan], b: &mut Budget) -> CompileResult<Permission> {
    ordered(b, ids.raw.len(), 1)?;
    let (id, mutable) = ids.raw.get(site).ok_or_else(|| failure("raw cell replacement has no genuine write loan site"))?;
    let loan = loans.get_mut(*id).ok_or_else(|| failure("raw write loan is outside its actual arena"))?;
    let mut joined = Permission { targets: std::mem::take(&mut loan.targets), loans: std::mem::take(&mut loan.parents) };
    union(&mut joined, source, b)?;
    joined.loans.remove(id);
    loan.targets = joined.targets;
    loan.parents = joined.loans;
    loan.mutable = *mutable;
    b.stored_exclusive_loans |= *mutable;
    let mut result = copy_permission(source, b)?;
    result.loans.clear(); result.loans.insert(*id);
    Ok(result)
}
fn install(state: &mut State, cell: &Target, value: &Permission, upper: &Targets,
    strong: bool, b: &mut Budget) -> CompileResult<()> {
    fee(b, target_width(cell).saturating_add(1))?;
    if let HoldRoot::Local(local) = cell.base
        && cell.retained_port.is_none() && cell.stored.is_empty()
        && cell.heap_element.is_none() && cell.heap_parents.is_empty() {
        ordered(b, state.locals.len(), cell.place.len())?;
        let Some(occupant) = state.locals.get_mut(&local) else { return Ok(()); };
        if cell.place.is_empty() {
            if strong { occupant.pointer = copy_permission(value, b)?; target_work(upper, b)?; occupant.source_upper = upper.clone(); }
            else { union(&mut occupant.pointer, value, b)?; dyn_address::upper_union(&mut occupant.source_upper, upper, b)?; }
            return Ok(());
        }
        ordered(b, occupant.slots.len(), cell.place.len())?;
        if strong { occupant.slots.insert(cell.place.clone(), copy_permission(value, b)?); }
        else { union(occupant.slots.entry(cell.place.clone()).or_default(), value, b)?; }
    } else if cell.retained_port.is_none() && cell.stored.is_empty() && !cell.place.is_empty()
        && cell.heap_element.is_none() && cell.heap_parents.is_empty() {
        ordered(b, state.external.len(), cell.place.len())?;
        let Some(rows) = state.external.get_mut(&cell.base) else { return Ok(()); };
        ordered(b, rows.len(), cell.place.len())?;
        if strong { rows.insert(cell.place.clone(), copy_permission(value, b)?); }
        else { union(rows.entry(cell.place.clone()).or_default(), value, b)?; }
        b.stored_slot_updates = true;
    } else {
        ordered(b, state.raw_cells.len(), target_width(cell))?;
        let Some(previous) = state.raw_cells.get_mut(cell) else { return Ok(()); };
        if strong { *previous = copy_permission(value, b)?; } else { union(previous, value, b)?; }
    }
    ordered(b, state.raw_upper.len(), target_width(cell))?;
    if strong { target_work(upper, b)?; state.raw_upper.insert(cell.clone(), upper.clone()); }
    else { dyn_address::upper_union(state.raw_upper.entry(cell.clone()).or_default(), upper, b)?; }
    Ok(())
}
pub(super) fn write(state: &mut State, address: &Permission, value: &Permission,
    upper: &Targets, instruction: &Instruction, ids: &SlotLoanIds,
    loans: &mut [Loan], b: &mut Budget) -> CompileResult<()> {
    let value = retain(value, &WriteSite::Write(instruction as *const Instruction as usize), ids, loans, b)?;
    let strong = address.targets.len() == 1;
    for cell in &address.targets { install(state, cell, &value, upper, strong, b)?; }
    Ok(())
}
fn target_binding(module: &TypedModule, f: &TypedFunction, source: &Target,
    b: &mut Budget) -> CompileResult<Option<StorageBinder>> {
    if source.base == HoldRoot::Static { return Ok(None); }
    let HoldRoot::Parameter(parameter) = source.base else { return Err(failure("raw cell after-state retains activation-local or captured data")); };
    Ok(Some(match &source.retained_port {
        Some(PortOrigin::RawReference { depth }) => binding(f, parameter, *depth, b)?,
        Some(PortOrigin::MutexPayload { .. }) => mutex::source_binding(f, source, b)?,
        Some(_) => task_address::source_binding(module, f, source, b)?,
        None => environment_memory::binding(module, f, parameter, &source.stored, b)?,
    }))
}
pub(super) fn summarize(module: &TypedModule, f: &TypedFunction, entry: &State,
    terminal: &State, loans: &[Loan], facts: &mut Facts, b: &mut Budget) -> CompileResult<()> {
    for (cell, initial) in &entry.raw_cells {
        let HoldRoot::Parameter(parameter) = cell.base else { continue; };
        let depth = match cell.retained_port { None => 0, Some(PortOrigin::RawReference { depth }) => depth, _ => continue };
        let required = binding(f, parameter, depth.checked_add(1).ok_or_else(|| failure("raw cell class depth overflow"))?, b)?;
        ordered(b, terminal.raw_cells.len(), target_width(cell))?;
        let permission = terminal.raw_cells.get(cell).ok_or_else(|| failure("raw input cell disappears on an actual Return"))?;
        permission_clone_work(initial, b)?; permission_clone_work(permission, b)?;
        let stable = initial == permission;
        let parents = ancestry(&permission.loans, loans, b)?;
        let mut heads = BTreeSet::new();
        for (index, local) in f.parameters.iter().enumerate() {
            fee(b, 1)?;
            if f.storage_life.as_ref().and_then(|c| super::super::signature_life::outer(c, StorageBase::Parameter(index as u32))) != Some(required) { continue; }
            ordered(b, entry.locals.len(), 0)?;
            let Some(input) = entry.locals.get(local) else { continue; };
            let mut all = !input.pointer.loans.is_empty();
            for parent in &input.pointer.loans {
                ordered(b, parents.len(), 0)?;
                all &= parents.contains(parent);
            }
            if all { ordered(b, heads.len(), 0)?; heads.insert(index as u32); }
        }
        for source in &permission.targets {
            if let Some(actual) = target_binding(module, f, source, b)?
                && actual != required && heads.is_empty() {
                return Err(failure("raw cell after-state changes its complete stored lifetime relation"));
            }
        }
        ordered(b, facts.raw_after.len(), target_width(cell))?;
        let next = facts.raw_after.entry(cell.clone()).or_insert_with(|| After { stable: true, complete: true, ..After::default() });
        next.stable &= stable;
        next.complete &= !permission.targets.is_empty();
        for source in &permission.targets { ordered(b, next.targets.len(), target_width(source))?; fee(b, target_width(source))?; next.targets.insert(source.clone()); }
        for head in heads { ordered(b, next.heads.len(), 0)?; next.heads.insert(head); }
        if let Some(upper) = terminal.raw_upper.get(cell) { dyn_address::upper_union(&mut next.upper, upper, b)?; }
    }
    Ok(())
}
fn actual_address(cell: &Target, arguments: &[ValueId], values: &[Value], state: &State,
    b: &mut Budget) -> CompileResult<Permission> {
    let HoldRoot::Parameter(parameter) = cell.base else { return Err(failure("raw after-state cell has no actual input coordinate")); };
    let value = arguments.get(parameter as usize).and_then(|id| values.get(id.0 as usize)).ok_or_else(|| failure("raw after-state omits an actual argument"))?;
    match cell.retained_port {
        None => copy_permission(&value.pointer, b),
        Some(PortOrigin::RawReference { depth }) => substitute(state, &value.pointer, depth, b),
        _ => Err(failure("raw after-state changes its pointer-cell namespace")),
    }
}
pub(super) fn after_call(callee: &TypedFunction, summary: &Facts, arguments: &[ValueId],
    values: &[Value], state: &mut State, instruction: &Instruction,
    ids: &SlotLoanIds, loans: &mut [Loan], b: &mut Budget) -> CompileResult<()> {
    // Resolve all RHSs and target cells against the same call-before memory.
    // Mutating one actual alias must not change another summary substitution.
    let mut updates = Vec::new();
    for (cell, after) in &summary.raw_after {
        if after.stable { continue; }
        let address = actual_address(cell, arguments, values, state, b)?;
        let mut value = Permission::default();
        for source in &after.targets { union(&mut value, &super::substitute(source, arguments, values, state, b)?, b)?; }
        for head in &after.heads {
            let input = arguments.get(*head as usize).and_then(|id| values.get(id.0 as usize)).ok_or_else(|| failure("raw after-state omits a genuine head parent"))?;
            for parent in &input.pointer.loans { ordered(b, value.loans.len(), 0)?; value.loans.insert(*parent); }
        }
        target_work(&after.upper, b)?;
        let upper_rows = after.upper.iter().cloned().collect::<Vec<_>>();
        let upper = dyn_address::upper_substitute(&upper_rows, arguments, values, state, b)?;
        let HoldRoot::Parameter(parameter) = cell.base else { return Err(failure("raw after-state changes its actual input identity")); };
        let depth = match cell.retained_port { None => 0, Some(PortOrigin::RawReference { depth }) => depth, _ => return Err(failure("raw after-state has a nonraw cell")) };
        let value = retain(&value, &WriteSite::Call { site: instruction as *const Instruction as usize, parameter, depth }, ids, loans, b)?;
        fee(b, address.targets.len().saturating_add(1))?;
        for cell in &address.targets {
            fee(b, target_width(cell))?; target_work(&upper, b)?;
            updates.push((cell.clone(), copy_permission(&value, b)?, upper.clone(), address.targets.len() == 1));
        }
    }
    // A missing mutable output is solver bottom, never a fabricated identity.
    for (parameter, local) in callee.parameters.iter().enumerate() {
        let mut ty = &callee.locals[local.0 as usize].ty;
        let mut depth = 0u32;
        while let Type::Address { mutable, inner } = ty {
            if !matches!(inner.as_ref(), Type::Address { .. }) { break; }
            fee(b, 1)?;
            if *mutable {
                let cell = Target { base: HoldRoot::Parameter(parameter as u32),
                    retained_port: if depth == 0 { None } else { Some(PortOrigin::RawReference { depth }) },
                    stored: vec![], place: vec![], heap_element: None, heap_parents: vec![] };
                ordered(b, summary.raw_after.len(), target_width(&cell))?;
                if !summary.raw_after.contains_key(&cell) {
                    let address = actual_address(&cell, arguments, values, state, b)?;
                    for cell in &address.targets {
                        fee(b, target_width(cell).saturating_add(1))?;
                        updates.push((cell.clone(), Permission::default(), Targets::new(), address.targets.len() == 1));
                    }
                }
            }
            depth = depth.checked_add(1).ok_or_else(|| failure("raw after-state depth overflows"))?;
            if depth as usize > b.partial_depth { return Err(failure("raw after-state exceeds its complete input chain")); }
            ty = inner;
        }
    }
    // Multiple symbolic inputs can denote the same physical cell. Join all
    // proposed writes before publishing; none may observe another's output.
    let mut joined: BTreeMap<Target, (Permission, Targets, bool)> = BTreeMap::new();
    for (cell, value, upper, strong) in updates {
        ordered(b, joined.len(), target_width(&cell))?;
        if let Some((previous, categories, unique)) = joined.get_mut(&cell) {
            union(previous, &value, b)?;
            dyn_address::upper_union(categories, &upper, b)?;
            *unique = false;
        } else { joined.insert(cell, (value, upper, strong)); }
    }
    for (cell, (value, upper, strong)) in joined { install(state, &cell, &value, &upper, strong, b)?; }
    Ok(())
}
