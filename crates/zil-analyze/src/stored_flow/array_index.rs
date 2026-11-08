//! Dynamic selection keeps every physical candidate, never a container root.
use super::*;

pub(super) fn candidate(f: &TypedFunction, i: &Instruction) -> bool {
    let InstructionKind::ExtractIndex { aggregate, .. } = i.kind else {
        return false;
    };
    f.values
        .get(aggregate.0 as usize)
        .is_some_and(shared_array_value)
}

pub(super) fn types(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> CompileResult<()> {
    let InstructionKind::ExtractIndex { aggregate, index } = i.kind else {
        return Err(failure("dynamic Array consumer is not ExtractIndex"));
    };
    let actual = value_type(f, aggregate)?;
    if !shared_array_shape(actual, b)? {
        return Err(failure(
            "dynamic selection lacks an actual finite shared Array",
        ));
    }
    let Type::Array { element, .. } = actual else {
        unreachable!()
    };
    b.type_work(element, false, "dynamic Array exact result")?;
    if !value_type(f, index)?.is_integer()
        || i.result.map(|r| value_type(f, r)).transpose()? != Some(element.as_ref())
    {
        return Err(failure(
            "dynamic Array index or exact element result differs",
        ));
    }
    Ok(())
}

pub(super) fn transfer(
    f: &TypedFunction,
    i: &Instruction,
    input: &Value,
    b: &mut Budget,
) -> CompileResult<Permission> {
    types(f, i, b)?;
    let InstructionKind::ExtractIndex { aggregate, .. } = i.kind else {
        unreachable!()
    };
    let Type::Array { length, .. } = value_type(f, aggregate)? else {
        unreachable!()
    };
    fee(b, *length as usize)?;
    let mut result = Permission::default();
    for ordinal in 0..*length {
        let path = [PlaceProjection::Field(ordinal)];
        ordered(b, input.slots.len(), path.len())?;
        // Transfer can be bottom before the fixed point. Final windows check
        // each candidate separately rather than accepting a nonempty union.
        if let Some(permission) = input.slots.get(path.as_slice()) {
            union(&mut result, permission, b)?;
        }
    }
    Ok(result)
}

pub(super) fn check(
    f: &TypedFunction,
    i: &Instruction,
    input: &Value,
    active: &Loans,
    loans: &[Loan],
    apart: &mut BTreeSet<(Target, Target)>,
    b: &mut Budget,
) -> CompileResult<()> {
    let InstructionKind::ExtractIndex { aggregate, .. } = i.kind else {
        unreachable!()
    };
    let Type::Array { length, .. } = value_type(f, aggregate)? else {
        unreachable!()
    };
    fee(b, *length as usize)?;
    for ordinal in 0..*length {
        let path = [PlaceProjection::Field(ordinal)];
        ordered(b, input.slots.len(), path.len())?;
        let permission = input
            .slots
            .get(path.as_slice())
            .ok_or_else(|| failure("dynamic Array candidate leaf is absent"))?;
        if permission.targets.is_empty() {
            return Err(failure("dynamic Array candidate has no actual source"));
        }
        use_permission(permission, false, active, loans, apart, b)?;
    }
    Ok(())
}
