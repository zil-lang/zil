//! Independent canonical semantic-class checking. Rows do not supply roots.
use super::*;

// Only the actual type graph can establish a value-only Array. Its nested
// qualified instances are checked independently in module preflight.
fn pure_array_value(ty: &Type) -> bool {
    if !matches!(ty, Type::Array { .. }) {
        return false;
    }
    pure_value_tree(ty)
}

fn pure_nominal_value(ty: &Type) -> bool {
    matches!(ty, Type::BorrowedStorage { life, .. } if life.slots.is_empty()) && pure_value_tree(ty)
}

// Each qualified child of an ordinary Tuple has its own local class domain.
// The enclosing signature must name every domain at its physical Field path.
// Admission of the actual nominal instances and all stored leaves is a
// separate module/body proof; this only selects the shared value directory.
fn scoped_compound(ty: &Type) -> bool {
    scoped_compound_inner(ty, false)
}
fn scoped_compound_inner(ty: &Type, qualified: bool) -> bool {
    match ty {
        Type::BorrowedStorage { storage, life } => {
            life.slots.iter().all(|slot| {
                matches!(
                    slot.mode,
                    StoredBorrowMode::SharedAddress | StoredBorrowMode::ExclusiveAddress
                )
            }) && scoped_compound_inner(storage, true)
        }
        Type::Address { .. } => qualified,
        Type::Managed(payload) => payload.is_copy_value(),
        Type::Tuple(fields) => fields
            .iter()
            .all(|field| scoped_compound_inner(field, qualified)),
        Type::Record { fields, .. } => fields
            .iter()
            .all(|(_, field)| scoped_compound_inner(field, qualified)),
        Type::Array { element, .. } => scoped_compound_inner(element, qualified),
        Type::Enum { variants, .. } => variants.iter().all(|(_, fields)| {
            fields
                .iter()
                .all(|field| scoped_compound_inner(field, qualified))
        }),
        Type::ErrorUnion { error, result } => {
            scoped_compound_inner(error, qualified) && scoped_compound_inner(result, qualified)
        }
        other => other.is_copy_value(),
    }
}

fn pure_value_tree(ty: &Type) -> bool {
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        match current {
            Type::BorrowedStorage { storage, life } if life.slots.is_empty() => {
                pending.push(storage)
            }
            Type::Array { element, .. } => pending.push(element),
            Type::Tuple(fields) => pending.extend(fields),
            Type::Record { fields, .. } => pending.extend(fields.iter().map(|(_, t)| t)),
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    pending.extend(fields);
                }
            }
            Type::Unit
            | Type::Bool
            | Type::I32
            | Type::I8
            | Type::I16
            | Type::Isize
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::F32
            | Type::F64 => {}
            _ => return false,
        }
    }
    true
}

