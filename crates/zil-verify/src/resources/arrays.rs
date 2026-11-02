//! Untrusted owning-array types and operations are admitted independently here.
//! Slot responsibility and provenance are reconstructed by the resource checker.
use super::*;

pub(crate) fn is_owned_type(ty: &Type) -> bool {
    matches!(ty, Type::Array { element, .. } if matches!(element.as_ref(), Type::Package { .. } | Type::Closure(_)))
}

pub(crate) fn check_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    let Type::Array { element, length } = ty else {
        return Err(reject(
            "E-VERIFY-OWNED-ARRAY",
            span,
            "operation requires an owning array",
        ));
    };
    if *length == 0 {
        return Err(unsupported(
            "owning arrays require a nonzero fixed length",
            span,
        ));
    }
    match element.as_ref() {
        Type::Package { .. } => {
            if packages::has_callable_fields(element) {
                return Err(unsupported(
                    "callback State arrays require a separate nested-field protocol",
                    span,
                ));
            }
            let element_work = if packages::has_array_fields(element) {
                packages::layout_work(element, budget)?
            } else {
                closures::type_layout_work(element, budget)?
            };
            let work = element_work
                .checked_mul(*length as usize)
                .ok_or_else(|| limit("owning array layout expansion overflow"))?;
            budget.spend(work)?;
            packages::check_type_for(module, element, span, budget)
        }
        Type::Closure(id) => {
            closures::definition(module, *id, span)?;
            closures::old_container_signature(module, *id, span, budget)?;
            budget.spend(*length as usize)?;
            // The global independent closure-layout pass also charges the
            // length multiplied by fully expanded inline child work.
            closures::self_contained(module, *id, budget)
        }
        _ => Err(unsupported(
            "owning arrays require finite Package or self-contained Closure elements",
            span,
        )),
    }
}

/// The existing lowering for a mutable owning closure local uses one private
/// array cell. Preserve the old public/container gate, then admit only this
/// grounded local carrier when its actual callback signature is the closed
/// owning-data profile. Function parameters and results still call the old
/// `check_type_for` directly.
pub(crate) fn check_internal_single_slot_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    let rejected = match check_type_for(module, ty, span, budget) {
        Ok(()) => return Ok(()),
        Err(rejected) => rejected,
    };
    if rejected.category != zil_base::ResultCategory::Unsupported {
        return Err(rejected);
    }
    let Type::Array { element, length: 1 } = ty else {
        return Err(rejected);
    };
    let Type::Closure(id) = element.as_ref() else {
        return Err(rejected);
    };
    let definition = closures::definition(module, *id, span)?;
    if !owned_callbacks::signature(module, definition, budget)? {
        return Err(rejected);
    }
    budget.spend(1)?;
    closures::self_contained(module, *id, budget)
}

fn owning_slot(module: &TypedModule, ty: &Type, budget: &mut Budget) -> Check<Option<ClosureId>> {
    let Type::Array { element, length: 1 } = ty else {
        return Ok(None);
    };
    let Type::Closure(id) = element.as_ref() else {
        return Ok(None);
    };
    let entry = closures::definition(module, *id, None)?;
    budget.spend(entry.parameters.len().saturating_add(1))?;
    for ty in entry
        .parameters
        .iter()
        .chain(std::iter::once(&entry.result))
    {
        let work = closures::type_layout_work(ty, budget)?;
        budget.spend(work)?;
    }
    Ok(
        (entry.parameters.iter().any(owned_callbacks::root)
            || owned_callbacks::root(&entry.result))
        .then_some(*id),
    )
}

fn reaches(edges: &[Vec<usize>], source: usize, target: usize, budget: &mut Budget) -> Check<bool> {
    budget.spend(edges.len().saturating_mul(2))?;
    let mut visited = vec![false; edges.len()];
    let mut pending = vec![source];
    visited[source] = true;
    while let Some(node) = pending.pop() {
        budget.spend(edges[node].len().saturating_add(1))?;
        if node == target {
            return Ok(true);
        }
        for &next in &edges[node] {
            if !visited[next] {
                visited[next] = true;
                pending.push(next);
            }
        }
    }
    Ok(false)
}

