//! An expected opaque signature is checked against the independently checked
//! actual closure domain. Extra actual free Fixed classes remain grounded by
//! the full environment, not by this Parameter/Result projection.
use super::*;

pub(super) fn check(
    f: &TypedFunction,
    claim: &CallableReturnContract,
    definition: &ClosureDefinition,
    budget: &mut Budget,
) -> Check<()> {
    let Some(expected) = &claim.life else { return Ok(()) };
    let actual = definition.storage_life.as_ref()
        .ok_or_else(|| bad(f, "expected callable life lacks its actual environment relation"))?;
    let work = storage_relation_metadata_work(actual, budget.limits.max_nodes,
        budget.limits.max_call_depth, budget.remaining)
        .ok_or_else(|| limit("actual callable return relation exceeds metadata bounds"))?;
    budget.spend(work)?;
    if expected.fixed_binder_count != actual.fixed_binder_count
        || expected.invocation_binder_count != actual.invocation_binder_count
    {
        return Err(bad(f, "expected callable life omits or invents an actual class domain"));
    }
    let count = expected.invocation_binder_count as usize;
    budget.spend(count.saturating_mul(2).saturating_add(2))?;
    let mut required = vec![None; count];
    let mut supplied = vec![None; count];
    let mut binder = |a: StorageBinder, b: StorageBinder| -> Check<()> {
        match (a, b) {
            (StorageBinder::Fixed(a), StorageBinder::Fixed(b)) if a == b => Ok(()),
            (StorageBinder::Invocation(a), StorageBinder::Invocation(b)) => {
                let left = required.get_mut(a as usize)
                    .ok_or_else(|| bad(f, "expected Invocation class exceeds its actual domain"))?;
                let right = supplied.get_mut(b as usize)
                    .ok_or_else(|| bad(f, "actual Invocation class exceeds its declared domain"))?;
                if left.is_some_and(|old| old != b) || right.is_some_and(|old| old != a) {
                    return Err(bad(f, "expected callable life merges distinct invocation inputs"));
                }
                *left = Some(b);
                *right = Some(a);
                Ok(())
            }
            _ => Err(bad(f, "expected callable life changes a rigid actual Fixed class")),
        }
    };
    // Actual capture rows are checked elsewhere with the actual capture types,
    // entry CFG and factory result sources. They cannot be supplied by a bound.
    let mut borrows = expected.bindings.iter();
    for row in &actual.bindings {
        budget.spend(1)?;
        if matches!(row.position.base, StorageBase::Capture(_)) { continue; }
        let expected = borrows.next()
            .ok_or_else(|| bad(f, "expected callable life omits an actual signature borrow"))?;
        budget.spend(expected.position.path.len().saturating_add(row.position.path.len()).saturating_add(2))?;
        if expected.position != row.position {
            return Err(bad(f, "expected callable life changes an actual signature borrow position"));
        }
        binder(expected.binder, row.binder)?;
    }
    if borrows.next().is_some() {
        return Err(bad(f, "expected callable life fabricates a signature borrow"));
    }
    let mut classes = expected.classes.iter();
    for row in &actual.classes {
        budget.spend(1)?;
        if matches!(row.position.base, StorageBase::Capture(_)) { continue; }
        let expected = classes.next()
            .ok_or_else(|| bad(f, "expected callable life omits an actual signature class"))?;
        budget.spend(expected.position.path.len().saturating_add(row.position.path.len()).saturating_add(3))?;
        if expected.position != row.position {
            return Err(bad(f, "expected callable life changes an actual signature class position"));
        }
        binder(expected.binder, row.binder)?;
    }
    if classes.next().is_some() {
        return Err(bad(f, "expected callable life fabricates a signature class"));
    }
    // The actual relation is canonicalized in the full environment. A raw
    // expected relation must use that same namespace, not a second numbering.
    budget.spend(count.saturating_mul(2))?;
    if required.iter().enumerate().any(|(i, row)| *row != Some(i as u32))
        || supplied.iter().enumerate().any(|(i, row)| *row != Some(i as u32))
    {
        return Err(bad(f, "expected callable life is not in the complete actual Invocation namespace"));
    }
    Ok(())
}
