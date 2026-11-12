//! Context envelopes are complete qualified types, never provider authority.
use super::*;

pub(super) fn validate(
    interface: &DynInterface,
    modules: &[&CoreModule],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let fail = || {
        unsupported(
            Span::default(),
            "dyn Context differs from its complete qualified port envelope",
        )
    };
    let Some(context) = &interface.context else {
        if interface.retained_life.binder_count != 0
            || !interface.retained_life.ports.is_empty()
            || !interface.retained_life.retained.is_empty()
            || !interface.retained_life.nominal_arguments.is_empty()
        {
            return Err(fail());
        }
        return Ok(());
    };
    inference::charge_type_snapshot(context, Span::default(), limits, left, 4)?;
    stored::shape(context, left, limits.max_call_depth, Span::default())?;
    let Type::BorrowedStorage { storage, life } = context else {
        return Err(fail());
    };
    let mut matches = 0usize;
    for module in modules {
        *left = left
            .checked_sub(module.life_instances.len())
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "dyn Context identity work exceeded", None)
            })?;
        for instance in &module.life_instances {
            inference::charge_type_snapshot(&instance.ty, Span::default(), limits, left, 2)?;
            if instance.ty == *context {
                matches += 1;
            }
        }
    }
    if matches != 1
        || interface.retained_life.binder_count != life.binder_count
        || interface.retained_life.retained != life.retained
        || interface.retained_life.nominal_arguments != life.nominal_arguments
    {
        return Err(fail());
    }
    *left = left
        .checked_sub(life.slots.len())
        .ok_or_else(|| Diagnostic::limit(Phase::Types, "dyn Context port work exceeded", None))?;
    let mut leaves: Vec<(&Type, StoredBorrowMode, u32)> = Vec::new();
    for slot in &life.slots {
        let mut leaf = storage.as_ref();
        for step in &slot.path {
            *left = left.checked_sub(1).ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "dyn Context path work exceeded", None)
            })?;
            if let Type::BorrowedStorage { storage, .. } = leaf {
                leaf = storage;
            }
            leaf = match (leaf, step) {
                (
                    Type::Record { fields, .. } | Type::Package { fields, .. },
                    PlaceProjection::Field(i),
                ) => fields.get(*i as usize).map(|(_, t)| t),
                (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(*i as usize),
                (Type::Array { element, length }, PlaceProjection::Field(i)) if *i < *length => {
                    Some(element.as_ref())
                }
                (Type::Enum { variants, .. }, PlaceProjection::Variant(p)) => variants
                    .get(p.variant as usize)
                    .and_then(|(_, f)| f.get(p.index as usize)),
                (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
                    Some(if *e { error.as_ref() } else { result.as_ref() })
                }
                _ => None,
            }
            .ok_or_else(fail)?;
        }
        let mut duplicate = false;
        for (prior, mode, binder) in &leaves {
            inference::charge_type_snapshot(prior, Span::default(), limits, left, 2)?;
            inference::charge_type_snapshot(leaf, Span::default(), limits, left, 2)?;
            if *prior == leaf && *mode == slot.mode && *binder == slot.binder {
                duplicate = true;
                break;
            }
        }
        if !duplicate {
            leaves.push((leaf, slot.mode, slot.binder));
        }
    }
    if leaves.len() != interface.retained_life.ports.len() {
        return Err(fail());
    }
    for ((leaf, mode, binder), port) in leaves.into_iter().zip(&interface.retained_life.ports) {
        inference::charge_type_snapshot(&port.leaf, Span::default(), limits, left, 2)?;
        if leaf != &port.leaf || mode != port.mode || binder != port.binder {
            return Err(fail());
        }
    }
    Ok(())
}

