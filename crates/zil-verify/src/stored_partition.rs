//! Bounded separation of actual stored-reference bodies from ordinary bodies.
//! This table is reconstructed from raw types and calls, never from a plan.
use super::*;

fn stored_type(module: &TypedModule, ty: &Type, budget: &mut Budget) -> Check<bool> {
    let mut pending = vec![ty];
    let mut found = false;
    let mut closures_seen = std::collections::BTreeSet::new();
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        match ty {
            Type::Closure(id) => {
                budget.spend(closures_seen.len().saturating_add(1))?;
                if !closures_seen.insert(*id) {
                    continue;
                }
                let definition = closures::definition(module, *id, None)?;
                if crate::ordinary_heads::resource_closure(module, *id, budget)? {
                    continue;
                }
                // Work selection only. The complete relation, every actual
                // creator/entry and its memory sources are checked elsewhere.
                found |= definition.storage_life.is_some();
                budget.spend(
                    definition
                        .captures
                        .len()
                        .saturating_add(definition.parameters.len())
                        .saturating_add(1),
                )?;
                pending.extend(&definition.captures);
                pending.extend(&definition.parameters);
                pending.push(&definition.result);
            }
            Type::BorrowedStorage { storage, .. } => {
                found = true;
                pending.push(storage);
            }
            Type::ByteBuffer => {
                found = true;
            }
            Type::OwnedStore { element } => {
                found |= super::stored_owned::ordinary_store_value(module, ty, budget)?;
                pending.push(element);
            }
            Type::Registration {
                environment: element,
            }
            | Type::AsyncRegistration {
                environment: element,
            }
            | Type::Task {
                result: element, ..
            } => pending.push(element),
            Type::FunctionPointer { parameters, result }
            | Type::BorrowedFunctionPointer {
                parameters, result, ..
            }
            | Type::ForeignCallback { parameters, result } => {
                pending.extend(parameters);
                pending.push(result);
            }
            Type::Address { inner, .. } => {
                found |= matches!(inner.as_ref(), Type::Desc(_) | Type::BorrowedStorage { .. });
                pending.push(inner);
            }
            Type::Managed(inner)
            | Type::Desc(inner)
            | Type::Array { element: inner, .. }
            | Type::Slice { element: inner }
            | Type::Opaque { hidden: inner, .. } => pending.push(inner),
            Type::Tuple(fields) => {
                found |= stored_flow::compound_targets(ty, budget)?;
                pending.extend(fields);
            }
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| ty));
            }
            Type::Enum { variants, .. } => {
                pending.extend(variants.iter().flat_map(|(_, fields)| fields));
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            _ => {}
        }
    }
    Ok(found)
}

fn connected_storage(ty: &Type, budget: &mut Budget) -> Check<bool> {
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        match ty {
            Type::Managed(_)
            | Type::Desc(_)
            | Type::OwnedStore { .. }
            | Type::Registration { .. }
            | Type::AsyncRegistration { .. }
            | Type::TaskI32
            | Type::Task { .. }
            | Type::ByteBuffer
            | Type::FunctionPointer { .. }
            | Type::BorrowedFunctionPointer { .. } => return Ok(true),
            // Indirect targets and callback environments need the connected
            // program proof. Their signatures and captures are checked there.
            Type::ForeignCallback { .. } | Type::Closure(_) => return Ok(true),
            Type::BorrowedStorage { storage, .. }
            | Type::Address { inner: storage, .. }
            | Type::Array {
                element: storage, ..
            }
            | Type::Slice { element: storage }
            | Type::Opaque {
                hidden: storage, ..
            } => pending.push(storage),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } | Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| ty))
            }
            Type::Enum { variants, .. } => {
                pending.extend(variants.iter().flat_map(|(_, fields)| fields))
            }
            Type::ErrorUnion { error, result } => pending.extend([error.as_ref(), result.as_ref()]),
            _ => {}
        }
    }
    Ok(false)
}