fn array_classes(
    ty: &Type,
    path: &mut Vec<LifeTypeProjection>,
    out: &mut Vec<(Vec<LifeTypeProjection>, u32)>,
) {
    match ty {
        Type::BorrowedStorage { life, .. } => {
            for class in 0..life.binder_count {
                out.push((path.clone(), class));
            }
        }
        Type::ErrorUnion { error, result } => {
            for (arm, child) in [(true, error.as_ref()), (false, result.as_ref())] {
                path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(arm)));
                array_classes(child, path, out);
                path.pop();
            }
        }
        Type::Array { element, .. } => {
            path.push(LifeTypeProjection::Element);
            array_classes(element, path, out);
            path.pop();
        }
        Type::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                array_classes(field, path, out);
                path.pop();
            }
        }
        Type::Record { fields, .. } => {
            for (index, (_, field)) in fields.iter().enumerate() {
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(
                    index as u32,
                )));
                array_classes(field, path, out);
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            for (variant, (_, fields)) in variants.iter().enumerate() {
                for (index, field) in fields.iter().enumerate() {
                    path.push(LifeTypeProjection::Place(PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        },
                    )));
                    array_classes(field, path, out);
                    path.pop();
                }
            }
        }
        _ => {}
    }
}

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
    relation_inner(parameters, result, c, span, false, None, None, false)
}
pub(super) fn function_relation(
    module: &TypedModule,
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
) -> Check<()> {
    function_relation_inner(module, parameters, result, c, span, allow_callback, false)
}
pub(super) fn function_relation_byte_inputs(
    module: &TypedModule,
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
) -> Check<()> {
    function_relation_inner(module, parameters, result, c, span, allow_callback, true)
}
fn function_relation_inner(
    module: &TypedModule,
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
    allow_byte_input: bool,
) -> Check<()> {
    if let Type::ErrorUnion {
        result: success, ..
    } = result
        && let Type::Managed(payload) = success.as_ref()
        && let Type::Task { retained_life, .. } = payload.as_ref()
        && !retained_life.ports.is_empty()
    {
        super::tasks::require_closed_context(payload, span)?;
        let inputs = parameters
            .iter()
            .enumerate()
            .filter(|(_, ty)| matches!(ty, Type::Address { .. }))
            .collect::<Vec<_>>();
        if inputs.len() != retained_life.ports.len() {
            return Err(reject(
                "E-VERIFY-SIGNATURE-CLASS",
                span,
                "borrowed Task signature input count differs from actual ports",
            ));
        }
        let mut expected_bindings = Vec::new();
        for (port, (input, ty)) in inputs.iter().enumerate() {
            if *ty != &retained_life.ports[port].leaf {
                return Err(reject(
                    "E-VERIFY-SIGNATURE-CLASS",
                    span,
                    "borrowed Task signature input type differs from actual port",
                ));
            }
            expected_bindings.push(StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Parameter(*input as u32),
                    path: vec![],
                },
                binder: StorageBinder::Invocation(retained_life.ports[port].binder),
            });
        }
        for (port, row) in retained_life.ports.iter().enumerate() {
            expected_bindings.push(StorageLifeBinding {
                position: StoragePosition {
                    base: StorageBase::Result,
                    path: vec![
                        ContractProjection::Place(PlaceProjection::ErrorArm(false)),
                        ContractProjection::RetainedPort(port as u32),
                    ],
                },
                binder: StorageBinder::Invocation(row.binder),
            });
        }
        let mut expected_classes = Vec::new();
        for class in 0..retained_life.binder_count {
            expected_classes.push(SignatureLifeClassBinding {
                position: SignatureLifeClass {
                    base: StorageBase::Result,
                    path: vec![
                        LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)),
                        LifeTypeProjection::Pointee,
                    ],
                    class,
                },
                binder: StorageBinder::Invocation(class),
            });
        }
        if c.fixed_binder_count != 0
            || c.invocation_binder_count != retained_life.binder_count
            || c.bindings != expected_bindings
            || c.classes != expected_classes
        {
            return Err(reject(
                "E-VERIFY-SIGNATURE-CLASS",
                span,
                "borrowed Task signature has no exact actual input port relation",
            ));
        }
        return Ok(());
    }
    relation_inner(
        parameters,
        result,
        c,
        span,
        allow_callback,
        None,
        Some(module),
        allow_byte_input,
    )
}
pub(super) fn closure_relation(
    module: &TypedModule,
    parameters: &[Type],
    captures: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
) -> Check<()> {
    relation_inner(
        parameters,
        result,
        c,
        span,
        true,
        Some(captures),
        Some(module),
        false,
    )
}
#[allow(clippy::too_many_arguments)]
fn relation_inner(
    parameters: &[Type],
    result: &Type,
    c: &StorageLifeContract,
    span: Option<Span>,
    allow_callback: bool,
    captures: Option<&[Type]>,
    module: Option<&TypedModule>,
    allow_byte_input: bool,
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
    if domains > rows || captures.is_none() && c.fixed_binder_count as usize > c.classes.len() {
        return Err(invalid());
    }
    let input_classes = c
        .classes
        .iter()
        .filter(|r| matches!(r.position.base, StorageBase::Parameter(_)))
        .count();
    if captures.is_none() && c.fixed_binder_count as usize > input_classes {
        return Err(invalid());
    }
    let array_output_upper = match result {
        Type::Address { mutable: false, .. } => c
            .bindings
            .last()
            .filter(|row| row.position.base == StorageBase::Result && row.position.path.is_empty())
            .map(|row| row.binder),
        _ => None,
    };
    let mut array_result_input = false;
    let mut borrow = 0usize;
    let mut class = 0usize;
    let mut next = [0u32; 2];
    let mut inputs = [0u32; 2];
    let mut grounded = vec![false; c.fixed_binder_count as usize];
    let capture_types = captures.unwrap_or(&[]);
    for (ordinal, ty) in parameters
        .iter()
        .chain(capture_types)
        .chain(std::iter::once(result))
        .enumerate()
    {
        let output = ordinal == parameters.len() + capture_types.len();
        let capture = !output && ordinal >= parameters.len();
        if output {
            inputs = next;
        }
        let base = if output {
            StorageBase::Result
        } else if capture {
            StorageBase::Capture(u32::try_from(ordinal - parameters.len()).map_err(|_| invalid())?)
        } else {
            StorageBase::Parameter(u32::try_from(ordinal).map_err(|_| invalid())?)
        };
        let mut visit = |binder: StorageBinder, retained: bool| -> Check<()> {
            if capture && matches!(binder, StorageBinder::Invocation(_)) {
                return Err(invalid());
            }
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
                visit(row.binder, capture)?;
                (inner.as_ref(), true)
            }
            _ => (ty, false),
        };
        if super::stored_flow::shared_array_value(actual) {
            if capture {
                return Err(invalid());
            }
            let Type::Array { length, .. } = actual else {
                unreachable!()
            };
            for index in 0..*length {
                let row = c.bindings.get(borrow).ok_or_else(invalid)?;
                if row.position.base != base
                    || row.position.path.as_slice()
                        != [ContractProjection::Place(PlaceProjection::Field(index))]
                {
                    return Err(invalid());
                }
                if !output && Some(row.binder) == array_output_upper {
                    array_result_input = true;
                }
                borrow += 1;
                visit(row.binder, false)?;
            }
            continue;
        }
        let task = match actual {
            Type::Task { .. } => Some((actual, None, false)),
            Type::Managed(inner) | Type::Desc(inner)
                if matches!(inner.as_ref(), Type::Task { .. }) =>
            {
                Some((inner.as_ref(), None, true))
            }
            Type::ErrorUnion {
                result: success, ..
            } => match success.as_ref() {
                Type::Managed(inner) | Type::Desc(inner)
                    if matches!(inner.as_ref(), Type::Task { .. }) =>
                {
                    Some((inner.as_ref(), Some(PlaceProjection::ErrorArm(false)), true))
                }
                _ => None,
            },
            _ => None,
        };
        if let Some((task, arm, wrapped)) = task {
            if capture {
                return Err(invalid());
            }
            super::tasks::require_closed_context(task, span)?;
            let Type::Task { retained_life, .. } = task else {
                unreachable!()
            };
            if retained_life.ports.is_empty() {
                return Err(invalid());
            }
            let mut class_path = Vec::new();
            let mut port_path = Vec::new();
            if let Some(arm) = arm {
                class_path.push(LifeTypeProjection::Place(arm));
                port_path.push(ContractProjection::Place(arm));
            }
            if pointee {
                class_path.push(LifeTypeProjection::Pointee);
            }
            if wrapped {
                class_path.push(LifeTypeProjection::Pointee);
            }
            for (port, shape) in retained_life.ports.iter().enumerate() {
                let row = c.bindings.get(borrow).ok_or_else(invalid)?;
                let mut path = port_path.clone();
                path.push(ContractProjection::RetainedPort(port as u32));
                if row.position.base != base
                    || row.position.path != path
                    || c.classes
                        .get(class + shape.binder as usize)
                        .is_none_or(|class_row| class_row.binder != row.binder)
                {
                    return Err(invalid());
                }
                borrow += 1;
                visit(row.binder, !output)?;
            }
            for local in 0..retained_life.binder_count {
                let row = c.classes.get(class).ok_or_else(invalid)?;
                if row.position.base != base
                    || row.position.path != class_path
                    || row.position.class != local
                {
                    return Err(invalid());
                }
                class += 1;
                visit(row.binder, !output)?;
            }
            continue;
        }
        // The allocation outcome retains the success owner's complete class
        // identity even on an error return. Its port source proof remains in
        // the independent SSA/store kernel; this is metadata equality only.
        if let Type::ErrorUnion {
            error,
            result: success,
        } = actual
            && let Type::OwnedDyn { interface } = success.as_ref()
            && let Some(owner) =
                module.and_then(|m| m.dyn_interfaces.iter().find(|d| d.id == *interface))
            && owner.context.is_some()
        {
            if error.as_ref() != &crate::dynamic_dispatch::create_error() {
                return Err(invalid());
            }
            for local in 0..owner.retained_life.binder_count {
                let row = c.classes.get(class).ok_or_else(invalid)?;
                let mut path = if pointee {
                    vec![LifeTypeProjection::Pointee]
                } else {
                    vec![]
                };
                path.push(LifeTypeProjection::Place(PlaceProjection::ErrorArm(false)));
                if row.position.base != base
                    || row.position.path != path
                    || row.position.class != local
                {
                    return Err(invalid());
                }
                class += 1;
                visit(
                    row.binder,
                    if captures.is_some() { capture } else { !output },
                )?;
            }
            continue;
        }
        match actual {
            Type::Dyn { interface } | Type::OwnedDyn { interface } => {
                let owner = module
                    .and_then(|m| m.dyn_interfaces.iter().find(|d| d.id == *interface))
                    .ok_or_else(invalid)?;
                for local in 0..owner.retained_life.binder_count {
                    let row = c.classes.get(class).ok_or_else(invalid)?;
                    if row.position.base != base
                        || row.position.class != local
                        || if pointee {
                            row.position.path.as_slice() != [LifeTypeProjection::Pointee]
                        } else {
                            !row.position.path.is_empty()
                        }
                    {
                        return Err(invalid());
                    }
                    if capture && !matches!(row.binder, StorageBinder::Fixed(_)) {
                        return Err(invalid());
                    }
                    class += 1;
                    visit(
                        row.binder,
                        if captures.is_some() { capture } else { !output },
                    )?;
                }
            }
            Type::BorrowedStorage { storage, life } => {
                if (!matches!(
                    storage.as_ref(),
                    Type::Tuple(_) | Type::Record { .. } | Type::Package { .. }
                ) || (!life.retained.is_empty()
                    && !matches!(storage.as_ref(), Type::Package { .. } | Type::Tuple(_))))
                    && !pure_nominal_value(actual)
                    && !super::stored_flow::enum_address::signature(actual)
                    && !super::stored_flow::option_address::signature(actual)
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
                    visit(
                        row.binder,
                        if captures.is_some() { capture } else { !output },
                    )?;
                }
            }
            Type::Array { .. } | Type::ErrorUnion { .. } | Type::Tuple(_)
                if pure_array_value(actual)
                    || super::stored_flow::enum_address::signature(actual)
                    || matches!(actual, Type::Tuple(_)) && scoped_compound(actual) =>
            {
                let mut path = if pointee {
                    vec![LifeTypeProjection::Pointee]
                } else {
                    Vec::new()
                };
                let mut classes = Vec::new();
                array_classes(actual, &mut path, &mut classes);
                for (path, local) in classes {
                    let row = c.classes.get(class).ok_or_else(invalid)?;
                    if row.position.base != base
                        || row.position.path != path
                        || row.position.class != local
                    {
                        return Err(invalid());
                    }
                    class += 1;
                    visit(
                        row.binder,
                        if captures.is_some() { capture } else { !output },
                    )?;
                }
            }
            Type::Closure(id) if allow_callback => {
                let count = module
                    .and_then(|m| m.closures.get(id.0 as usize))
                    .and_then(|d| d.storage_life.as_ref())
                    .map_or(0, |c| c.fixed_binder_count);
                for local in 0..count {
                    let row = c.classes.get(class).ok_or_else(invalid)?;
                    if row.position.base != base
                        || row.position.class != local
                        || if pointee {
                            row.position.path.as_slice() != [LifeTypeProjection::Pointee]
                        } else {
                            !row.position.path.is_empty()
                        }
                    {
                        return Err(invalid());
                    }
                    class += 1;
                    // Returned Fixed classes must already be grounded by an
                    // input. A result row does not introduce retained storage.
                    visit(row.binder, !output)?;
                }
            }
            t if pointee => {
                if !(t.is_address_pointee()
                    // Input Package shape permits either receiver mode. The
                    // actual Store source, parent loan and access authority
                    // still require the independent body checks.
                    || !output
                        && matches!(base, StorageBase::Parameter(_))
                        && matches!(ty, Type::Address { .. })
                        && matches!(t, Type::Package { .. })
                    || allow_callback && matches!(t, Type::Closure(_)))
                {
                    return Err(invalid());
                }
            }
            t if (capture || allow_callback && !output) && super::stored_owned::owner(t) => {}
            t => {
                if !(t.is_copy_value()
                    || pure_array_value(t)
                    // This exact Direct-query input consumes no relation row.
                    || allow_byte_input && !output && !capture && !pointee
                        && matches!(ty,Type::Desc(payload) if payload.as_ref()==&Type::ByteBuffer)
                    || allow_callback && !output && matches!(t, Type::Closure(_)))
                {
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
        possible |= c.classes.iter().any(|r| {
            let StorageBase::Parameter(i) = r.position.base else {
                return false;
            };
            Some(r.binder) == upper
                && (!mutable
                    || matches!(
                        parameters.get(i as usize),
                        Some(Type::Address { mutable: true, .. })
                    ))
                && (!mutable || r.position.path.contains(&LifeTypeProjection::Pointee))
        });
        if !possible && !array_result_input {
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
