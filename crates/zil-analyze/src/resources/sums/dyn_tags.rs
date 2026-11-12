//! Current actual tag proofs for canonical owning-dyn outcomes. This reuses
//! Analyze's ordinary projection/epoch kernel; it grants no owner or Copy fact.
use super::*;
use partial_places::{Meta, State};

fn canonical(ty: &Type) -> bool {
    let Type::ErrorUnion { error, result } = ty else {
        return false;
    };
    if !matches!(result.as_ref(), Type::OwnedDyn { .. }) {
        return false;
    }
    matches!(error.as_ref(), Type::Enum {name,variants}
        if name == "@std.dyn:v1:DynCreateError"
            && matches!(variants.as_slice(), [(case, fields)] if case == "Allocation" && fields.is_empty()))
}
fn copy(state: &State, b: &mut Budget) -> CompileResult<State> {
    state.cost(b)?;
    Ok(state.clone())
}
fn input(
    f: &TypedFunction,
    block: usize,
    preds: &[Vec<usize>],
    reachable: &[bool],
    outputs: &[Option<State>],
    meta: &Meta,
    b: &mut Budget,
) -> CompileResult<Option<State>> {
    let mut state = (block == f.entry.0 as usize).then(State::default);
    for predecessor in &preds[block] {
        b.spend(1, "dyn outcome actual predecessor")?;
        if !reachable[*predecessor] {
            continue;
        }
        let Some(previous) = &outputs[*predecessor] else {
            continue;
        };
        let mut previous = copy(previous, b)?;
        meta.edge(&mut previous, &f.blocks[*predecessor].terminator, block, b)?;
        if let Some(state) = &mut state {
            state.join(&previous, b)?;
        } else {
            state = Some(previous);
        }
    }
    Ok(state)
}
fn step(
    meta: &Meta,
    state: &mut State,
    i: &Instruction,
    constructors: &BTreeMap<ValueId, bool>,
    locals: &BTreeSet<LocalId>,
    b: &mut Budget,
) -> CompileResult<()> {
    meta.step(state, i, b)?;
    if let InstructionKind::Store { local, value } = i.kind {
        b.lookup(locals.len(), "dyn outcome constructor destination")?;
        if locals.contains(&local) {
            b.lookup(constructors.len(), "dyn outcome actual constructor")?;
            if let Some(is_error) = constructors.get(&value) {
                state.constructed(local, *is_error, b)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn check(
    f: &TypedFunction,
    preds: &[Vec<usize>],
    reachable: &[bool],
    b: &mut Budget,
) -> CompileResult<()> {
    if b.dyn_interfaces.is_empty() {
        return Ok(());
    }
    let mut locals = BTreeSet::new();
    b.spend(f.locals.len(), "dyn outcome local inventory")?;
    for local in &f.locals {
        b.spend(40, "dyn outcome exact canonical type")?;
        if canonical(&local.ty) {
            locals.insert(local.id);
        }
    }
    if locals.is_empty() {
        return Ok(());
    }
    let mut constructors = BTreeMap::new();
    for instruction in f.blocks.iter().flat_map(|block| &block.instructions) {
        b.spend(1, "dyn outcome constructor inventory")?;
        if let (Some(value), InstructionKind::MakeErrorUnion { is_error, .. }) =
            (instruction.result, &instruction.kind)
        {
            let ty = f
                .values
                .get(value.0 as usize)
                .ok_or_else(|| error("dyn outcome constructor result is absent"))?;
            if canonical(ty) {
                b.lookup(constructors.len(), "dyn outcome constructor index")?;
                constructors.insert(value, *is_error);
            }
        }
    }
    let meta = Meta::new(f, b)?;
    b.spend(
        f.blocks.len().saturating_mul(2),
        "dyn outcome graph state reservation",
    )?;
    let mut outputs = vec![None; f.blocks.len()];
    loop {
        let mut changed = false;
        for (block_index, block) in f.blocks.iter().enumerate() {
            b.spend(1, "dyn outcome actual graph visit")?;
            if !reachable[block_index] {
                continue;
            }
            let Some(mut state) = input(f, block_index, preds, reachable, &outputs, &meta, b)?
            else {
                continue;
            };
            for instruction in &block.instructions {
                step(&meta, &mut state, instruction, &constructors, &locals, b)?;
            }
            state.cost(b)?;
            if let Some(previous) = &outputs[block_index] {
                previous.cost(b)?;
            }
            if outputs[block_index].as_ref() != Some(&state) {
                outputs[block_index] = Some(state);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for (block_index, block) in f.blocks.iter().enumerate() {
        b.spend(1, "dyn outcome guarded projection check")?;
        if !reachable[block_index] {
            continue;
        }
        let mut state = input(f, block_index, preds, reachable, &outputs, &meta, b)?
            .ok_or_else(|| error("owning dyn outcome block has no actual predecessor proof"))?;
        for instruction in &block.instructions {
            if let InstructionKind::TakeVariantField { local, path } = &instruction.kind {
                b.lookup(locals.len(), "dyn outcome selective source identity")?;
                if locals.contains(local) {
                    state.require(*local, path, b)?;
                }
            }
            step(&meta, &mut state, instruction, &constructors, &locals, b)?;
        }
    }
    Ok(())
}
