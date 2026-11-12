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
