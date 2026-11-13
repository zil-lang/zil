//! Context envelopes are complete qualified types, never provider authority.
use super::*;

type ClosedTypeQuery = fn(&Type, &TypedModule, &mut usize, Span, usize, usize) -> R<bool>;
fn scope_work(left: &mut usize, n: usize, span: Span) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "dynamic environment scope work exceeded",
            Some(span),
        )
    })?;
    Ok(())
}

/// The complete public environment directory is distinct from cleanup debt.
/// This proposes only Known0; unknown or nonzero child inventories stay false.
pub(super) fn closed_scope(
    module: &TypedModule,
    id: DynInterfaceId,
    left: &mut usize,
    span: Span,
    depth: usize,
    limit: usize,
    closed: ClosedTypeQuery,
) -> R<bool> {
    scope_work(left, module.dyn_interfaces.len().saturating_add(1), span)?;
    if depth > limit {
        return Err(Diagnostic::limit(
            Phase::Types,
            "dynamic environment scope depth exceeded",
            Some(span),
        ));
    }
    let mut matching = module
        .dyn_interfaces
        .iter()
        .filter(|interface| interface.id == id);
    let interface = matching
        .next()
        .ok_or_else(|| unsupported(span, "dynamic environment has no actual interface"))?;
    if matching.next().is_some() {
        return Err(unsupported(
            span,
            "dynamic environment interface identity is ambiguous",
        ));
    }
    let life = &interface.retained_life;
    scope_work(
        left,
        interface
            .inherited
            .len()
            .saturating_add(interface.associated_types.len())
            .saturating_add(interface.associated_constants.len())
            .saturating_add(interface.slots.len()),
        span,
    )?;
    if life.binder_count != 0
        || !life.ports.is_empty()
        || !life.retained.is_empty()
        || !life.nominal_arguments.is_empty()
    {
        return Ok(false);
    }
    if let Some(context) = &interface.context {
        let Type::BorrowedStorage { life, .. } = context else {
            return Ok(false);
        };
        if life.binder_count != 0
            || !life.slots.is_empty()
            || !life.retained.is_empty()
            || !life.nominal_arguments.is_empty()
        {
            return Ok(false);
        }
        let limits = Limits {
            max_call_depth: limit,
            ..Limits::default()
        };
        inference::charge_type_snapshot(context, span, &limits, left, 2)?;
        let mut matches = 0usize;
        for instance in &module.life_instances {
            inference::charge_type_snapshot(&instance.ty, span, &limits, left, 2)?;
            if instance.ty == *context {
                matches += 1;
            }
        }
        if matches != 1 || !closed(context, module, left, span, depth + 1, limit)? {
            return Ok(false);
        }
    }
    // A closed Context cannot erase a phantom trait argument or associated
    // result. Method-local Invocation variables are checked in their own scope.
    for application in std::iter::once(&interface.application).chain(&interface.inherited) {
        scope_work(left, application.actuals.len(), span)?;
        for actual in &application.actuals {
            match actual {
                DynGenericActual::Type(ty) => {
                    if !closed(ty, module, left, span, depth + 1, limit)? {
                        return Ok(false);
                    }
                }
                DynGenericActual::Const(value) => {
                    if !closed(&value.ty(), module, left, span, depth + 1, limit)? {
                        return Ok(false);
                    }
                }
                DynGenericActual::QualifiedType(_) | DynGenericActual::Life(_) => return Ok(false),
            }
        }
    }
    for associated in &interface.associated_types {
        if associated.life.binder_count != 0
            || !associated.life.slots.is_empty()
            || !associated.life.retained.is_empty()
            || !associated.life.nominal_arguments.is_empty()
            || !associated.classes.is_empty()
            || !closed(&associated.ty, module, left, span, depth + 1, limit)?
        {
            return Ok(false);
        }
    }
    for associated in &interface.associated_constants {
        if !closed(&associated.ty, module, left, span, depth + 1, limit)? {
            return Ok(false);
        }
    }
    for slot in &interface.slots {
        scope_work(left, slot.actuals.len(), span)?;
        let Some(scope) = &slot.storage_life else {
            return Ok(false);
        };
        if scope.fixed_binder_count != 0 {
            return Ok(false);
        }
        for actual in &slot.actuals {
            match actual {
                DynGenericActual::Type(ty) => {
                    if !closed(ty, module, left, span, depth + 1, limit)? {
                        return Ok(false);
                    }
                }
                DynGenericActual::Const(value) => {
                    if !closed(&value.ty(), module, left, span, depth + 1, limit)? {
                        return Ok(false);
                    }
                }
                DynGenericActual::Life(StorageBinder::Invocation(i))
                    if *i < scope.invocation_binder_count => {}
                _ => return Ok(false),
            }
        }
    }
    Ok(true)
}

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
