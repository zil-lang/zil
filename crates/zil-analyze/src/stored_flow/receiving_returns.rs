//! Directed saved-reference receiving uses. Original roles select one use;
//! actual memory supplies every target, current field and parent loan here.
use super::*;

fn contract_source(
    module: &TypedModule, f: &TypedFunction, parameter: u32,
    path: &[PlaceProjection], b: &mut Budget,
) -> CompileResult<()> {
    // An ordinary borrowed aggregate needs no erased-interface upper. Every
    // actual provider sharing this entry must, however, cover the exact saved
    // field through its real Context port and complete result source atom.
    for table in &module.dyn_vtables {
        fee(b, table.entries.len().saturating_add(1))?;
        for entry in &table.entries {
            if entry.function != f.id { continue; }
            let interface = module.dyn_interfaces.iter().find(|row| row.id == table.interface)
                .ok_or_else(|| failure("receiving provider has no complete actual interface"))?;
            fee(b, module.dyn_interfaces.len())?;
            let slot = interface.slots.get(entry.slot as usize)
                .ok_or_else(|| failure("receiving provider has no actual result slot"))?;
            let Some(DynResultSource::Borrow { sources, mutable: false }) = &slot.contract.result else {
                return Err(failure("receiving provider omits its complete borrowed-result source upper"));
            };
            let input = f.parameters.get(parameter as usize).and_then(|id| f.locals.get(id.0 as usize))
                .ok_or_else(|| failure("receiving provider has no genuine input"))?;
            crate::dyn_metadata::ty(&input.ty, b)?;
            crate::dyn_metadata::ty(&table.self_type, b)?;
            if parameter != 0 || !matches!(&input.ty, Type::Address { inner, .. } if inner.as_ref() == &table.self_type)
                || slot.result != f.return_type {
                return Err(failure("receiving provider changes complete Self, result leaf or receiver identity"));
            }
            let mut covered = false;
            for atom in sources {
                crate::dyn_metadata::recipe(atom, b)?;
                // Static and owning inline views cannot certify a saved
                // external address, even when its pointee has the same Type.
                let StorageSource::ReborrowedLeaf { source, head } = atom else { continue; };
                let [ContractProjection::RetainedPort(port)] = source.path.as_slice() else { continue; };
                if source.base != StorageBase::Parameter(parameter) || head.base != source.base || !head.path.is_empty() { continue; }
                let leaf = interface.retained_life.ports.get(*port as usize)
                    .ok_or_else(|| failure("receiving result source selects no complete Context port"))?;
                crate::dyn_metadata::ty(&leaf.leaf, b)?;
                if leaf.mode != StoredBorrowMode::SharedAddress || leaf.leaf != f.return_type { continue; }
                let (_, domain) = carrier(&table.self_type)
                    .ok_or_else(|| failure("receiving provider has no complete saved-field Self domain"))?;
                fee(b, domain.slots.len().saturating_mul(path.len().saturating_add(1)))?;
                let mut fields = domain.slots.iter().filter(|slot| slot.path == path);
                let field = fields.next().ok_or_else(|| failure("receiving provider path has no actual Self class"))?;
                if fields.next().is_some() || field.mode != StoredBorrowMode::SharedAddress
                    || table.self_classes.get(field.binder as usize) != Some(&StorageBinder::Fixed(leaf.binder)) {
                    return Err(failure("receiving source port changes its complete intrinsic Self/Context class mapping"));
                }
                fee(b, table.ports.len())?;
                let mut rows = table.ports.iter().filter(|row| row.port == *port);
                let row = rows.next().ok_or_else(|| failure("receiving provider omits the actual source-port mapping"))?;
                if rows.next().is_some() { return Err(failure("receiving provider repeats a source-port mapping")); }
                for mapped in &row.sources {
                    fee(b, mapped.path.len().saturating_add(path.len()).saturating_add(1))?;
                    if mapped.base == StorageBase::Result && mapped.path.len() == path.len()
                        && mapped.path.iter().zip(path).all(|(actual, field)| *actual == ContractProjection::Place(*field)) {
                        covered = true;
                    }
                }
            }
            if !covered { return Err(failure("actual receiving field is outside every complete provider source-port upper")); }
        }
    }
    Ok(())
}