/// An ordinary caller may use the already checked single-slot lowering only
/// when a real Parameter-return call transfers that slot's element out. The
/// returned instance may move to another local or slot. The per-instruction
/// resource/borrow passes still prove ownership, live flags and old loans.
/// A forwarding function instead receives a separate path-sensitive proof in
/// retained::provenance. No module-level marker can grant this qualification.
pub(crate) fn check_internal_slot_uses(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    budget: &mut Budget,
) -> Check<()> {
    budget.spend(f.locals.len().saturating_add(f.values.len()))?;
    let mut slots = Vec::new();
    for local in &f.locals {
        if let Some(id) = owning_slot(module, &local.ty, budget)? {
            slots.push((local.id, id));
        }
    }
    let mut array_values = Vec::new();
    for (index, ty) in f.values.iter().enumerate() {
        if owning_slot(module, ty, budget)?.is_some() {
            array_values.push(ValueId(index as u32));
        }
    }
    if slots.is_empty() && array_values.is_empty() {
        return Ok(());
    }
    if f.foreign.is_some() || f.environment.is_some() {
        return Err(unsupported(
            "owning callback internal slot requires an ordinary checked caller",
            Some(f.span),
        ));
    }
    let width = f
        .locals
        .len()
        .checked_add(f.values.len())
        .ok_or_else(|| limit("internal callback slot graph width overflow"))?;
    let instructions = f.blocks.iter().map(|b| b.instructions.len()).sum::<usize>();
    budget.spend(width.saturating_add(instructions.saturating_mul(3)))?;
    let at = |value: ValueId| f.locals.len() + value.0 as usize;
    let mut edges = vec![Vec::new(); width];
    let mut calls = Vec::new();
    let mut refills = Vec::new();
    let mut takes = Vec::new();
    let mut stores = Vec::new();
    let mut constructions = Vec::new();
    for (index, block) in f.blocks.iter().enumerate() {
        if !graph.reachable[index] {
            continue;
        }
        for instruction in &block.instructions {
            budget.spend(1)?;
            match &instruction.kind {
                InstructionKind::Store { local, value } => {
                    edges[at(*value)].push(local.0 as usize);
                    stores.push((*local, *value));
                }
                InstructionKind::MakeOwnedArray { elements }
                    if elements.len() == 1 && instruction.result.is_some() =>
                {
                    constructions.push((elements[0], instruction.result.unwrap()));
                }
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    if let Some(value) = instruction.result {
                        edges[local.0 as usize].push(at(value));
                    }
                }
                InstructionKind::TakeOwnedArrayElement { local, index: 0 } => {
                    if let Some(value) = instruction.result {
                        takes.push((*local, value));
                    }
                }
                InstructionKind::StoreOwnedArrayElement {
                    local,
                    index: 0,
                    value,
                } => {
                    refills.push((*local, *value));
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let target = &module.functions[function.0 as usize];
                    if let Some(contract) = &target.callable_return_contract
                        && let CallableReturnSource::Parameter(position) = contract.source
                        && let (Some(actual), Some(result)) =
                            (arguments.get(position as usize), instruction.result)
                    {
                        calls.push((*actual, result));
                    }
                }
                _ => {}
            }
        }
    }
    let forwarding = f
        .callable_return_contract
        .as_ref()
        .is_some_and(|contract| matches!(contract.source, CallableReturnSource::Parameter(_)));
    budget.spend(
        slots.len().saturating_mul(
            takes
                .len()
                .saturating_add(refills.len())
                .saturating_add(calls.len())
                .saturating_add(constructions.len())
                .saturating_add(1),
        ),
    )?;
    for &(slot, id) in &slots {
        if forwarding && f.return_type == Type::Closure(id) {
            let CallableReturnSource::Parameter(position) =
                f.callable_return_contract.as_ref().unwrap().source
            else {
                unreachable!()
            };
            let formal = f.parameters.get(position as usize).ok_or_else(|| {
                reject(
                    "E-VERIFY-OWNED-ARRAY",
                    Some(f.span),
                    "forwarded slot has no actual formal",
                )
            })?;
            if takes.iter().any(|(local, _)| *local == slot) {
                let mut actual = false;
                for &(element, array) in &constructions {
                    if f.values[array.0 as usize] != f.locals[slot.0 as usize].ty
                        || !reaches(&edges, formal.0 as usize, at(element), budget)?
                    {
                        continue;
                    }
                    for &(local, value) in &stores {
                        if local == slot && reaches(&edges, at(array), at(value), budget)? {
                            actual = true;
                            break;
                        }
                    }
                    if actual {
                        break;
                    }
                }
                if actual {
                    continue; // retained::provenance checks every path and return.
                }
            }
            let mut direct_refill = false;
            for &(local, value) in &refills {
                if local == slot && reaches(&edges, formal.0 as usize, at(value), budget)? {
                    direct_refill = true;
                    break;
                }
            }
            if direct_refill && takes.iter().any(|(local, _)| *local == slot) {
                continue;
            }
            if let Err(old_gate) =
                closures::old_container_signature(module, id, Some(f.span), budget)
            {
                return Err(old_gate);
            }
            return Err(unsupported(
                "forwarded F array is not its actual internal parameter slot",
                Some(f.span),
            ));
        }
        let mut qualified = false;
        for &(taken_local, taken) in &takes {
            if taken_local != slot {
                continue;
            }
            for &(actual, _) in &calls {
                if !reaches(&edges, at(taken), at(actual), budget)? {
                    continue;
                }
                qualified = true;
                break;
            }
            if qualified {
                break;
            }
        }
        if !qualified {
            if let Err(old_gate) =
                closures::old_container_signature(module, id, Some(f.span), budget)
            {
                return Err(old_gate);
            }
            return Err(unsupported(
                "owning callback array is not an internal slot of an actual Parameter-return call",
                Some(f.span),
            ));
        }
    }
    for value in array_values {
        let mut stored = false;
        for &(local, actual) in &stores {
            if slots.iter().any(|(slot, _)| *slot == local)
                && reaches(&edges, at(value), at(actual), budget)?
            {
                stored = true;
                break;
            }
        }
        if !stored {
            if let Type::Array { element, .. } = &f.values[value.0 as usize]
                && let Type::Closure(id) = element.as_ref()
                && let Err(old_gate) =
                    closures::old_container_signature(module, *id, Some(f.span), budget)
            {
                return Err(old_gate);
            }
            return Err(unsupported(
                "owning callback array value must enter its checked internal slot",
                Some(f.span),
            ));
        }
    }
    Ok(())
}

