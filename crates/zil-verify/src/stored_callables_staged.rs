//! Exact identity through finite owning staging, independently of producer facts.
//! This does not prove initialization, ownership, loans or once-consumption.
use super::*;

fn profile(module: &TypedModule, d: &ClosureDefinition, b: &mut Budget) -> Check<()> {
    b.spend(d.captures.len().saturating_add(1))?;
    for ty in &d.captures {
        super::super::stored_flow::type_work(ty, &b.limits.clone(), b)?;
    }
    let [Type::Managed(payload), Type::Address { mutable: false, inner }] =
        d.captures.as_slice()
    else {
        return Err(bad("staged callback requires one finite owner and shared Tag"));
    };
    let Type::BorrowedStorage { storage, life } = inner.as_ref() else {
        return Err(bad("staged callback capture lacks actual shared Tag storage"));
    };
    let Type::Record { fields, .. } = storage.as_ref() else {
        return Err(bad("staged callback capture requires a shared Tag record"));
    };
    if d.kind != CallableKind::FnOnce
        || d.storage_life.is_none()
        || !d.lending_captures.is_empty()
        || !scalar(&d.result)
        || !super::super::stored_owned::payload(payload)
        || life.binder_count != 1
        || !life.retained.is_empty()
        || !matches!(life.slots.as_slice(), [slot]
            if slot.binder == 0 && slot.path.as_slice() == [PlaceProjection::Field(0)]
                && slot.mode == StoredBorrowMode::SharedAddress)
        || !matches!(fields.as_slice(), [(_, Type::Address { mutable: false, inner })]
            if scalar(inner))
    {
        return Err(bad("staged callback is outside the finite owning Fixed profile"));
    }
    b.spend(module.resources.len())?;
    let mut found = false;
    for resource in &module.resources {
        super::super::stored_flow::type_work(payload, &b.limits.clone(), b)?;
        super::super::stored_flow::type_work(&resource.payload, &b.limits.clone(), b)?;
        if payload.as_ref() == &resource.payload {
            if found || resource.teardown.is_some() {
                return Err(bad("staged callback owner requires one teardown-free resource"));
            }
            found = true;
        }
    }
    if !found {
        return Err(bad("staged callback owner has no actual resource definition"));
    }
    Ok(())
}

// No local address can be exposed in this first fallback. In particular a
// WriteAddress cannot silently change the whole-local Store correspondence.
fn finite_instruction(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::ScopeEnter { .. }
            | InstructionKind::ScopeExit { .. }
            | InstructionKind::StorageLive(_)
            | InstructionKind::StorageDead(_)
            | InstructionKind::DropClosure { .. }
            | InstructionKind::Constant(_)
            | InstructionKind::Load(_)
            | InstructionKind::TakeLocal { .. }
            | InstructionKind::Store { .. }
            | InstructionKind::ReborrowAddress { mutable: false, .. }
            | InstructionKind::CallClosureOnce { .. }
            | InstructionKind::Unary { .. }
            | InstructionKind::Binary { .. }
            | InstructionKind::ConvertInteger { .. }
            | InstructionKind::ConvertNumber { .. }
    )
}

pub(super) fn parameter(
    module: &TypedModule,
    f: &TypedFunction,
    call: &InstructionKind,
    d: &ClosureDefinition,
    b: &mut Budget,
) -> Check<usize> {
    profile(module, d, b)?;
    b.spend(f.blocks.len().saturating_add(1))?;
    let [block] = f.blocks.as_slice() else {
        return Err(bad("staged callback requires one acyclic actual block"));
    };
    if f.entry != block.id || !matches!(block.terminator, Terminator::Return(_) | Terminator::Trap) {
        return Err(bad("staged callback requires an actual Return/Trap exit"));
    }
    let InstructionKind::CallClosureOnce { callee, .. } = call else {
        return Err(bad("staged callback requires an actual consuming call"));
    };
    b.spend(block.instructions.len())?;
    let mut before = None;
    for (index, instruction) in block.instructions.iter().enumerate() {
        if !finite_instruction(&instruction.kind) {
            return Err(bad("staged callback body exposes storage or unsupported effects"));
        }
        if std::ptr::eq(&instruction.kind, call) {
            before = Some(index);
        }
    }
    let mut before = before.ok_or_else(|| bad("staged callback call is not an actual instruction"))?;
    let mut current = *callee;
    // Every iteration decreases the actual instruction position. No recursive
    // walk, identity table, uncharged temporary clone or guessed Store order.
    for depth in 0..=f.values.len() {
        b.spend(1)?;
        if depth > b.limits.max_call_depth {
            return Err(limit("staged callback origin depth exhausted"));
        }
        if f.values.get(current.0 as usize) != Some(&Type::Closure(d.id)) {
            return Err(bad("staged callback SSA has a different actual environment"));
        }
        let mut definition = None;
        let mut use_count = 0usize;
        b.spend(block.instructions.len())?;
        for (index, instruction) in block.instructions.iter().enumerate() {
            if instruction.result == Some(current)
                && definition.replace((index, &instruction.kind)).is_some()
            {
                return Err(bad("staged callback SSA has duplicate definitions"));
            }
            for operand in instruction_operands(&instruction.kind) {
                b.spend(1)?;
                if operand == current {
                    use_count += 1;
                    if index != before {
                        return Err(bad("staged callback SSA escapes its unique transfer"));
                    }
                }
            }
        }
        b.spend(1)?;
        if use_count != 1 || matches!(block.terminator, Terminator::Return(Some(value)) if value == current) {
            return Err(bad("staged callback SSA lacks one exact consuming transfer"));
        }
        let Some((take_at, InstructionKind::TakeLocal { local })) = definition else {
            return Err(bad("staged callback requires TakeLocal, never a copied Load"));
        };
        if take_at >= before
            || f.locals.get(local.0 as usize).map(|local| &local.ty) != Some(&Type::Closure(d.id))
        {
            return Err(bad("staged callback transfer is cyclic or changes environment"));
        }
        b.spend(f.parameters.len())?;
        let mut parameter = None;
        for (position, actual) in f.parameters.iter().enumerate() {
            if actual == local && parameter.replace(position).is_some() {
                return Err(bad("staged callback parameter identity is duplicated"));
            }
        }
        let mut store = None;
        let mut takes = 0usize;
        b.spend(block.instructions.len())?;
        for (index, instruction) in block.instructions.iter().enumerate() {
            match instruction.kind {
                InstructionKind::Store { local: target, value } if target == *local => {
                    if store.replace((index, value)).is_some() {
                        return Err(bad("staged callback local has ambiguous or repeated Stores"));
                    }
                }
                InstructionKind::TakeLocal { local: target } if target == *local => takes += 1,
                InstructionKind::Load(target) if target == *local => {
                    return Err(bad("staged callback owner is also copied by Load"));
                }
                _ => {}
            }
        }
        if takes != 1 {
            return Err(bad("staged callback local has repeated consuming transfers"));
        }
        if let Some(position) = parameter {
            if store.is_some() {
                return Err(bad("staged callback parameter was rebound"));
            }
            return Ok(position);
        }
        let (store_at, source) = store.ok_or_else(|| bad("staged callback local has no unique Store"))?;
        if store_at >= take_at {
            return Err(bad("staged callback Store does not precede its Take"));
        }
        current = source;
        before = store_at;
    }
    Err(bad("staged callback origin did not reach an actual parameter"))
}