/// Check every concrete borrowed Self leaf against the typed public upper.
/// This static relation supplies no runtime roots; each constructor must also
/// transport actual operands and satisfy the complete Self/Context equations.
#[allow(clippy::too_many_arguments)]
pub(super) fn provider(
    self_type: &Type,
    classes: &[StorageBinder],
    rows: &[DynRetainedPortBinding],
    interface: &DynInterface,
    modules: &[&CoreModule],
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    let fail = || {
        unsupported(
            Span::default(),
            "dyn provider does not cover its complete Self borrowing and class relation",
        )
    };
    let pay = |left: &mut usize, n: usize| -> R<()> {
        *left = left.checked_sub(n).ok_or_else(|| {
            Diagnostic::limit(Phase::Types, "dyn provider relation work exceeded", None)
        })?;
        Ok(())
    };
    inference::charge_type_snapshot(self_type, Span::default(), limits, left, 4)?;
    let (storage, life) = match self_type {
        Type::BorrowedStorage { storage, life } => {
            stored::shape(self_type, left, limits.max_call_depth, Span::default())?;
            let mut matches = 0usize;
            for module in modules {
                pay(left, module.life_instances.len())?;
                for instance in &module.life_instances {
                    inference::charge_type_snapshot(
                        &instance.ty,
                        Span::default(),
                        limits,
                        left,
                        2,
                    )?;
                    if instance.ty == *self_type {
                        matches += 1;
                    }
                }
            }
            if matches != 1 {
                return Err(fail());
            }
            (storage.as_ref(), Some(life))
        }
        Type::Record { .. } | Type::Package { .. } => {
            if crate::dyn_dispatch::closed_owner_shape(self_type, left, 0, limits)?.is_none() {
                return Err(fail());
            }
            (self_type, None)
        }
        _ => return Err(fail()),
    };
    let count = life.map_or(0, |life| life.binder_count as usize);
    if classes.len() > limits.max_nodes || rows.len() > limits.max_nodes {
        return Err(fail());
    }
    pay(
        left,
        classes
            .len()
            .saturating_add(rows.len())
            .saturating_add(count),
    )?;
    if classes.len() != count || rows.len() != interface.retained_life.ports.len() {
        return Err(fail());
    }
    for class in classes {
        if !matches!(class, StorageBinder::Fixed(i) if *i < interface.retained_life.binder_count) {
            return Err(fail());
        }
    }
    let mut covered = std::collections::BTreeSet::new();
    for (port, (row, upper)) in rows.iter().zip(&interface.retained_life.ports).enumerate() {
        if row.port as usize != port || row.sources.len() > limits.max_nodes {
            return Err(fail());
        }
        pay(left, row.sources.len())?;
        for source in &row.sources {
            if source.base != StorageBase::Result || source.path.len() > limits.max_call_depth {
                return Err(fail());
            }
            pay(
                left,
                source
                    .path
                    .len()
                    .saturating_mul(3)
                    .saturating_add(covered.len())
                    .saturating_add(1),
            )?;
            let mut path = Vec::with_capacity(source.path.len());
            for part in &source.path {
                let ContractProjection::Place(part) = part else {
                    return Err(fail());
                };
                path.push(*part);
            }
            if !covered.insert(path.clone()) {
                return Err(fail());
            }
            let life = life.ok_or_else(fail)?;
            pay(
                left,
                life.slots
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
            )?;
            let slot = life
                .slots
                .iter()
                .find(|slot| slot.path == path)
                .ok_or_else(fail)?;
            if slot.mode != upper.mode
                || classes.get(slot.binder as usize) != Some(&StorageBinder::Fixed(upper.binder))
            {
                return Err(fail());
            }
            let mut leaf = storage;
            for selector in &path {
                if let Type::BorrowedStorage { storage, .. } = leaf {
                    leaf = storage;
                }
                leaf = match (leaf, selector) {
                    (
                        Type::Record { fields, .. } | Type::Package { fields, .. },
                        PlaceProjection::Field(i),
                    ) => fields.get(*i as usize).map(|(_, t)| t),
                    (Type::Tuple(fields), PlaceProjection::Field(i)) => fields.get(*i as usize),
                    (Type::Array { element, length }, PlaceProjection::Field(i))
                        if *i < *length =>
                    {
                        Some(element.as_ref())
                    }
                    (Type::Enum { variants, .. }, PlaceProjection::Variant(p)) => variants
                        .get(p.variant as usize)
                        .and_then(|(_, f)| f.get(p.index as usize)),
                    (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(e)) => {
                        Some(if *e { error.as_ref() } else { result.as_ref() })
                    }
                    _ => None,
                }
                .ok_or_else(fail)?;
            }
            inference::charge_type_snapshot(leaf, Span::default(), limits, left, 2)?;
            inference::charge_type_snapshot(&upper.leaf, Span::default(), limits, left, 2)?;
            if leaf != &upper.leaf {
                return Err(fail());
            }
        }
    }
    if covered.len() != life.map_or(0, |life| life.slots.len()) {
        return Err(fail());
    }
    Ok(())
}