pub(crate) fn check_instruction(
    module: &TypedModule,
    function: &TypedFunction,
    instruction: &Instruction,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(instruction.span);
    let bad = |message| reject("E-VERIFY-OWNED-ARRAY", span, message);
    if let InstructionKind::MakeOwnedArray { elements } = &instruction.kind {
        let result = instruction
            .result
            .ok_or_else(|| bad("owning array construction needs a result"))?;
        let ty = value_type(function, result, span)?;
        check_internal_single_slot_type_for(module, ty, span, budget)?;
        let Type::Array { element, length } = ty else {
            unreachable!()
        };
        if elements.len() != *length as usize {
            return Err(bad(
                "owning array construction must provide every element exactly once",
            ));
        }
        budget.spend(elements.len())?;
        for value in elements {
            charge_payload(element, budget)?;
            if value_type(function, *value, span)? != element.as_ref() {
                return Err(bad("owning array construction element type differs"));
            }
        }
        return Ok(Some(ty.clone()));
    }
    let (local, index) = match instruction.kind {
        InstructionKind::ProjectOwnedArrayField { local, index, .. }
        | InstructionKind::TakeOwnedArrayElement { local, index }
        | InstructionKind::DropOwnedArrayElement { local, index }
        | InstructionKind::StoreOwnedArrayElement { local, index, .. }
        | InstructionKind::BorrowOwnedArrayElement { local, index, .. } => (local, Some(index)),
        InstructionKind::DropOwnedArray { local } => (local, None),
        _ => unreachable!("only owning array operations enter this checker"),
    };
    let ty = local_type(function, local, span)?;
    check_internal_single_slot_type_for(module, ty, span, budget)?;
    let Type::Array { element, length } = ty else {
        unreachable!()
    };
    if index.is_some_and(|index| index >= *length) {
        return Err(bad(
            "owning array element index is outside the declared fixed length",
        ));
    }
    match &instruction.kind {
        InstructionKind::ProjectOwnedArrayField { field, .. } => {
            let Type::Package { fields, .. } = element.as_ref() else {
                return Err(bad("field projection requires an actual Package element"));
            };
            let (_, ty) = fields.get(*field as usize).ok_or_else(|| {
                bad("owning array field index is outside its Package declaration")
            })?;
            charge_payload(ty, budget)?;
            Ok(Some(match ty {
                Type::Managed(payload) => Type::Desc(payload.clone()),
                ty => {
                    check_type(ty, span)?;
                    ty.clone()
                }
            }))
        }
        InstructionKind::TakeOwnedArrayElement { .. } => Ok(Some(element.as_ref().clone())),
        InstructionKind::StoreOwnedArrayElement { value, .. } => {
            if !function.locals[local.0 as usize].mutable {
                return Err(reject(
                    "E-VERIFY-MUTABILITY",
                    span,
                    "owning array refill requires mutable storage",
                ));
            }
            charge_payload(element, budget)?;
            if value_type(function, *value, span)? != element.as_ref() {
                return Err(bad(
                    "owning array refill input type differs from its element type",
                ));
            }
            Ok(None)
        }
        InstructionKind::DropOwnedArrayElement { .. } | InstructionKind::DropOwnedArray { .. } => {
            Ok(None)
        }
        InstructionKind::BorrowOwnedArrayElement { mutable, .. } => {
            if !matches!(element.as_ref(), Type::Closure(_)) {
                return Err(bad(
                    "environment borrowing requires a concrete Closure array element",
                ));
            }
            if *mutable && !function.locals[local.0 as usize].mutable {
                return Err(reject(
                    "E-VERIFY-MUTABILITY",
                    span,
                    "exclusive callback array borrow requires mutable storage",
                ));
            }
            Ok(Some(Type::Address {
                mutable: *mutable,
                inner: element.clone(),
            }))
        }
        _ => unreachable!("owning array local operation"),
    }
}