pub(super) fn mixed(module: &TypedModule, budget: &mut Budget) -> Check<Option<Vec<bool>>> {
    budget.spend(module.functions.len())?;
    let mut mask = Vec::with_capacity(module.functions.len());
    for f in &module.functions {
        budget.spend(
            f.locals
                .len()
                .saturating_add(f.values.len())
                .saturating_add(2),
        )?;
        let mut stored = f.storage_life.is_some()
            && !super::ordinary_heads::body(module, f, budget)?
            && f.call_kind != CallKind::Suspend
            && !tasks::borrowed_signature(module, f, budget)?;
        for ty in std::iter::once(&f.return_type)
            .chain(f.locals.iter().map(|l| &l.ty))
            .chain(&f.values)
        {
            stored |= stored_type(module, ty, budget)?;
        }
        mask.push(stored);
    }
    // None selects the complete stored ledger; an explicit empty partition
    // instead proves every actual body with the ordinary borrow/resource lane.
    if !mask.iter().any(|v| *v) {
        return Ok(Some(mask));
    }
    if mask.iter().all(|v| *v) {
        return Ok(None);
    }
    // Independent CFG components can use separate solvers. Every executable
    // edge between components selects the complete proof, regardless of ABI.
    for definition in &module.closures {
        budget.spend(1)?;
        let Some(stored) = mask.get(definition.function.0 as usize) else {
            return Err(reject(
                "E-VERIFY-STORED-PARTITION",
                None,
                "closure entry is outside the function table",
            ));
        };
        if *stored { return Ok(None); }
    }
    budget.spend(module.dyn_vtables.len())?;
    for table in &module.dyn_vtables {
        budget.spend(table.entries.len().saturating_add(1))?;
        for entry in table.entries.iter().chain(table.cleanup.iter()) {
            if mask.get(entry.function.0 as usize).copied().unwrap_or(true) {
                return Ok(None);
            }
        }
    }
    for resource in &module.resources {
        budget.spend(1)?;
        if stored_type(module, &resource.payload, budget)? {
            return Ok(None);
        }
        if resource
            .teardown
            .is_some_and(|id| mask.get(id.0 as usize).copied().unwrap_or(true))
        {
            return Ok(None);
        }
    }
    for (index, f) in module.functions.iter().enumerate() {
        if mask[index] {
            let mut forbidden = false;
            for ty in std::iter::once(&f.return_type)
                .chain(f.locals.iter().map(|l| &l.ty))
                .chain(&f.values)
            {
                forbidden |= connected_storage(ty, budget)?;
            }
            if forbidden {
                return Ok(None);
            }
        }
        for block in &f.blocks {
            budget.spend(block.instructions.len())?;
            for i in &block.instructions {
                let targets = match i.kind {
                    InstructionKind::Call { function, .. }
                    | InstructionKind::SuspendCall { function, .. }
                    | InstructionKind::ForeignCall { function, .. }
                    | InstructionKind::FunctionRef { function }
                    | InstructionKind::ForeignFunctionRef { function } => [Some(function), None, None],
                    InstructionKind::TaskCreate { worker, .. }
                    | InstructionKind::TaskCreateOwned { worker, .. }
                    | InstructionKind::TaskStart { worker, .. }
                    | InstructionKind::WithScopedThread { worker, .. } => [Some(worker), None, None],
                    InstructionKind::MakeClosure { closure, .. } => {
                        [Some(closures::definition(module, closure, Some(i.span))?.function), None, None]
                    }
                    InstructionKind::WithForeignCallback { register, close, .. }
                    | InstructionKind::RegistrationStart { register, close, .. } => [Some(register), Some(close), None],
                    InstructionKind::AsyncRegistrationStart { register, request_close, poll_drain, .. } =>
                        [Some(register), Some(request_close), Some(poll_drain)],
                    _ => [None; 3],
                };
                for target in targets.into_iter().flatten() {
                    budget.spend(1)?;
                    let target_domain = mask.get(target.0 as usize).ok_or_else(|| reject(
                        "E-VERIFY-STORED-PARTITION", Some(i.span), "function edge target is absent",
                    ))?;
                    if mask[index] != *target_domain { return Ok(None); }
                }
            }
        }
    }
    Ok(Some(mask))
}

