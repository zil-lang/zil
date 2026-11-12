//! Producer-side validation of owned opaque callable results. No body facts
//! from this module are exported as authority: Verify reconstructs its own.
use super::*;

mod return_life;
mod sources;
pub(super) fn check_pending_sources(b: &Budget) -> CompileResult<()> {
    if b.pending_returned_source_proofs.is_empty() {
        Ok(())
    } else {
        Err(bad(
            "returned callable has no completed actual terminal memory source proof",
        ))
    }
}
pub(super) fn check_sources(
    module: &TypedModule,
    function: &TypedFunction,
    budget: &mut Budget,
) -> CompileResult<()> {
    sources::check(module, function, budget)
}

pub(super) fn capture_effects(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<(bool, bool, bool)> {
    let mut pending = vec![(ty, 0usize)];
    let (mut hold, mut call, mut write) = (false, false, false);
    while let Some((ty, depth)) = pending.pop() {
        b.spend(1, "actual helper external leaf envelope")?;
        if depth > b.partial_depth {
            return Err(bad("helper capture envelope exceeds finite depth"));
        }
        match ty {
            Type::Address { mutable, .. } => {
                hold = true;
                call = true;
                write |= *mutable;
            }
            Type::Desc(_) => {
                call = true;
                write = true;
            }
            Type::BorrowedStorage { life, .. } => {
                b.spend(life.slots.len(), "actual helper qualified slots")?;
                for slot in &life.slots {
                    call = true;
                    hold |= matches!(
                        slot.mode,
                        StoredBorrowMode::SharedAddress | StoredBorrowMode::ExclusiveAddress
                    );
                    write |= matches!(
                        slot.mode,
                        StoredBorrowMode::ExclusiveAddress | StoredBorrowMode::WritableDescription
                    );
                }
            }
            Type::Closure(id) => {
                let d = module
                    .closures
                    .get(id.0 as usize)
                    .ok_or_else(|| bad("helper capture child definition absent"))?;
                b.spend(d.captures.len(), "actual helper child capture graph")?;
                pending.extend(d.captures.iter().map(|ty| (ty, depth + 1)));
            }
            _ => {}
        }
    }
    Ok((hold, call, write))
}

#[path = "owning_factory.rs"]
mod owning_factory;

pub(super) fn check_function(
    module: &TypedModule,
    function: &TypedFunction,
    reachable: &[bool],
    budget: &mut Budget,
) -> CompileResult<()> {
    if function.callable_return_contract.is_none()
        && !matches!(function.return_type, Type::Closure(_))
    {
        return Ok(());
    }
    budget.spend(1, "callable return envelope")?;
    let contract = function
        .callable_return_contract
        .as_ref()
        .ok_or_else(|| bad("closure result has no public callable return contract"))?;

    return_life::check(module, function, contract, budget)?;
    let bindings = contract
        .capture_sources
        .as_ref()
        .ok_or_else(|| bad("callable capture sources are unknown"))?;
    budget.spend(
        bindings.len().saturating_add(1),
        "callable source binding inventory",
    )?;
    for binding in bindings {
        budget.spend(
            binding
                .position
                .path
                .len()
                .saturating_add(binding.sources.len())
                .saturating_add(1),
            "callable source binding paths",
        )?;
        for atom in &binding.sources {
            let source = crate::exact_storage_source(atom)?;
            budget.spend(
                source.path.len().saturating_add(1),
                "callable actual source path",
            )?;
        }
    }
    if contract.source != CallableReturnSource::Fresh {
        return super::forward::envelope(module, function, budget);
    }
    let definition = module
        .closures
        .get(contract.closure.0 as usize)
        .ok_or_else(|| bad("callable result refers to an absent closure"))?;
    budget.spend(
        definition
            .lending_captures
            .len()
            .saturating_add(contract.lending_captures.len()),
        "factory lending comparison",
    )?;
    if contract.lending_captures != definition.lending_captures {
        return Err(bad(
            "callable return lending differs from its concrete environment",
        ));
    }
    budget.spend(function.parameters.len(), "factory parameter envelope")?;
    let mut parameters_supported = true;
    let mut owned_input = None;
    for parameter in &function.parameters {
        let ty = &function.locals[parameter.0 as usize].ty;
        if let Type::Closure(id) = ty {
            if owned_input.replace(*id).is_some() {
                parameters_supported = false;
            }
            continue;
        }
        if let Type::OwnedDyn { interface } = ty {
            crate::dyn_cleanup::owner_interface(module, *interface, budget)?;
            continue;
        }
        if definition.kind == CallableKind::FnOnce
            && matches!(ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{..}))
        {
            continue;
        }
        if matches!(ty, Type::Address { .. } | Type::BorrowedStorage { .. })
            || ty.is_copy_value()
            || matches!(ty, Type::Managed(payload) if **payload == Type::ByteBuffer || payload.is_copy_value())
            || matches!(ty, Type::Package { .. })
        {
            continue;
        }
        if dynamic_store::is_owned_carrier(ty) {
            dynamic_store::owned_capture(module, ty, budget)?;
        } else {
            parameters_supported = false;
        }
    }
    if function.environment.is_some()
        || function.foreign.is_some()
        || function.callable_contract.is_some()
        || function.result_mode != PassingMode::Infer
        || function.return_type != Type::Closure(contract.closure)
        || definition.id != contract.closure
        || definition.kind != contract.kind
        || contract.kind == CallableKind::FunctionPointer
        || !parameters_supported
    {
        return Err(bad(
            "callable return contract requires an ordinary self-contained factory with its exact kind",
        ));
    }
    if !bindings.is_empty()
        || (budget.address_capture
            && definition
                .captures
                .iter()
                .any(|ty| matches!(ty, Type::Closure(_))))
        || definition.captures.iter().any(|ty| {
            matches!(
                ty,
                Type::Address { .. } | Type::Desc(_) | Type::BorrowedStorage { .. }
            )
        })
    {
        sources::check(module, function, budget)?;
        let expected = bindings
            .iter()
            .map(|row| match row.position.base {
                StorageBase::Capture(index) => Ok(index),
                _ => Err(bad(
                    "returned capture position is not an actual environment slot",
                )),
            })
            .collect::<CompileResult<BTreeSet<_>>>()?;
        let expected = expected.into_iter().collect::<Vec<_>>();
        if contract.hold != expected || contract.call != expected {
            return Err(bad("returned callable Hold/Call omits an external capture"));
        }
        let writes = expected
            .iter()
            .copied()
            .filter(|index| {
                matches!(
                    definition.captures[*index as usize],
                    Type::Address { mutable: true, .. } | Type::Desc(_)
                ) || matches!(&definition.captures[*index as usize], Type::BorrowedStorage { life, .. }
                    if life.slots.iter().any(|slot| matches!(slot.mode, zil_ir::StoredBorrowMode::ExclusiveAddress | zil_ir::StoredBorrowMode::WritableDescription)))
                || matches!(&definition.captures[*index as usize], Type::Managed(payload)
                    if matches!(payload.as_ref(),Type::Task {retained_life,..} if retained_life.ports.iter().any(|port| matches!(port.mode,zil_ir::StoredBorrowMode::ExclusiveAddress|zil_ir::StoredBorrowMode::WritableDescription))))
            })
            .collect::<Vec<_>>();
        if contract.writes != writes {
            return Err(bad(
                "returned callable write envelope differs from its actual captured permissions",
            ));
        }
    } else if !contract.hold.is_empty() || !contract.call.is_empty() || !contract.writes.is_empty()
    {
        return Err(bad(
            "self-contained callable result advertises external capture dependencies",
        ));
    }
    if let Some(input) = owned_input {
        owning_factory::check(module, function, input, definition, budget)?;
    }
    budget.spend(definition.captures.len(), "factory cleanup reconstruction")?;
    let mut cleanup = Vec::new();
    for (index, ty) in definition.captures.iter().enumerate() {
        if let Type::OwnedDyn { interface } = ty {
            crate::dyn_cleanup::owner_interface(module, *interface, budget)?;
            cleanup.push(index as u32);
            continue;
        }
        if definition.kind == CallableKind::FnOnce
            && matches!(ty,Type::Managed(payload) if matches!(payload.as_ref(),Type::Task{..}))
        {
            cleanup.push(index as u32);
            continue;
        }
        if matches!(ty, Type::Managed(payload) if **payload == Type::ByteBuffer || payload.is_copy_value())
            || matches!(ty, Type::Package { .. } | Type::Closure(_))
            || matches!(ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Package { .. }))
        {
            cleanup.push(index as u32);
        } else if dynamic_store::is_owned_carrier(ty) {
            dynamic_store::owned_capture(module, ty, budget)?;
            cleanup.push(index as u32);
        } else if matches!(
            ty,
            Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
        ) && budget.ordinary_store_sum_capture(definition.id, ty)?
        {
            if !resources::sums::admit(module, ty, budget)? {
                return Err(bad(
                    "callable Sum capture lacks a complete ordinary Store leaf",
                ));
            }
            cleanup.push(index as u32);
        } else if matches!(ty, Type::Address { .. } | Type::Desc(_)) {
            // The independently reconstructed source relation above, followed
            // by actual caller loans, authorizes only these borrowed leaves.
        } else if !ty.is_copy_value() {
            return Err(bad(
                "callable result captures an unsupported external carrier",
            ));
        }
    }
    budget.spend(
        cleanup.len().saturating_add(contract.cleanup.len()),
        "factory cleanup comparison",
    )?;
    if contract.cleanup != cleanup {
        return Err(bad(
            "callable return cleanup differs from owned capture indices",
        ));
    }
    let mut creations = 0usize;
    let mut returns = 0usize;
    for (block, live) in function.blocks.iter().zip(reachable) {
        budget.spend(1, "factory block")?;
        if !live {
            continue;
        }
        for instruction in &block.instructions {
            budget.spend(1, "factory value origin")?;
            if let InstructionKind::Call {
                function: callee, ..
            } = instruction.kind
            {
                let target = module
                    .functions
                    .get(callee.0 as usize)
                    .ok_or_else(|| bad("factory calls an absent function"))?;
                if target.callable_return_contract.as_ref().is_some_and(|c| {
                    c.closure == contract.closure && c.source == CallableReturnSource::Fresh
                }) {
                    creations += 1;
                }
            }
            let result_type = instruction
                .result
                .map(|v| {
                    function
                        .values
                        .get(v.0 as usize)
                        .ok_or_else(|| bad("factory instruction result outside value table"))
                })
                .transpose()?;
            if result_type == Some(&function.return_type) {
                match instruction.kind {
                    InstructionKind::MakeClosure { closure, .. } if closure == contract.closure => {
                        creations += 1
                    }
                    InstructionKind::Load(_)
                    | InstructionKind::TakeLocal { .. }
                    | InstructionKind::Call { .. } => {}
                    _ => {
                        return Err(bad(
                            "factory result is not derived from its concrete environment",
                        ));
                    }
                }
            }
        }
        if let Terminator::Return(value) = block.terminator {
            let value = value.ok_or_else(|| bad("callable factory returns no environment"))?;
            if function.values.get(value.0 as usize) != Some(&function.return_type) {
                return Err(bad(
                    "callable factory return differs from the hidden closure identity",
                ));
            }
            returns += 1;
        }
    }
    if creations == 0 || returns == 0 {
        return Err(bad(
            "callable factory requires a grounded environment creation or forwarding site and a returned identity",
        ));
    }
    Ok(())
}
