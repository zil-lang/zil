//! Protocol owner transport through actual tagged carriers and inline places.
//! These values carry existing roots; neither a projection nor a tag mints one.
use super::*;

pub(super) fn choice(world: &mut World<'_, '_>, mut values: Vec<Value>) -> R<Value> {
    world.pay(values.len())?;
    if values.is_empty() { return Err(bad("actual protocol carrier has no active alternative")); }
    if values.len() > world.limits.max_nodes {
        return Err(Diagnostic::limit(Phase::Types, "actual protocol carrier alternative limit exceeded", None));
    }
    if values.len() == 1 { Ok(values.remove(0)) } else { Ok(Value::Choice(values)) }
}

pub(super) fn variant(world: &mut World<'_, '_>, state: &State, tag: u32, fields: Vec<Value>) -> R<Value> {
    world.pay(state.phases.len().saturating_add(fields.len()).saturating_add(1))?;
    Ok(Value::Variant { tag, fields, phases: state.phases.clone(), fresh: true })
}

pub(super) fn tag(world: &mut World<'_, '_>, value: &Value) -> R<Value> {
    world.pay(value.work())?;
    let mut outcomes = Vec::new();
    fn collect(value: &Value, outcomes: &mut Vec<Outcome>, fresh: &mut bool) -> bool {
        match value {
            Value::Variant { tag, phases, fresh: current, .. } => {
                *fresh &= *current;
                outcomes.push(Outcome { tag: i64::from(*tag), phases: phases.clone() });
                true
            }
            Value::Choice(values) => values.iter().all(|value| collect(value, outcomes, fresh)),
            Value::Poll { fresh: current, outcomes: rows } => {
                *fresh &= *current;
                outcomes.extend(rows.iter().cloned());
                true
            }
            _ => false,
        }
    }
    let mut fresh = true;
    if collect(value, &mut outcomes, &mut fresh) { Ok(Value::Tag { fresh, outcomes }) }
    else { Ok(Value::Plain) }
}

/// A guarded take filters inactive alternatives and leaves a vacant subtree.
/// The ordinary actual CFG/operand checker independently checks the same path
/// and its active discriminator. No surviving sibling changes root or phase.
fn select(
    world: &mut World<'_, '_>, value: &mut Value, path: &[PlaceProjection], take: bool, depth: usize,
) -> R<Option<Value>> {
    world.pay(1)?;
    if depth > world.limits.max_call_depth {
        return Err(Diagnostic::limit(Phase::Types, "actual protocol projection depth exceeded", None));
    }
    if let Value::Choice(values) = value {
        let mut selected = Vec::new();
        let mut survivors = Vec::new();
        for mut alternative in std::mem::take(values) {
            if let Some(result) = select(world, &mut alternative, path, take, depth + 1)? {
                world.pay(2)?;
                selected.push(result);
                survivors.push(alternative);
            }
        }
        *values = survivors;
        return if selected.is_empty() { Ok(None) } else { choice(world, selected).map(Some) };
    }
    let Some((step, rest)) = path.split_first() else {
        if matches!(value, Value::Vacant) { return Err(bad("actual protocol projection uses an already moved subtree")); }
        world.pay(value.work())?;
        return Ok(Some(if take { std::mem::replace(value, Value::Vacant) } else { value.clone() }));
    };
    let child = match (step, value) {
        (PlaceProjection::Field(index), Value::Tuple(values)) => values.get_mut(*index as usize),
        (PlaceProjection::Variant(projection), Value::Variant { tag, fields, .. }) => {
            if projection.variant != *tag { return Ok(None); }
            fields.get_mut(projection.index as usize)
        }
        (PlaceProjection::ErrorArm(wanted), Value::Variant { tag, fields, .. }) => {
            if u32::from(*wanted) != *tag { return Ok(None); }
            fields.get_mut(0)
        }
        (_, Value::Plain) => return Ok(Some(Value::Plain)),
        (_, Value::Vacant) => return Err(bad("actual protocol projection crosses a moved subtree")),
        _ => None,
    }.ok_or_else(|| pending("actual protocol projection lacks its exact structured parent carrier"))?;
    select(world, child, rest, take, depth + 1)
}

pub(super) fn project(world: &mut World<'_, '_>, mut value: Value, path: &[PlaceProjection]) -> R<Value> {
    select(world, &mut value, path, false, 0)?
        .ok_or_else(|| bad("actual protocol projection selects an inactive carrier tag"))
}

