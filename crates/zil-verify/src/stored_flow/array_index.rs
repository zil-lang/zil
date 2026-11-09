//! Independently recover all real alternatives of a dynamic Array selection.
use super::*;

pub(super) fn candidate(f: &TypedFunction, i: &Instruction) -> bool {
    match i.kind {
        InstructionKind::ExtractIndex { aggregate, .. } => f
            .values
            .get(aggregate.0 as usize)
            .is_some_and(shared_array_value),
        _ => false,
    }
}

pub(super) fn types(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<()> {
    let InstructionKind::ExtractIndex { aggregate, index } = i.kind else {
        return Err(err("shared Array selection has no dynamic extraction"));
    };
    let actual = vt(f, aggregate)?;
    if !shared_array_shape(actual, b)? {
        return Err(err(
            "dynamic extraction requires a finite actual shared Array",
        ));
    }
    let Type::Array { element, .. } = actual else {
        unreachable!()
    };
    type_work(element, &b.limits.clone(), b)?;
    if !vt(f, index)?.is_integer()
        || i.result.map(|r| vt(f, r)).transpose()? != Some(element.as_ref())
    {
        return Err(err(
            "dynamic Array selection index or actual element result is invalid",
        ));
    }
    Ok(())
}

pub(super) fn transfer(
    f: &TypedFunction,
    i: &Instruction,
    input: &Payload,
    b: &mut Budget,
) -> Check<Permission> {
    types(f, i, b)?;
    let InstructionKind::ExtractIndex { aggregate, .. } = i.kind else {
        unreachable!()
    };
    let Type::Array { length, .. } = vt(f, aggregate)? else {
        unreachable!()
    };
    b.spend(*length as usize)?;
    let mut result = Permission::default();
    for ordinal in 0..*length {
        if let Some(permission) = leaf(input, &[PlaceProjection::Field(ordinal)], b)? {
            join_permission(&mut result, permission, b)?;
        }
    }
    Ok(result)
}

pub(super) fn check(
    f: &TypedFunction,
    i: &Instruction,
    input: &Payload,
    active: &[usize],
    loans: &[Loan],
    requirements: &mut Vec<(Source, Source)>,
    b: &mut Budget,
) -> Check<()> {
    let InstructionKind::ExtractIndex { aggregate, .. } = i.kind else {
        unreachable!()
    };
    let Type::Array { length, .. } = vt(f, aggregate)? else {
        unreachable!()
    };
    b.spend(*length as usize)?;
    for ordinal in 0..*length {
        let permission = leaf(input, &[PlaceProjection::Field(ordinal)], b)?
            .ok_or_else(|| err("dynamic Array physical candidate is missing"))?;
        if permission.sources.is_empty() {
            return Err(err("dynamic Array physical candidate has no actual source"));
        }
        access(permission, false, active, loans, requirements, b)?;
    }
    Ok(())
}