/// Apply only the receiving adapter already proved for this exact Return SSA.
/// Its original expression classes are unchanged. No other CopyShared read,
/// including an explicit Fixed result, acquires the receiver dependency.
pub(super) fn apply(
    module: &TypedModule, f: &TypedFunction, instruction: &Instruction,
    storage: &Permission, path: &[PlaceProjection], selected: &mut Permission,
    loans: &[Loan], b: &mut Budget,
) -> CompileResult<()> {
    let Some(value) = instruction.result else { return Ok(()); };
    if !b.initialization.as_ref().is_some_and(|init| init.module == module as *const TypedModule as usize) { return Ok(()); }
    fee(b, b.semantic_receiving_returns.len())?;
    let mut roles = b.semantic_receiving_returns.iter().filter(|(function, _, source, _)| *function == f.id && *source == value);
    let Some(first) = roles.next() else { return Ok(()); };
    let binder = first.3;
    if roles.any(|row| row.3 != binder) { return Err(failure("receiving SSA has incompatible original result roles")); }
    let mut roots = storage.targets.iter();
    let Some(root) = roots.next() else { return Ok(()); };
    if roots.next().is_some() || !root.stored.is_empty() || !root.place.is_empty()
        || root.retained_port.is_some() || root.heap_element.is_some() || !root.heap_parents.is_empty() {
        return Err(failure("receiving field has no unique complete actual input head"));
    }
    let HoldRoot::Parameter(parameter) = root.base else { return Err(failure("receiving field is not rooted in its genuine borrowed input")); };
    if path.is_empty() || binding(f, StorageBase::Parameter(parameter), &Vec::new(), b)? != binder
        || binding(f, StorageBase::Result, &Vec::new(), b)? != binder
        || !matches!(binder, StorageBinder::Invocation(_)) {
        return Err(failure("receiving field changes its independently checked input-head result bound"));
    }
    if selected.targets.is_empty() { return Ok(()); } // solver bottom supplies no new source
    if storage.loans.is_empty() || selected.loans.is_empty() {
        return Err(failure("receiving field omits its actual owner or saved-source parent loans"));
    }
    // Walk every parent now. Bounds-checked actual loan IDs, not a Source
    // label or a header, supply the graph carried into the returned view.
    ancestry(&storage.loans, loans, b)?;
    ancestry(&selected.loans, loans, b)?;
    contract_source(module, f, parameter, path, b)?;
    fee(b, storage.loans.len())?;
    for parent in &storage.loans { ordered(b, selected.loans.len(), 0)?; selected.loans.insert(*parent); }
    Ok(())
}

/// Track a separate relation for each data target, intersecting the head
/// dependencies over all actual Return alternatives that contain that target.
/// A head found on a different branch cannot certify this target's escape.
pub(super) fn record(
    f: &TypedFunction, value: &Value, entry: &State, loans: &[Loan],
    facts: &mut Facts, b: &mut Budget,
) -> CompileResult<()> {
    if !matches!(f.return_type, Type::Address { .. }) { return Ok(()); }
    let Some(contract) = &f.storage_life else { return Ok(()); };
    let Some(result) = super::super::signature_life::outer(contract, StorageBase::Result) else { return Ok(()); };
    let parents = ancestry(&value.pointer.loans, loans, b)?;
    let mut heads = BTreeSet::new();
    for (parameter, local) in f.parameters.iter().enumerate() {
        fee(b, 1)?;
        let Some(input) = f.locals.get(local.0 as usize) else { return Err(failure("returned head input is outside its actual locals")); };
        if !matches!(input.ty, Type::Address { .. }) { continue; }
        let parameter = u32::try_from(parameter).map_err(|_| failure("returned input ordinal overflows"))?;
        if super::super::signature_life::outer(contract, StorageBase::Parameter(parameter)) != Some(result) { continue; }
        let head = &entry.locals.get(local).ok_or_else(|| failure("returned dependency has no actual entry input"))?.pointer;
        fee(b, head.loans.len())?;
        if head.loans.is_empty() || !head.loans.iter().all(|loan| parents.contains(loan)) { continue; }
        heads.insert(parameter);
        ordered(b, facts.returned_head_inputs.len(), 0)?;
        facts.returned_head_inputs.insert(parameter);
    }
    for target in &value.pointer.targets {
        ordered(b, facts.returned_head_targets.len(), target_width(target))?;
        if let Some(previous) = facts.returned_head_targets.get_mut(target) {
            fee(b, previous.len().saturating_add(heads.len()))?;
            previous.retain(|head| heads.contains(head));
        } else {
            fee(b, target_width(target).saturating_add(heads.len()))?;
            facts.returned_head_targets.insert(target.clone(), heads.clone());
        }
    }
    Ok(())
}
