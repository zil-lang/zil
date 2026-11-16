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
#[derive(Default)]
struct Guards { rows: Vec<Outcome>, incomplete: bool }
fn select(
    world: &mut World<'_, '_>, value: &mut Value, path: &[PlaceProjection], take: bool, guards: &mut Guards, covered: bool, depth: usize,
) -> R<Option<Value>> {
    world.pay(1)?;
    if depth > world.limits.max_call_depth {
        return Err(Diagnostic::limit(Phase::Types, "actual protocol projection depth exceeded", None));
    }
    if let Value::Choice(values) = value {
        let mut selected = Vec::new();
        let mut survivors = Vec::new();
        for mut alternative in std::mem::take(values) {
            if let Some(result) = select(world, &mut alternative, path, take, guards, covered, depth + 1)? {
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
        if !covered { guards.incomplete = true; }
        return Ok(Some(if take { std::mem::replace(value, Value::Vacant) } else { value.clone() }));
    };
    let mut covered = covered;
    if let (PlaceProjection::Variant(projection), Value::Poll { fresh, outcomes }) = (step, &mut *value) {
        // Conditional protocol Polls carry only the independently checked
        // scalar payload. Their Ready projection still narrows the same root
        // phases; it never reconstructs an owning payload from a tag.
        if projection.variant != 1 || projection.index != 0 || !rest.is_empty() {
            return Err(bad("actual protocol Poll projection changes its canonical Ready payload"));
        }
        let mut active = false;
        for outcome in outcomes.iter().filter(|outcome| outcome.tag == i64::from(projection.variant)) {
            world.pay(outcome.phases.len().saturating_add(1))?;
            if *fresh { guards.rows.push(outcome.clone()); }
            else { guards.incomplete = true; }
            active = true;
        }
        return Ok(active.then_some(Value::Plain));
    }
    let child = match (step, value) {
        (PlaceProjection::Field(index), Value::Tuple(values)) => values.get_mut(*index as usize),
        (PlaceProjection::Variant(projection), Value::Variant { tag, fields, phases, fresh }) => {
            if projection.variant != *tag { return Ok(None); }
            if *fresh { world.pay(phases.len().saturating_add(1))?; guards.rows.push(Outcome { tag: i64::from(*tag), phases: phases.clone() }); covered = true; }
            else { guards.incomplete = true; }
            fields.get_mut(projection.index as usize)
        }
        (PlaceProjection::ErrorArm(wanted), Value::Variant { tag, fields, phases, fresh }) => {
            if u32::from(*wanted) != *tag { return Ok(None); }
            if *fresh { world.pay(phases.len().saturating_add(1))?; guards.rows.push(Outcome { tag: i64::from(*tag), phases: phases.clone() }); covered = true; }
            else { guards.incomplete = true; }
            fields.get_mut(0)
        }
        (_, Value::Plain) => { guards.incomplete = true; return Ok(Some(Value::Plain)); },
        (_, Value::Vacant) => return Err(bad("actual protocol projection crosses a moved subtree")),
        _ => None,
    }.ok_or_else(|| pending("actual protocol projection lacks its exact structured parent carrier"))?;
    select(world, child, rest, take, guards, covered, depth + 1)
}

pub(super) fn project(world: &mut World<'_, '_>, mut value: Value, path: &[PlaceProjection]) -> R<Value> {
    select(world, &mut value, path, false, &mut Guards::default(), false, 0)?
        .ok_or_else(|| bad("actual protocol projection selects an inactive carrier tag"))
}

pub(super) fn take_local(
    world: &mut World<'_, '_>, state: &mut State, local: LocalId, path: &[PlaceProjection],
) -> R<Value> {
    let value = state.locals.get_mut(local.0 as usize).and_then(Option::as_mut)
        .ok_or_else(|| bad("actual protocol take has no initialized local carrier"))?;
    let mut guards = Guards::default();
    let selected = select(world, value, path, true, &mut guards, false, 0)?
        .ok_or_else(|| bad("actual protocol take selects an inactive carrier tag"))?;
    refine(world, state, &guards)?;
    Ok(selected)
}

pub(super) fn take_value(
    world: &mut World<'_, '_>, state: &mut State, id: ValueId, path: &[PlaceProjection],
) -> R<Value> {
    let value = state.values.get_mut(id.0 as usize).and_then(Option::as_mut)
        .ok_or_else(|| bad("actual protocol take has no dominating SSA carrier"))?;
    let mut guards = Guards::default();
    let selected = select(world, value, path, true, &mut guards, false, 0)?
        .ok_or_else(|| bad("actual protocol take selects an inactive SSA carrier tag"))?;
    refine(world, state, &guards)?;
    Ok(selected)
}

fn refine(world: &mut World<'_, '_>, state: &mut State, guards: &Guards) -> R<()> {
    if guards.incomplete || guards.rows.is_empty() { return Ok(()); }
    for (root, current) in &mut state.phases {
        let mut allowed = 0;
        let mut complete = true;
        for guard in &guards.rows {
            world.pay(guard.phases.len().saturating_add(1).ilog2() as usize + 1)?;
            if let Some(phase) = guard.phases.get(root) { allowed |= *phase; }
            else { complete = false; }
        }
        if complete {
            *current &= allowed;
            if *current == 0 { return Err(bad("actual protocol guarded take contradicts its current root generation")); }
        }
    }
    Ok(())
}

pub(super) fn borrowed(world: &mut World<'_, '_>, mut value: Value) -> R<Value> {
    fn demote(world: &mut World<'_, '_>, value: &mut Value, depth: usize) -> R<()> {
        world.pay(1)?;
        if depth > world.limits.max_call_depth { return Err(Diagnostic::limit(Phase::Types, "actual protocol view depth exceeded", None)); }
        match value {
            Value::Root(_, _, owns, _) => *owns = false,
            Value::Tuple(values) | Value::Choice(values) | Value::Variant { fields: values, .. } => {
                for value in values { demote(world, value, depth + 1)?; }
            }
            _ => {}
        }
        Ok(())
    }
    demote(world, &mut value, 0)?;
    Ok(value)
}

pub(super) fn tuple_arity(world: &mut World<'_, '_>, value: &Value, wanted: usize, depth: usize) -> R<()> {
    world.pay(1)?;
    if depth > world.limits.max_call_depth { return Err(Diagnostic::limit(Phase::Types, "actual protocol tuple depth exceeded", None)); }
    match value {
        Value::Tuple(fields) if fields.len() == wanted => Ok(()),
        Value::Choice(alternatives) => {
            for value in alternatives { tuple_arity(world, value, wanted, depth + 1)?; }
            Ok(())
        }
        Value::Plain => Ok(()),
        _ => Err(bad("actual protocol tuple destructuring changes its complete initialized carrier")),
    }
}

pub(super) fn copy_projection(world: &mut World<'_, '_>, value: Value) -> R<Value> {
    fn copyable(world: &mut World<'_, '_>, value: &Value) -> R<()> {
        world.pay(1)?;
        match value {
            Value::Root(_, _, true, _) => return Err(bad("actual protocol Copy projection duplicates an owning root")),
            Value::Tuple(values) | Value::Choice(values) | Value::Variant { fields: values, .. } => {
                for value in values { copyable(world, value)?; }
            }
            _ => {}
        }
        Ok(())
    }
    copyable(world, &value)?;
    Ok(value)
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
    let mut input_type = &fields[1].1;
    while let Type::BorrowedStorage { storage, .. } = input_type { world.pay(1)?; input_type = storage; }
    if !matches!(input_type, Type::Tuple(types) if types.len() == arguments.len()) {
        return Err(bad("actual owned Task failure omits complete input tuple positions"));
    }
    let values = operand_values(world, state, arguments)?;
    fn merge(world: &mut World<'_, '_>, into: &mut BTreeMap<Root, bool>, from: BTreeMap<Root, bool>) -> R<()> {
        for (root, owns) in from {
            world.pay(into.len().saturating_add(1).ilog2() as usize + 1)?;
            match into.entry(root) {
                std::collections::btree_map::Entry::Occupied(previous) => {
                    if owns || *previous.get() { return Err(bad("actual Task inputs repeat an exclusive owning root")); }
                }
                std::collections::btree_map::Entry::Vacant(entry) => {
                    world.pay(1)?;
                    entry.insert(owns);
                }
            }
        }
        Ok(())
    }
    fn transferred(world: &mut World<'_, '_>, state: &State, value: &Value, depth: usize) -> R<BTreeMap<Root, bool>> {
        world.pay(1)?;
        if depth > world.limits.max_call_depth { return Err(Diagnostic::limit(Phase::Types, "actual Task input root depth exceeded", None)); }
        let mut result = BTreeMap::new();
        match value {
            Value::Root(root, true, owns, None) => {
                world.pay(state.phases.len().saturating_add(1).ilog2() as usize + 1)?;
                state.root(value)?;
                result.insert(*root, *owns);
            }
            Value::Root(..) | Value::Vacant => return Err(bad("actual owned Task input is not its live current root generation")),
            Value::Tuple(fields) | Value::Variant { fields, .. } => {
                for field in fields {
                    let current = transferred(world, state, field, depth + 1)?;
                    merge(world, &mut result, current)?;
                }
            }
            Value::Choice(alternatives) => {
                let mut uniform: Option<BTreeMap<Root, bool>> = None;
                for value in alternatives {
                    let current = transferred(world, state, value, depth + 1)?;
                    if let Some(previous) = &uniform {
                        world.pay(1)?;
                        let mut same = previous.len() == current.len();
                        if same {
                            for (expected, actual) in previous.iter().zip(&current) {
                                world.pay(1)?;
                                if expected != actual { same = false; break; }
                            }
                        }
                        if !same { return Err(pending("actual Task input alternatives need their complete uniform root ownership relation")); }
                    } else { uniform = Some(current); }
                }
                result = uniform.ok_or_else(|| bad("actual Task input has no initialized alternative"))?;
            }
            _ => {}
        }
        Ok(result)
    }
    let mut roots = BTreeMap::new();
    for value in &values {
        let current = transferred(world, state, value, 0)?;
        merge(world, &mut roots, current)?;
    }
    let failure = variant(world, state, 1, vec![Value::Tuple(vec![Value::Plain, Value::Tuple(values)])])?;
    world.pay(state.phases.len())?;
    let mut success_phases = state.phases.clone();
    for (root, owns) in &roots {
        // A successful transfer retires the caller's owning occurrence. It is
        // not Closed and cannot be used as a reclaimed registration head.
        world.pay(success_phases.len().saturating_add(1).ilog2() as usize + 1)?;
        success_phases.insert(*root, if *owns { RETIRED } else { UNKNOWN });
    }
    let success = Value::Variant { tag: 0, fields: vec![Value::Plain], phases: success_phases, fresh: true };
    for (root, owns) in roots {
        world.pay(state.phases.len().saturating_add(1).ilog2() as usize + 1)?;
        let phase = state.phases.get(&root).copied().ok_or_else(|| bad("actual Task failure input lost its original root phase"))?;
        state.change(world, root, phase | if owns { RETIRED } else { UNKNOWN }, false)?;
    }
    choice(world, vec![failure, success])
}
