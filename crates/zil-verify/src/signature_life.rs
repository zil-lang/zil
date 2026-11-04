//! Independent canonical semantic-class checking. Rows do not supply roots.
use super::*;

pub(super) fn charge(c: &StorageLifeContract, b: &mut Budget) -> Check<()> {
    let n = c.bindings.len().saturating_add(c.classes.len());
    b.spend(
        n.saturating_mul(n)
            .saturating_add(n.saturating_mul(2))
            .saturating_add(2)
            .saturating_add(c.fixed_binder_count as usize)
            .saturating_add(c.invocation_binder_count as usize),
    )?;
    for r in &c.bindings {
        b.spend(r.position.path.len())?;
    }
    for r in &c.classes {
        b.spend(r.position.path.len())?;
    }
    Ok(())
}

pub(super) fn outer(c: &StorageLifeContract, base: StorageBase) -> Option<StorageBinder> {
    c.bindings
        .iter()
        .find(|r| r.position.base == base && r.position.path.is_empty())
        .map(|r| r.binder)
}
pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
) -> Check<()> {
    relation_inner(parameters, result, c, span, false)
}
pub(super) fn function_relation(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
) -> Check<()> {
    relation_inner(parameters, result, c, span, allow_callback)
}
fn relation_inner(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
) -> Check<()> {
    let invalid = || {
        reject(
            "E-VERIFY-SIGNATURE-CLASS",
            span,
            "incomplete, noncanonical or ungrounded signature class relation",
        )
    };
    if c.fixed_binder_count == 0 && c.invocation_binder_count == 0 {
        return Err(invalid());
    }
    let rows = c
        .bindings
        .len()
        .checked_add(c.classes.len())
        .ok_or_else(invalid)?;
    let domains = (c.fixed_binder_count as usize)
        .checked_add(c.invocation_binder_count as usize)
        .ok_or_else(invalid)?;
    if domains > rows || c.fixed_binder_count as usize > c.classes.len() {
        return Err(invalid());
    }
    let input_classes = c
        .classes
        .iter()
        .filter(|r| matches!(r.position.base, StorageBase::Parameter(_)))
        .count();
    if c.fixed_binder_count as usize > input_classes {
        return Err(invalid());
    }
    let mut borrow = 0usize;
    let mut class = 0usize;
    let mut next = [0u32; 2];
    let mut inputs = [0u32; 2];
    let mut grounded = vec![false; c.fixed_binder_count as usize];
    for (ordinal, ty) in parameters.iter().chain(std::iter::once(result)).enumerate() {
        let output = ordinal == parameters.len();
        if output {
            inputs = next;
        }
        let base = if output {
            StorageBase::Result
        } else {
            StorageBase::Parameter(u32::try_from(ordinal).map_err(|_| invalid())?)
        };
        let mut visit = |binder: StorageBinder, retained: bool| -> Check<()> {
            let (domain, id, count) = match binder {
                StorageBinder::Fixed(i) => (0, i, c.fixed_binder_count),
                StorageBinder::Invocation(i) => (1, i, c.invocation_binder_count),
            };
            if id >= count || id > next[domain] || (output && id >= inputs[domain]) {
                return Err(invalid());
            }
            if id == next[domain] {
                next[domain] += 1;
            }
            if retained && domain == 0 {
                grounded[id as usize] = true;
            }
            Ok(())
        };
        let (actual, pointee) = match ty {
            Type::Address { inner, .. } => {
                let row = c.bindings.get(borrow).ok_or_else(invalid)?;
                if row.position.base != base || !row.position.path.is_empty() {
                    return Err(invalid());
                }
                borrow += 1;
                visit(row.binder, false)?;
                (inner.as_ref(), true)
            }
            _ => (ty, false),
        };
        match actual {
            Type::BorrowedStorage { storage, life } => {
                if !matches!(storage.as_ref(), Type::Tuple(_) | Type::Record { .. })
                    || !life.retained.is_empty()
                {
                    return Err(unsupported(
                        "signature owning/nested carrier consumer is not implemented",
                        span,
                    ));
                }
                if life.binder_count == 0
                    || life.binder_count as usize > c.classes.len().saturating_sub(class)
                {
                    return Err(invalid());
                }
                for local in 0..life.binder_count {
                    let row = &c.classes[class];
                    let path = if pointee {
                        row.position.path.as_slice() == [LifeTypeProjection::Pointee]
                    } else {
                        row.position.path.is_empty()
                    };
                    if row.position.base != base || row.position.class != local || !path {
                        return Err(invalid());
                    }
                    class += 1;
                    visit(row.binder, !output)?;
                }
            }
            t if pointee => {
                if !(t.is_address_pointee() || allow_callback && matches!(t, Type::Closure(_))) {
                    return Err(invalid());
                }
            }
            t => {
                if !(t.is_copy_value() || allow_callback && !output && matches!(t, Type::Closure(_))) {
                    return Err(invalid());
                }
            }
        }
    }
    if borrow != c.bindings.len()
        || class != c.classes.len()
        || next != [c.fixed_binder_count, c.invocation_binder_count]
        || grounded.contains(&false)
    {
        return Err(invalid());
    }
    if let Type::Address { mutable, .. } = result {
        let upper = outer(c, StorageBase::Result);
        let mut possible = false;
        for (i, p) in parameters.iter().enumerate() {
            possible |= matches!(p,Type::Address{mutable:m,..}if !mutable||*m)
                && outer(c, StorageBase::Parameter(i as u32)) == upper;
        }
        if !mutable {
            possible |= c.classes.iter().any(|r| {
                matches!(r.position.base, StorageBase::Parameter(_)) && Some(r.binder) == upper
            });
        }
        if !possible {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn entails(required: &StorageLifeContract, supplied: &StorageLifeContract) -> bool {
    if required.bindings.len() != supplied.bindings.len()
        || required.classes.len() != supplied.classes.len()
    {
        return false;
    }
    for (a, b) in required.bindings.iter().zip(&supplied.bindings) {
        if a.position != b.position {
            return false;
        }
    }
    for (a, b) in required.classes.iter().zip(&supplied.classes) {
        if a.position != b.position {
            return false;
        }
    }
    let count = required.bindings.len() + required.classes.len();
    let pair = |i: usize| -> (StorageBinder, StorageBinder) {
        if i < required.bindings.len() {
            (required.bindings[i].binder, supplied.bindings[i].binder)
        } else {
            let j = i - required.bindings.len();
            (required.classes[j].binder, supplied.classes[j].binder)
        }
    };
    for i in 0..count {
        let (a, x) = pair(i);
        for j in 0..i {
            let (b, y) = pair(j);
            if a == b && x != y {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn impossible_fixed_domain_never_allocates_a_claimed_grounding_vector() {
        let relation_data = StorageLifeContract {
            fixed_binder_count: u32::MAX,
            invocation_binder_count: 0,
            bindings: vec![],
            classes: vec![],
        };
        assert!(relation(&[], &Type::I32, &relation_data, None).is_err());
    }
}