/// A Known0 ordinary head still owes an executable effect/memory proof when
/// its original method contract is pending. This only selects the existing
/// full verifier and all exact executable dependencies; it grants no facts.
pub(super) fn contracts(module: &TypedModule, mask: &mut [bool], b: &mut Budget) -> Check<()> {
    if mask.len() != module.functions.len() {
        return Err(reject(
            "E-VERIFY-STORED-PARTITION",
            None,
            "contract work mask omits actual functions",
        ));
    }
    b.spend(b.pending_dyn_contracts.methods.len())?;
    let roots = b
        .pending_dyn_contracts
        .methods
        .iter()
        .filter(|request| {
            matches!(
                request.origin,
                crate::dynamic_witness::requests::Origin::Definition { .. }
            )
        })
        .map(|request| request.function)
        .collect::<Vec<_>>();
    if roots.is_empty() {
        return Ok(());
    }
    b.spend(module.functions.len().saturating_add(roots.len()))?;
    let mut seen = vec![false; module.functions.len()];
    let mut queue = roots;
    while let Some(function) = queue.pop() {
        b.spend(1)?;
        let index = function.0 as usize;
        if module
            .functions
            .get(index)
            .is_none_or(|row| row.id != function)
        {
            return Err(reject(
                "E-VERIFY-STORED-PARTITION",
                None,
                "original contract selects an absent actual body",
            ));
        }
        if std::mem::replace(&mut seen[index], true) {
            continue;
        }
        mask[index] = true;
        for target in crate::stored_flow::contract_dependencies(module, function, b)? {
            b.spend(queue.len().saturating_add(1))?;
            queue.push(target);
        }
    }
    Ok(())
}

/// A caller without a raw cell in its own spelling can still consume a
/// callee's copied inner pointer. Keep that caller and its actual executable
/// dependencies in the same memory proof. This selects work only; no edge
/// supplies a source, a class equality or a complete callee summary.
pub(super) fn references(module: &TypedModule, mask: &mut [bool], b: &mut Budget) -> Check<()> {
    if !b.raw_reference_cells {
        return Ok(());
    }
    let count = module.functions.len();
    if mask.len() != count {
        return Err(reject(
            "E-VERIFY-STORED-PARTITION",
            None,
            "raw reference work mask omits actual functions",
        ));
    }
    b.spend(count.saturating_mul(3))?;
    let mut edges = vec![Vec::new(); count];
    let mut seen = vec![false; count];
    let mut pending = mask
        .iter()
        .enumerate()
        .filter_map(|(index, yes)| yes.then_some(index))
        .collect::<Vec<_>>();
    for (index, function) in module.functions.iter().enumerate() {
        for target in crate::stored_flow::contract_dependencies(module, function.id, b)? {
            let target_index = target.0 as usize;
            if module
                .functions
                .get(target_index)
                .is_none_or(|row| row.id != target)
            {
                return Err(reject(
                    "E-VERIFY-STORED-PARTITION",
                    None,
                    "raw reference dependency selects an absent actual function",
                ));
            }
            b.spend(
                edges[index]
                    .len()
                    .saturating_add(edges[target_index].len())
                    .saturating_add(2),
            )?;
            edges[index].push(target_index);
            edges[target_index].push(index);
        }
    }
    while let Some(index) = pending.pop() {
        b.spend(1)?;
        if std::mem::replace(&mut seen[index], true) {
            continue;
        }
        mask[index] = true;
        b.spend(edges[index].len())?;
        for target in &edges[index] {
            if !seen[*target] {
                if pending.len() == pending.capacity() {
                    b.spend(pending.len().saturating_add(1))?;
                }
                b.spend(1)?;
                pending.push(*target);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod raw_call_work_tests {
    use super::*;
    #[test]
    fn rich_return_callers_and_their_actual_dependencies_share_the_existing_solver() {
        let (module, _) = crate::tests::baseline();
        let mut module = module;
        let mut raw = module.functions[0].clone();
        raw.id = FunctionId(0);
        let mut middle = raw.clone();
        middle.id = FunctionId(1);
        let mut caller = raw.clone();
        caller.id = FunctionId(2);
        let mut peer = raw.clone();
        peer.id = FunctionId(3);
        for f in [&mut raw, &mut middle, &mut caller, &mut peer] {
            f.blocks[0].instructions.clear();
        }
        let span = raw.span;
        middle.blocks[0].instructions.push(Instruction {
            result: None,
            kind: InstructionKind::Call {
                function: FunctionId(0),
                arguments: vec![],
            },
            span,
        });
        caller.blocks[0].instructions.push(Instruction {
            result: None,
            kind: InstructionKind::Call {
                function: FunctionId(1),
                arguments: vec![],
            },
            span,
        });
        module.functions = vec![raw, middle, caller, peer];
        let mut mask = vec![true, false, false, false];
        let mut b = Budget::new(&Limits::default());
        b.raw_reference_cells = true;
        references(&module, &mut mask, &mut b).unwrap();
        assert_eq!(mask, vec![true, true, true, false]);
        let mut ordinary = vec![true, false, false, false];
        references(&module, &mut ordinary, &mut Budget::new(&Limits::default())).unwrap();
        assert_eq!(
            ordinary,
            vec![true, false, false, false],
            "unrelated profiles keep their work selection"
        );
    }
}