pub(super) fn take_local(
    world: &mut World<'_, '_>, state: &mut State, local: LocalId, path: &[PlaceProjection],
) -> R<Value> {
    let value = state.locals.get_mut(local.0 as usize).and_then(Option::as_mut)
        .ok_or_else(|| bad("actual protocol take has no initialized local carrier"))?;
    select(world, value, path, true, 0)?
        .ok_or_else(|| bad("actual protocol take selects an inactive carrier tag"))
}

pub(super) fn replace_local(
    world: &mut World<'_, '_>, state: &mut State, local: LocalId, path: &[PlaceProjection], replacement: Value,
) -> R<Value> {
    fn replace(world: &mut World<'_, '_>, value: &mut Value, path: &[PlaceProjection], replacement: &Value, depth: usize) -> R<Value> {
        world.pay(1)?;
        if depth > world.limits.max_call_depth {
            return Err(Diagnostic::limit(Phase::Types, "actual protocol replacement depth exceeded", None));
        }
        let Some((step, rest)) = path.split_first() else {
            world.pay(replacement.work())?;
            return Ok(std::mem::replace(value, replacement.clone()));
        };
        let next = match (step, value) {
            (PlaceProjection::Field(index), Value::Tuple(fields)) => fields.get_mut(*index as usize),
            (PlaceProjection::Variant(projection), Value::Variant { tag, fields, .. }) if *tag == projection.variant => fields.get_mut(projection.index as usize),
            (PlaceProjection::ErrorArm(wanted), Value::Variant { tag, fields, .. }) if *tag == u32::from(*wanted) => fields.get_mut(0),
            _ => None,
        }.ok_or_else(|| pending("actual protocol replacement lacks its exact active parent storage"))?;
        replace(world, next, rest, replacement, depth + 1)
    }
    let target = state.locals.get_mut(local.0 as usize).and_then(Option::as_mut)
        .ok_or_else(|| bad("actual protocol replacement local is uninitialized"))?;
    replace(world, target, path, &replacement, 0)
}

/// Allocation failure returns every transferred input in declaration order.
/// Success instead moves those responsibilities into the actual Task frame.
/// The error discriminator carries the distinct phase relation on both edges.
pub(super) fn create_owned(
    world: &mut World<'_, '_>, state: &mut State, f: &TypedFunction, result: Option<ValueId>, arguments: &[ValueId],
) -> R<Value> {
    let id = result.ok_or_else(|| bad("actual owned Task creation has no result occurrence"))?;
    let ty = f.values.get(id.0 as usize).ok_or_else(|| bad("actual owned Task result type is absent"))?;
    let Type::ErrorUnion { error, .. } = ty else { return Err(bad("actual owned Task creation changes its complete result constructor")); };
    let mut physical = error.as_ref();
    while let Type::BorrowedStorage { storage, .. } = physical { world.pay(1)?; physical = storage; }
    let (Type::Record { name, fields } | Type::Package { name, fields }) = physical else {
        return Err(bad("actual owned Task failure has no original input carrier"));
    };
    world.pay(name.len().saturating_add(fields.len()))?;
    let [(reason, _), (inputs, _)] = fields.as_slice() else { return Err(bad("actual owned Task failure changes its field arity")); };
    if !name.starts_with("@std.task:v1:TaskCreateFailure:") || reason != "reason" || inputs != "inputs" {
        return Err(bad("actual owned Task failure changes its canonical input field coordinates"));
    }
    let values = operand_values(world, state, arguments)?;
    let mut roots = BTreeSet::new();
    for value in &values { world.pay(value.work())?; value.roots(&mut roots); }
    let failure = variant(world, state, 1, vec![Value::Tuple(vec![Value::Plain, Value::Tuple(values)])])?;
    world.pay(state.phases.len())?;
    let mut success_phases = state.phases.clone();
    for root in &roots {
        // A successful transfer retires the caller's owning occurrence. It is
        // not Closed and cannot be used as a reclaimed registration head.
        success_phases.insert(*root, RETIRED);
    }
    let success = Value::Variant { tag: 0, fields: vec![Value::Plain], phases: success_phases, fresh: true };
    for root in roots {
        let phase = state.phases.get(&root).copied().ok_or_else(|| bad("actual Task failure input lost its original root phase"))?;
        state.change(world, root, phase | RETIRED, false)?;
    }
    choice(world, vec![failure, success])
}
