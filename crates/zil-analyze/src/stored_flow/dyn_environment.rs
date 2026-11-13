//! Actual dynamic dependencies inside concrete callable environments. Logical
//! capture selectors never become physical addresses or replace owner debt.
use super::*;
use zil_ir::ContractProjection;
type Key = Vec<ContractProjection>;
type SourceRows = BTreeMap<Key, (Vec<Target>, StoredBorrowMode)>;

fn exact_target(position: &StoragePosition, b: &mut Budget) -> CompileResult<Target> {
    let StorageBase::Parameter(index) = position.base else {
        return Err(failure("dynamic environment source is not an actual input"));
    };
    fee(b, position.path.len().saturating_mul(2).saturating_add(1))?;
    let mut carrier = position.path.clone();
    let retained_port =
        if let Some(ContractProjection::RetainedPort(port)) = carrier.last().cloned() {
            carrier.pop();
            Some(PortOrigin::Dyn {
                carrier: carrier.clone(),
                port,
            })
        } else {
            None
        };
    let mut stored = Vec::new();
    for p in &carrier {
        match p {
            ContractProjection::Place(p) => stored.push(*p),
            ContractProjection::Capture(_) if retained_port.is_some() => {}
            _ => {
                return Err(failure(
                    "environment source crosses an unproved logical selector",
                ));
            }
        }
    }
    // For an opaque port the full carrier is already its semantic identity;
    // physical stored paths apply only before the first Capture boundary.
    if retained_port.is_some() {
        stored = carrier
            .iter()
            .take_while(|p| matches!(p, ContractProjection::Place(_)))
            .map(|p| {
                let ContractProjection::Place(p) = p else {
                    unreachable!()
                };
                *p
            })
            .collect();
    }
    Ok(Target {
        base: HoldRoot::Parameter(index),
        stored,
        place: vec![],
        retained_port,
        heap_element: None,
        heap_parents: vec![],
    })
}
fn contract(module: &TypedModule, f: &TypedFunction, b: &mut Budget) -> CompileResult<SourceRows> {
    let Some(c) = &f.callable_return_contract else {
        return Ok(BTreeMap::new());
    };
    let mut directory = Vec::new();
    dyn_address::logical_directory(module, &f.return_type, &mut vec![], &mut directory, b)?;
    let Some(bindings) = &c.capture_sources else {
        return Err(failure("environment source upper is Unknown"));
    };
    let mut rows = BTreeMap::new();
    for (path, item) in directory {
        for (port, shape) in item.retained_life.ports.iter().enumerate() {
            let mut key = path.clone();
            key.push(ContractProjection::RetainedPort(port as u32));
            let Some((ContractProjection::Capture(index), tail)) = key.split_first() else {
                return Err(failure(
                    "returned dynamic environment has no capture position",
                ));
            };
            let wanted = StoragePosition {
                base: StorageBase::Capture(*index),
                path: tail.to_vec(),
            };
            fee(
                b,
                bindings.len().saturating_mul(key.len().saturating_add(1)),
            )?;
            let mut claims = bindings.iter().filter(|row| row.position == wanted);
            let claim = claims
                .next()
                .ok_or_else(|| failure("returned dynamic environment omits a port source row"))?;
            if claims.next().is_some() {
                return Err(failure("returned dynamic environment repeats a port row"));
            }
            let mut targets = Vec::new();
            for atom in &claim.sources {
                let source = crate::exact_storage_source(atom)?;
                let target = exact_target(source, b)?;
                fee(
                    b,
                    targets
                        .len()
                        .saturating_mul(target_width(&target).saturating_add(1)),
                )?;
                if targets.contains(&target) {
                    return Err(failure("dynamic environment repeats an input source"));
                }
                targets.push(target);
            }
            fee(b, key.len())?;
            rows.insert(key, (targets, shape.mode));
        }
    }
    Ok(rows)
}
pub(super) fn sites(
    module: &TypedModule,
    instruction: &Instruction,
    sites: &mut SlotLoanIds,
    loans: &mut Vec<Loan>,
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    let InstructionKind::Call { function, .. } = instruction.kind else {
        return Ok(());
    };
    let f = module
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| failure("environment call target absent"))?;
    for (path, (sources, mode)) in contract(module, f, b)? {
        for source in sources {
            if dyn_return::transports_existing(f, &source) {
                continue;
            }
            fee(
                b,
                path.len()
                    .saturating_add(target_width(&source))
                    .saturating_add(3),
            )?;
            ordered(
                b,
                sites.dynamic.len(),
                path.len().saturating_add(target_width(&source)),
            )?;
            let id = loans.len();
            loans.push(Loan::default());
            let mutable = mode == StoredBorrowMode::ExclusiveAddress;
            b.stored_exclusive_loans |= mutable;
            if sites
                .dynamic
                .insert(
                    (
                        instruction as *const Instruction as usize,
                        path.clone(),
                        source,
                    ),
                    (id, mutable),
                )
                .is_some()
            {
                return Err(failure("dynamic environment retained site repeated"));
            }
        }
    }
    Ok(())
}
pub(super) fn summarize(
    module: &TypedModule,
    f: &TypedFunction,
    value: &Value,
    facts: &mut Facts,
    b: &mut Budget,
) -> CompileResult<()> {
    for (path, (upper, _)) in contract(module, f, b)? {
        ordered(b, value.dyn_ports.len(), path.len())?;
        let Some(actual) = value.dyn_ports.get(&path) else {
            facts.dyn_return_complete = false;
            continue;
        };
        for target in &actual.targets {
            fee(
                b,
                upper
                    .len()
                    .saturating_mul(target_width(target).saturating_add(1)),
            )?;
            if !upper.contains(target) {
                return Err(failure(
                    "actual returned capture port exceeds its declared input upper",
                ));
            }
        }
        fee(b, path.len())?;
        facts.returned_dyn_capture_upper.insert(path, upper);
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn call(
    instruction: &Instruction,
    callee: &TypedFunction,
    summary: &Facts,
    args: &[ValueId],
    values: &[Value],
    state: &State,
    out: &mut Value,
    sites: &SlotLoanIds,
    loans: &mut [Loan],
    b: &mut Budget,
) -> CompileResult<()> {
    if !b.dyn_contexts || callee.callable_return_contract.is_none() {
        return Ok(());
    }
    if !summary.complete {
        out.creation_complete = Some(false);
        return Ok(());
    }
    // Every input is read from one call-before state. Moving a by-value owner
    // carries its existing leases; borrowed actuals establish fresh children.
    let mut prepared = BTreeMap::new();
    for (path, sources) in &summary.returned_dyn_capture_upper {
        let mut permission = Permission::default();
        for source in sources {
            let actual = substitute(source, args, values, state, b)?;
            let actual = if dyn_return::transports_existing(callee, source) {
                actual
            } else {
                dyn_return::retain(instruction, path, source, &actual, sites, loans, b)?
            };
            union(&mut permission, &actual, b)?;
        }
        fee(b, path.len())?;
        prepared.insert(path.clone(), permission);
    }
    for (path, permission) in prepared {
        out.dyn_ports.insert(path, permission);
    }
    Ok(())
}
fn remember(
    facts: &mut BTreeMap<Target, bool>,
    actual: &Permission,
    write: bool,
    b: &mut Budget,
) -> CompileResult<()> {
    for source in &actual.targets {
        if !matches!(source.base, HoldRoot::Parameter(_) | HoldRoot::Capture(_)) {
            continue;
        }
        ordered(b, facts.len(), target_width(source))?;
        fee(b, target_width(source))?;
        *facts.entry(source.clone()).or_default() |= write;
    }
    Ok(())
}
fn selected(
    actual: &Value,
    position: &StoragePosition,
    b: &mut Budget,
) -> CompileResult<Permission> {
    if position.path.is_empty() {
        return copy_permission(&actual.pointer, b);
    }
    let Some((ContractProjection::RetainedPort(port), carrier)) = position.path.split_last() else {
        return Err(failure("dynamic access is not a complete opaque position"));
    };
    dyn_address::select(actual, carrier, *port, b)
}
fn receiver(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &State,
    values: &[Value],
    b: &mut Budget,
) -> CompileResult<Option<Value>> {
    let mut out = match i.kind {
        InstructionKind::CallClosureShared { callee, .. }
        | InstructionKind::CallClosureMut { callee, .. } => {
            let mut value = snapshot(
                state
                    .locals
                    .get(&callee)
                    .ok_or_else(|| failure("called dynamic environment is not initialized"))?,
                b,
            )?;
            fee(b, 1)?;
            value.pointer.targets.insert(Target {
                base: local_capture_base(state, callee, b)?,
                stored: vec![],
                place: vec![],
                retained_port: None,
                heap_element: None,
                heap_parents: vec![],
            });
            value
        }
        InstructionKind::CallClosure { callee, .. }
        | InstructionKind::CallClosureOnce { callee, .. }
        | InstructionKind::CallClosureBorrowed { callee, .. } => snapshot(
            values
                .get(callee.0 as usize)
                .ok_or_else(|| failure("actual dynamic environment SSA absent"))?,
            b,
        )?,
        _ => return Ok(None),
    };
    if let Some((d, _)) = super::super::stored_callables::called(module, f, &i.kind) {
        let mut rows = Vec::new();
        dyn_address::logical_directory(module, &Type::Closure(d.id), &mut vec![], &mut rows, b)?;
        if rows.is_empty() {
            return Ok(None);
        }
        for (path, item) in rows {
            for port in 0..item.retained_life.ports.len() {
                let key = path
                    .iter()
                    .cloned()
                    .chain([ContractProjection::RetainedPort(port as u32)])
                    .collect::<Vec<_>>();
                ordered(b, out.dyn_ports.len(), key.len())?;
                if !out.dyn_ports.contains_key(&key) {
                    return Err(failure(
                        "called dynamic environment lacks an actual port row",
                    ));
                }
            }
        }
    }
    // Borrowed environments name their current memory, rather than a stale
    // SSA snapshot. All actual head alternatives must independently exist.
    if matches!(i.kind, InstructionKind::CallClosureBorrowed { .. }) {
        let mut current = Value::default();
        for head in &out.pointer.targets {
            if !head.place.is_empty() || !head.stored.is_empty() || head.retained_port.is_some() {
                return Err(failure(
                    "borrowed environment has no exact initialized head",
                ));
            }
            let value = match head.base {
                HoldRoot::Local(local) => state.locals.get(&local),
                HoldRoot::Capture(index) => state.captures.get(&index),
                _ => None,
            };
            let value = value.ok_or_else(|| {
                failure("borrowed dynamic environment current memory unavailable")
            })?;
            dyn_address::merge(&mut current, value, b)?;
        }
        out.dyn_ports = current.dyn_ports;
    }
    Ok(Some(out))
}
fn captured(source: &Target, receiver: &Value, b: &mut Budget) -> CompileResult<Permission> {
    let HoldRoot::Capture(index) = source.base else {
        return Err(failure(
            "entry dynamic access is not rooted in its actual environment",
        ));
    };
    if let Some(PortOrigin::Dyn { carrier, port }) = &source.retained_port {
        fee(b, carrier.len().saturating_add(1))?;
        let mut path = vec![ContractProjection::Capture(index)];
        path.extend_from_slice(carrier);
        return dyn_address::select(receiver, &path, *port, b);
    }
    if source.retained_port.is_some() || !source.stored.is_empty() {
        return Err(failure(
            "captured dynamic access needs its complete logical source",
        ));
    }
    let mut actual = copy_permission(&receiver.pointer, b)?;
    let mut targets = Targets::new();
    for root in &actual.targets {
        fee(
            b,
            target_width(root)
                .saturating_add(source.place.len())
                .saturating_add(2),
        )?;
        let mut selected = root.clone();
        selected.place.push(PlaceProjection::Field(index));
        selected.place.extend_from_slice(&source.place);
        targets.insert(selected);
    }
    actual.targets = targets;
    Ok(actual)
}
/// Return actual pre-call accesses. The caller performs the ordinary conflict
/// check against its own complete live loan arena before any result is added.
#[allow(clippy::too_many_arguments)]
pub(super) fn accesses(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &State,
    values: &[Value],
    summaries: &[Facts],
    records: &mut BTreeMap<Target, bool>,
    b: &mut Budget,
) -> CompileResult<Vec<(Permission, bool)>> {
    if !b.dyn_contexts {
        return Ok(vec![]);
    }
    let mut rows = Vec::new();
    match &i.kind {
        InstructionKind::CallDynSlot {
            callee,
            interface,
            slot,
            ..
        } => {
            fee(b, module.dyn_interfaces.len())?;
            let slot = module
                .dyn_interfaces
                .iter()
                .find(|d| d.id == *interface)
                .and_then(|d| d.slots.get(*slot as usize))
                .ok_or_else(|| failure("actual dynamic access slot absent"))?;
            let actual = &values[callee.0 as usize];
            let call = slot
                .contract
                .call
                .as_ref()
                .ok_or_else(|| failure("dynamic access contract Unknown"))?;
            for (positions, write) in [(&call.reads, false), (&call.writes, true)] {
                for position in positions {
                    if position.base != StorageBase::Parameter(0) {
                        return Err(failure(
                            "dynamic access needs actual nonreceiver argument substitution",
                        ));
                    }
                    let p = selected(actual, position, b)?;
                    if !p.targets.is_empty() {
                        remember(records, &p, write, b)?;
                        rows.push((p, write));
                    }
                }
            }
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            for (source, write) in &summaries[function.0 as usize].dyn_accesses {
                let actual = substitute(source, arguments, values, state, b)?;
                if actual.targets.is_empty() {
                    return Err(failure(
                        "ordinary dynamic effect lacks its actual input permission",
                    ));
                }
                remember(records, &actual, *write, b)?;
                rows.push((actual, *write));
            }
        }
        InstructionKind::DropDynOwned { local } => {
            if let Some(actual) = state.locals.get(local) {
                let Type::OwnedDyn { interface } = f.locals[local.0 as usize].ty else {
                    return Err(failure("cleanup actual owner type absent"));
                };
                let d = module
                    .dyn_interfaces
                    .iter()
                    .find(|d| d.id == interface)
                    .ok_or_else(|| failure("cleanup actual interface absent"))?;
                let c = d
                    .owning_cleanup
                    .as_ref()
                    .ok_or_else(|| failure("cleanup access upper Unknown"))?;
                for (positions, write) in [(&c.access.reads, false), (&c.access.writes, true)] {
                    for position in positions {
                        let p = selected(actual, position, b)?;
                        if !p.targets.is_empty() {
                            remember(records, &p, write, b)?;
                            rows.push((p, write));
                        }
                    }
                }
            }
        }
        InstructionKind::DropClosure { local } => {
            if let Some(actual) = state.locals.get(local) {
                let mut directory = Vec::new();
                dyn_address::logical_directory(
                    module,
                    &f.locals[local.0 as usize].ty,
                    &mut Vec::new(),
                    &mut directory,
                    b,
                )?;
                for (carrier, interface) in directory {
                    let cleanup = interface
                        .owning_cleanup
                        .as_ref()
                        .ok_or_else(|| failure("captured owner cleanup access is Unknown"))?;
                    for (positions, write) in [
                        (&cleanup.access.reads, false),
                        (&cleanup.access.writes, true),
                    ] {
                        for position in positions {
                            if position.base != StorageBase::Result {
                                return Err(failure(
                                    "captured cleanup access is not rooted in its actual owner",
                                ));
                            }
                            let mut path = carrier.clone();
                            fee(b, carrier.len().saturating_add(position.path.len()))?;
                            path.extend_from_slice(&position.path);
                            let permission = selected(
                                actual,
                                &StoragePosition {
                                    base: StorageBase::Result,
                                    path,
                                },
                                b,
                            )?;
                            if !permission.targets.is_empty() {
                                remember(records, &permission, write, b)?;
                                rows.push((permission, write));
                            }
                        }
                    }
                }
            }
        }
        _ => {
            if let Some((d, _)) = super::super::stored_callables::called(module, f, &i.kind)
                && let Some(actual) = receiver(module, f, i, state, values, b)?
            {
                for (source, write) in &summaries[d.function.0 as usize].dyn_accesses {
                    let permission = captured(source, &actual, b)?;
                    // A moved whole environment has no activation-storage head;
                    // its responsibility proof already owns the inline bytes.
                    if permission.targets.is_empty() && source.retained_port.is_some() {
                        return Err(failure(
                            "captured dynamic effect lacks an actual retained target",
                        ));
                    }
                    if !permission.targets.is_empty() {
                        remember(records, &permission, *write, b)?;
                        rows.push((permission, *write));
                    }
                }
            }
        }
    }
    Ok(rows)
}
