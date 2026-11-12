//! Provider bodies are checked against real SSA permissions, not their port
//! proposal or the analyzer's plan. Public rows are only upper bounds.
use super::*;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-BODY", None, message)
}
fn record(
    rows: &mut Vec<(Source, bool)>,
    permission: &Permission,
    writing: bool,
    b: &mut Budget,
) -> Check<()> {
    for source in &permission.sources {
        // Local stack accesses are checked by the memory kernel but do not
        // become external helper requirements.
        if matches!(source.base, HoldRoot::Local(_)) {
            continue;
        }
        source_work(std::slice::from_ref(source), b)?;
        b.spend(rows.len().saturating_mul(width(source).saturating_add(1)))?;
        if !rows
            .iter()
            .any(|(old, write)| old == source && *write == writing)
        {
            rows.push((source.clone(), writing));
        }
    }
    Ok(())
}

pub(super) fn work(summary: &Summary, b: &mut Budget) -> Check<()> {
    b.spend(
        summary
            .dyn_effects
            .len()
            .saturating_add(summary.dyn_accesses.len())
            .saturating_add(1),
    )?;
    for (source, _) in &summary.dyn_accesses {
        source_work(std::slice::from_ref(source), b)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn observe(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    state: &Frame,
    values: &[Payload],
    summaries: &[Summary],
    rows: &mut Vec<(Source, bool)>,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    let pointer = |id: ValueId| {
        values
            .get(id.0 as usize)
            .map(|p| &p.address)
            .ok_or_else(|| bad("provider actual access operand is absent"))
    };
    match &i.kind {
        InstructionKind::ReadAddress { address } => record(rows, pointer(*address)?, false, b)?,
        InstructionKind::WriteAddress { address, .. } => record(rows, pointer(*address)?, true, b)?,
        InstructionKind::AddressField { address, .. } => record(
            rows,
            pointer(*address)?,
            i.result.is_some_and(|id| {
                matches!(f.values[id.0 as usize], Type::Address { mutable: true, .. })
            }),
            b,
        )?,
        InstructionKind::LoadStoredBorrow {
            storage,
            path,
            mode,
        } => {
            record(
                rows,
                pointer(*storage)?,
                matches!(
                    mode,
                    StoredBorrowLoad::Take | StoredBorrowLoad::Reborrow { mutable: true }
                ),
                b,
            )?;
            let write = match mode {
                StoredBorrowLoad::Take => i.result.is_some_and(|id| {
                    matches!(f.values[id.0 as usize], Type::Address { mutable: true, .. })
                }),
                StoredBorrowLoad::Reborrow { mutable } => *mutable,
                _ => false,
            };
            record(
                rows,
                &super::select(state, pointer(*storage)?, path, b)?,
                write,
                b,
            )?;
        }
        InstructionKind::StoreStoredBorrow {
            storage,
            path,
            value,
        } => {
            record(rows, pointer(*storage)?, true, b)?;
            let exclusive = matches!(vt(f, *value)?, Type::Address { mutable: true, .. });
            record(
                rows,
                &super::select(state, pointer(*storage)?, path, b)?,
                exclusive,
                b,
            )?;
            record(rows, pointer(*value)?, exclusive, b)?;
        }
        InstructionKind::BorrowDynOwned { local, mutable } => {
            record(rows, &dyn_closed::head(f, *local, state, b)?, *mutable, b)?;
        }
        InstructionKind::MakeDynOwned { vtable, source, .. } => {
            b.spend(module.dyn_vtables.len())?;
            let table = module
                .dyn_vtables
                .iter()
                .find(|table| table.id == *vtable)
                .ok_or_else(|| bad("actual constructor access table is absent"))?;
            let interface = crate::dynamic_ports::interface(module, table.interface, b)?;
            let actual = values
                .get(source.0 as usize)
                .ok_or_else(|| bad("actual constructor input is absent"))?;
            for row in &table.ports {
                let port = interface
                    .retained_life
                    .ports
                    .get(row.port as usize)
                    .ok_or_else(|| bad("constructor access port is absent"))?;
                for position in &row.sources {
                    let path = crate::dynamic_ports::source_places(position, b)?;
                    let permission = leaf(actual, &path, b)?
                        .ok_or_else(|| bad("constructor access source leaf is absent"))?;
                    record(
                        rows,
                        permission,
                        matches!(
                            port.mode,
                            StoredBorrowMode::ExclusiveAddress
                                | StoredBorrowMode::WritableDescription
                        ),
                        b,
                    )?;
                }
            }
        }
        InstructionKind::DropDynOwned { local } if dyn_ports::operation(module, f, i, b)? => {
            if !dyn_ports::moved_owner(state, *local, b)? {
                let Type::OwnedDyn { interface } = f.locals[local.0 as usize].ty else {
                    return Err(bad("cleanup access local has no actual owner type"));
                };
                let item = crate::dynamic_ports::interface(module, interface, b)?;
                let cleanup = item
                    .owning_cleanup
                    .as_ref()
                    .ok_or_else(|| bad("actual cleanup access is Unknown"))?;
                for (write, positions) in [
                    (false, &cleanup.access.reads),
                    (true, &cleanup.access.writes),
                ] {
                    for position in positions {
                        let Some((ContractProjection::RetainedPort(port), carrier)) =
                            position.path.split_last()
                        else {
                            return Err(bad("cleanup access has no actual typed port"));
                        };
                        record(
                            rows,
                            &dyn_ports::select(&state.locals[local.0 as usize], carrier, *port, b)?,
                            write,
                            b,
                        )?;
                    }
                }
            }
        }
        InstructionKind::ReadPackageField { local, .. } => {
            record(rows, &dyn_closed::head(f, *local, state, b)?, false, b)?
        }
        InstructionKind::TakePackageField { local, .. }
        | InstructionKind::StorePackageField { local, .. }
        | InstructionKind::TakeTupleField { local, .. } => {
            record(rows, &dyn_closed::head(f, *local, state, b)?, true, b)?
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            let actual = summaries
                .get(function.0 as usize)
                .ok_or_else(|| bad("provider helper summary is absent"))?;
            for (source, write) in &actual.dyn_accesses {
                record(
                    rows,
                    &instantiate(module, f, source, arguments, values, state, b)?,
                    *write,
                    b,
                )?;
            }
        }
        InstructionKind::CallDynSlot {
            callee,
            interface,
            slot,
            arguments,
        } => {
            let item = crate::dynamic_ports::interface(module, *interface, b)?;
            let contract = &item
                .slots
                .get(*slot as usize)
                .ok_or_else(|| bad("provider nested slot is absent"))?
                .contract;
            let access = contract
                .call
                .as_ref()
                .ok_or_else(|| bad("provider nested dynamic access is Unknown"))?;
            b.spend(arguments.len().saturating_add(1))?;
            let mut args = vec![*callee];
            args.extend(arguments);
            for (write, positions) in [(false, &access.reads), (true, &access.writes)] {
                for position in positions {
                    record(
                        rows,
                        &dyn_ports::slot_source(module, f, position, &args, values, state, b)?,
                        write,
                        b,
                    )?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
fn effect(summary: &mut Summary, effect: DynEffect, b: &mut Budget) -> Check<()> {
    b.spend(summary.dyn_effects.len().saturating_add(1))?;
    if !summary.dyn_effects.contains(&effect) {
        summary.dyn_effects.push(effect);
    }
    Ok(())
}
pub(super) fn effects(
    module: &TypedModule,
    f: &TypedFunction,
    g: &FunctionGraph,
    summaries: &[Summary],
    summary: &mut Summary,
    b: &mut Budget,
) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    summary.dyn_effects_known = true;
    if f.foreign.is_some() {
        for e in [
            DynEffect::Alloc,
            DynEffect::Io,
            DynEffect::Block,
            DynEffect::Ffi,
            DynEffect::Reenter,
            DynEffect::Panic,
        ] {
            effect(summary, e, b)?;
        }
        return Ok(());
    }
    for (block, reachable) in f.blocks.iter().zip(&g.reachable) {
        if !reachable {
            continue;
        }
        b.spend(block.instructions.len().saturating_add(1))?;
        for i in &block.instructions {
            match &i.kind {
                InstructionKind::Call { function, .. }
                | InstructionKind::ForeignCall { function, .. } => {
                    let actual = summaries
                        .get(function.0 as usize)
                        .ok_or_else(|| bad("provider reachable helper summary is absent"))?;
                    summary.dyn_effects_known &= actual.dyn_effects_known;
                    for e in &actual.dyn_effects {
                        effect(summary, e.clone(), b)?;
                    }
                }
                InstructionKind::CallDynSlot {
                    interface, slot, ..
                } => {
                    let item = crate::dynamic_ports::interface(module, *interface, b)?;
                    if let Some(effects) = item
                        .slots
                        .get(*slot as usize)
                        .and_then(|s| s.contract.effects.as_ref())
                    {
                        for e in effects {
                            effect(summary, e.clone(), b)?;
                        }
                    } else {
                        summary.dyn_effects_known = false;
                    }
                }
                InstructionKind::Binary { .. }
                | InstructionKind::Unary { .. }
                | InstructionKind::ConvertInteger { .. }
                | InstructionKind::ConvertNumber { .. } => effect(summary, DynEffect::Panic, b)?,
                InstructionKind::Constant(_)
                | InstructionKind::Load(_)
                | InstructionKind::Store { .. }
                | InstructionKind::TakeLocal { .. }
                | InstructionKind::StorageLive(_)
                | InstructionKind::StorageDead(_)
                | InstructionKind::ScopeEnter { .. }
                | InstructionKind::ScopeExit { .. }
                | InstructionKind::UnsafeEnter { .. }
                | InstructionKind::UnsafeExit { .. }
                | InstructionKind::BorrowLocal { .. }
                | InstructionKind::BorrowLocalMut { .. }
                | InstructionKind::ReborrowAddress { .. }
                | InstructionKind::AddressField { .. }
                | InstructionKind::ReadAddress { .. }
                | InstructionKind::WriteAddress { .. }
                | InstructionKind::LoadStoredBorrow { .. }
                | InstructionKind::StoreStoredBorrow { .. }
                | InstructionKind::MakeAggregate { .. }
                | InstructionKind::ExtractField { .. }
                | InstructionKind::MakePackage { .. }
                | InstructionKind::ReadPackageField { .. }
                | InstructionKind::TakePackageField { .. }
                | InstructionKind::StorePackageField { .. }
                | InstructionKind::TakeTupleField { .. }
                | InstructionKind::MakeDynBorrow { .. }
                | InstructionKind::BorrowDynOwned { .. } => {}
                _ => summary.dyn_effects_known = false,
            }
        }
        if !matches!(
            block.terminator,
            Terminator::Return(_) | Terminator::Jump(_) | Terminator::Branch { .. }
        ) {
            summary.dyn_effects_known = false;
        }
    }
    Ok(())
}
fn selected(table: &DynVtable, source: &Source, b: &mut Budget) -> Check<Option<StoragePosition>> {
    b.spend(table.ports.len().saturating_add(width(source)))?;
    let HoldRoot::Parameter(parameter) = source.base else {
        return Ok(None);
    };
    if source.heap_element.is_some() || source.retained_port.is_some() {
        return Err(bad(
            "provider source has no complete physical Self-to-port route",
        ));
    }
    if parameter != 0 {
        return Ok(Some(StoragePosition {
            base: StorageBase::Parameter(parameter),
            path: source
                .leaf
                .iter()
                .copied()
                .map(ContractProjection::Place)
                .collect(),
        }));
    }
    if source.leaf.is_empty() {
        return Ok(Some(StoragePosition {
            base: StorageBase::Parameter(0),
            path: vec![],
        }));
    }
    for row in &table.ports {
        for position in &row.sources {
            let path = crate::dynamic_ports::source_places(position, b)?;
            if path == source.leaf {
                return Ok(Some(StoragePosition {
                    base: StorageBase::Parameter(0),
                    path: vec![ContractProjection::RetainedPort(row.port)],
                }));
            }
        }
    }
    Err(bad(
        "actual provider accesses a saved leaf absent from its port map",
    ))
}
pub(super) fn validate(module: &TypedModule, summaries: &[Summary], b: &mut Budget) -> Check<()> {
    if !b.dyn_contexts {
        return Ok(());
    }
    for table in &module.dyn_vtables {
        let iface = crate::dynamic_ports::interface(module, table.interface, b)?;
        if iface.context.is_none() {
            continue;
        }
        for entry in &table.entries {
            let summary = summaries
                .get(entry.function.0 as usize)
                .ok_or_else(|| bad("actual provider summary is absent"))?;
            if !summary.complete || !summary.dyn_effects_known {
                return Err(bad(
                    "provider lacks complete actual memory or effect coverage",
                ));
            }
            let slot = &iface.slots[entry.slot as usize];
            let effects = slot
                .contract
                .effects
                .as_ref()
                .ok_or_else(|| bad("provider effect upper is Unknown"))?;
            for effect in &summary.dyn_effects {
                b.spend(effects.len())?;
                if !effects.contains(effect) {
                    return Err(bad(
                        "actual reachable provider effect exceeds its declaration",
                    ));
                }
            }
            let access = slot
                .contract
                .call
                .as_ref()
                .ok_or_else(|| bad("provider access upper is Unknown"))?;
            for (source, writing) in &summary.dyn_accesses {
                let Some(position) = selected(table, source, b)? else {
                    continue;
                };
                let upper = if *writing {
                    &access.writes
                } else {
                    &access.reads
                };
                b.spend(
                    upper
                        .len()
                        .saturating_mul(position.path.len().saturating_add(1)),
                )?;
                if !upper.contains(&position) {
                    return Err(bad(
                        "actual provider target access exceeds its public head or port upper",
                    ));
                }
            }
            if let Some(DynResultSource::Borrow { sources, .. }) = &slot.contract.result {
                for (path, actuals) in &summary.result {
                    if !path.is_empty() {
                        return Err(bad(
                            "provider borrowed result has an unsupported nested result leaf",
                        ));
                    }
                    for source in actuals {
                        let position = selected(table, source, b)?
                            .ok_or_else(|| bad("provider returned activation-local storage"))?;
                        b.spend(
                            sources
                                .len()
                                .saturating_mul(position.path.len().saturating_add(1)),
                        )?;
                        for atom in sources { crate::exact_storage_source(atom)?; }
                        if !sources.contains(&StorageSource::Exact(position)) {
                            return Err(bad(
                                "provider actual borrowed result exceeds its public source upper",
                            ));
                        }
                    }
                }
            }
            let guarantees = slot
                .contract
                .guarantees
                .as_ref()
                .ok_or_else(|| bad("provider receiver-after source upper is Unknown"))?;
            for row in &table.ports {
                let Some(DynGuarantee::StorageSources(upper)) = guarantees.get(row.port as usize)
                else {
                    return Err(bad("provider receiver-after directory omits a port"));
                };
                for position in &row.sources {
                    let path = crate::dynamic_ports::source_places(position, b)?;
                    b.spend(
                        summary
                            .external_after
                            .len()
                            .saturating_mul(path.len().saturating_add(1)),
                    )?;
                    let actual = summary
                        .external_after
                        .iter()
                        .find(|(parameter, p, _)| *parameter == 0 && p == &path)
                        .ok_or_else(|| {
                            bad("provider actual Return omits a retained Self after-state leaf")
                        })?;
                    if actual.2.is_empty() {
                        return Err(bad(
                            "provider actual after-state leaf is missing, not known empty",
                        ));
                    }
                    for source in &actual.2 {
                        if source.base == HoldRoot::Static {
                            continue;
                        }
                        let source = selected(table, source, b)?.ok_or_else(|| {
                            bad("provider after-state retains activation-local storage")
                        })?;
                        b.spend(
                            upper
                                .sources
                                .len()
                                .saturating_mul(source.path.len().saturating_add(1)),
                        )?;
                        if !crate::exact_storage_sources_contain(&upper.sources,&source)? {
                            return Err(bad(
                                "actual provider after-state exceeds its declared source upper",
                            ));
                        }
                    }
                }
            }
        }
        let cleanup = iface
            .owning_cleanup
            .as_ref()
            .ok_or_else(|| bad("provider whole cleanup is Unknown"))?;
        cleanup_proof(module, &table.self_type, cleanup, b)?;
        b.pending_dyn_tables.remove(&table.id);
    }
    Ok(())
}

/// Saved external values are dependencies, not owned teardown payloads. Actual
/// owned descendants still require their exact independently proved cleanup.
fn cleanup_proof(
    module: &TypedModule,
    root: &Type,
    claim: &DynCleanupContract,
    b: &mut Budget,
) -> Check<()> {
    let mut pending = vec![(root, 0usize)];
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth.min(128) {
            return Err(limit("provider cleanup graph exceeds selected depth"));
        }
        match ty {
            Type::BorrowedStorage { storage, .. } => pending.push((storage, depth + 1)),
            Type::Address { .. } | Type::Desc(_) => {}
            Type::Managed(payload) => {
                if !crate::dynamic_ports::closed_targets(module, payload, b)? {
                    return Err(unsupported(
                        "borrowed resource teardown needs actual provider port access reconstruction",
                        None,
                    ));
                }
                crate::dynamic_cleanup::prove(module, ty, claim, b)?;
            }
            Type::OwnedDyn { interface } => {
                let item = crate::dynamic_ports::interface(module, *interface, b)?;
                if item.context.is_some() {
                    return Err(unsupported(
                        "nested Context owner cleanup needs complete retained source transport",
                        None,
                    ));
                }
                let inner = crate::dynamic_cleanup::bound(item.owning_cleanup.as_ref(), b)?;
                for effect in &inner.effects {
                    b.spend(claim.effects.len())?;
                    if !claim.effects.contains(effect) {
                        return Err(bad(
                            "nested erased-owner cleanup effect exceeds its outer bound",
                        ));
                    }
                }
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                b.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, ty)| (ty, depth + 1)));
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    b.spend(fields.len())?;
                    pending.extend(fields.iter().map(|ty| (ty, depth + 1)));
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
                pending.extend([(error.as_ref(), depth + 1), (result.as_ref(), depth + 1)]);
            }
            Type::Array { element, .. } => pending.push((element, depth + 1)),
            ty if ty.is_b0_scalar() || *ty == Type::Unit => {}
            _ => {
                return Err(unsupported(
                    "provider owning descendant lacks complete actual synchronous cleanup",
                    None,
                ));
            }
        }
        if pending.len() > b.limits.max_nodes {
            return Err(limit("provider cleanup pending graph exceeds node bound"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod access_tests {
    use super::*;
    #[test]
    fn stored_pointer_access_observes_the_target_instead_of_the_pointer_slot() {
        let (module, _) = zil_ir::wire::decode_module_candidate(
            include_bytes!("../../tests/fixtures/dyn-context/context-read.typed.wire"),
            &Limits::default(),
        )
        .unwrap();
        let (f, instruction) = module
            .functions
            .iter()
            .find_map(|f| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.instructions)
                    .find(|i| matches!(i.kind, InstructionKind::LoadStoredBorrow { .. }))
                    .map(|i| (f, i))
            })
            .unwrap();
        let InstructionKind::LoadStoredBorrow { storage, path, .. } = &instruction.kind else {
            unreachable!()
        };
        let mut b = Budget::new(&Limits::default());
        b.dyn_contexts = true;
        let head = Source {
            heap_parents: vec![],
            base: HoldRoot::Parameter(0),
            retained_port: None,
            leaf: vec![],
            inline: vec![],
            heap_element: None,
        };
        let target = Source {
            leaf: path.clone(),
            ..head.clone()
        };
        let slot = Source {
            inline: path.clone(),
            ..head.clone()
        };
        let mut values = vec![Payload::default(); f.values.len()];
        values[storage.0 as usize].address = Permission {
            sources: vec![head.clone()],
            loans: vec![],
        };
        let mut state = Frame {
            external: vec![Payload::default()],
            ..Frame::default()
        };
        add_leaf(
            &mut state.external[0],
            path,
            &Permission {
                sources: vec![target.clone()],
                loans: vec![],
            },
            &mut b,
        )
        .unwrap();
        let mut rows = Vec::new();
        observe(
            &module,
            f,
            instruction,
            &state,
            &values,
            &[],
            &mut rows,
            &mut b,
        )
        .unwrap();
        assert!(rows.contains(&(head, false)));
        assert!(rows.contains(&(target, false)));
        assert!(!rows.iter().any(|(source, _)| *source == slot));
    }
}
