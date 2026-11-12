//! Independent ordinary Sum admission, tag facts and deep source transport.
//! Nothing in this module reads producer responsibility/tag annotations.
use super::*;
use std::collections::{BTreeMap, VecDeque};
mod initialization;
#[cfg(test)]
mod outer_cow_tests;
mod partial_places;
pub(super) mod replacements;
mod tag_cache;
#[cfg(test)]
mod tag_fixed_point_tests;
mod values;
#[cfg(test)]
mod values_tests;

pub(super) fn owned_enum(ty: &Type) -> bool {
    matches!(ty, Type::Enum { .. }) && owned_leaf(ty)
}

pub(super) fn owned(ty: &Type) -> bool {
    (matches!(
        ty,
        Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. }
    ) || matches!(ty, Type::BorrowedStorage {storage,..} if matches!(storage.as_ref(),Type::Tuple(_) | Type::Package{..})))
        && owned_leaf(ty)
}

fn owned_leaf(ty: &Type) -> bool {
    match ty {
        Type::BorrowedStorage { storage, .. } => owned_leaf(storage),
        Type::Managed(_) => true,
        Type::Tuple(fields) => fields.iter().any(owned_leaf),
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            fields.iter().any(|(_, field)| owned_leaf(field))
        }
        Type::Enum { variants, .. } => variants
            .iter()
            .any(|(_, fields)| fields.iter().any(owned_leaf)),
        Type::ErrorUnion { error, result } => owned_leaf(error) || owned_leaf(result),
        Type::Array { element, .. } => owned_leaf(element),
        _ => false,
    }
}

// Both domains use the same complete Content constructor. Only immutable
// type/slot classification is indexed; every invalidation still updates the
// actual Content, missing fields, routes and current tag alternatives.
fn indexed_invalidation(budget: &Budget) -> bool {
    budget.pod_stores
        || budget.store_option_results
            && (budget.task_borrowed_results || budget.ordinary_error_places)
}

fn bad(span: Option<Span>, message: impl Into<String>) -> Diagnostic {
    reject("E-VERIFY-SUM", span, message)
}

/// Used only after the paid raw-tree pass has detected a new Sum profile.
pub(super) fn needs_shape(ty: &Type, budget: &mut Budget) -> Check<bool> {
    if !budget.sums {
        return Ok(false);
    }
    resources::charge_payload(ty, budget)?;
    let mut pending = vec![(ty, false, false)];
    while let Some((ty, in_error, in_product)) = pending.pop() {
        match ty {
            Type::BorrowedStorage { .. } if owned(ty) => return Ok(true),
            Type::Tuple(_) if owned(ty) => return Ok(true),
            Type::Enum { .. } if owned_enum(ty) => return Ok(true),
            Type::ErrorUnion { error, result } => {
                if (in_product || in_error) && owned(ty) {
                    return Ok(true);
                }
                if **error != Type::Unit
                    && **result != Type::Unit
                    && (packages::has_children(error) || packages::has_children(result))
                {
                    return Ok(true);
                }
                pending.push((error, true, in_product));
                pending.push((result, true, in_product));
            }
            Type::Package { fields, .. } => {
                pending.extend(fields.iter().map(|(_, ty)| (ty, in_error, true)));
            }
            Type::Managed(payload)
                if in_error && matches!(payload.as_ref(), Type::OwnedStore { .. }) =>
            {
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub(super) fn check_type_for(
    module: &TypedModule,
    ty: &Type,
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<()> {
    resources::charge_payload(ty, budget)?;
    let owning_container = matches!(
        ty,
        Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. } | Type::Package { .. }
    );
    let mut pending = vec![ty];
    let mut qualified_addresses = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        resources::charge_payload(ty, budget)?;
        if matches!(ty, Type::Address { .. })
            && qualified_addresses.contains(&(ty as *const Type as usize))
        {
            continue;
        }
        if owning_container {
            dynamic_store::check_owner_field(ty, span)?;
        }
        if ty.is_copy_value() {
            check_type(ty, span)?;
            continue;
        }
        if stored_flow::pure_inline(module, ty, budget)?
            || stored_flow::pure_read(module, ty, budget)?
        {
            continue;
        }
        if stored_flow::failure_shape(ty, budget)? {
            let Type::BorrowedStorage { storage, .. } = ty else {
                unreachable!()
            };
            if owned_leaf(storage) {
                let input_fields = match storage.as_ref() {
                    Type::Tuple(fields) => fields.as_slice(),
                    Type::Package { fields, .. } => match &fields[1].1 {
                        Type::Tuple(fields) => fields.as_slice(),
                        _ => unreachable!(),
                    },
                    _ => {
                        return Err(bad(
                            span,
                            "owning qualified failure has no physical product",
                        ));
                    }
                };
                budget.spend(input_fields.len())?;
                for input in input_fields {
                    if matches!(input, Type::Address { .. }) {
                        qualified_addresses.insert(input as *const Type as usize);
                    }
                }
                pending.push(storage);
            }
            continue;
        }
        match ty {
            Type::Managed(_) => resources::check_type_for(module, ty, span, budget)?,
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                for field in fields {
                    pending.push(field);
                }
            }
            Type::Package { name, fields } => {
                budget.spend(name.len().saturating_add(fields.len().saturating_mul(2)))?;
                let mut names = BTreeSet::new();
                if name.is_empty() || fields.is_empty() {
                    return Err(bad(
                        span,
                        "ordinary Product requires a nonempty nominal identity and owning fields",
                    ));
                }
                let mut owns = false;
                for (name, field) in fields {
                    budget.spend(name.len().saturating_mul(fields.len().saturating_add(1)))?;
                    if name.is_empty() || !names.insert(name) {
                        return Err(bad(span, "Product fields must have unique nonempty names"));
                    }
                    owns |=
                        matches!(field, Type::Managed(_) | Type::Package { .. }) || owned(field);
                    pending.push(field);
                }
                if !owns {
                    return Err(bad(span, "ordinary Product requires a real owning child"));
                }
            }
            Type::Enum { name, variants } => {
                budget.spend(name.len().saturating_add(variants.len().saturating_mul(2)))?;
                let mut names = BTreeSet::new();
                if name.is_empty() || variants.is_empty() {
                    return Err(bad(
                        span,
                        "owned Enum requires a nonempty nominal identity and variants",
                    ));
                }
                for (name, fields) in variants {
                    budget.spend(
                        name.len()
                            .saturating_mul(variants.len().saturating_add(1))
                            .saturating_add(fields.len()),
                    )?;
                    if name.is_empty() || !names.insert(name) {
                        return Err(bad(span, "Enum variants must have unique nonempty names"));
                    }
                    pending.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                if matches!(result.as_ref(),Type::Managed(t) if matches!(t.as_ref(),Type::Task{..}))
                    && stored_flow::failure_shape(error, budget)?
                {
                    pending.extend([error.as_ref(), result.as_ref()]);
                    continue;
                }
                if matches!(result.as_ref(),Type::Managed(t) if matches!(t.as_ref(),Type::Task{..}))
                    && let Type::Package { name, fields } = error.as_ref()
                    && name.starts_with("@std.task:v1:TaskCreateFailure:")
                    && matches!(fields.as_slice(),[(reason,_),(inputs,Type::Tuple(_))] if reason=="reason" && inputs=="inputs")
                {
                    // Finite owning error and success arms retain distinct tags.
                    pending.extend([error.as_ref(), result.as_ref()]);
                    continue;
                }
                // Keep the existing resource-success profile. Ordinary Sum
                // composition does not turn an owning error into a Copy error.
                if dynamic_store::inline_owner(error) || dynamic_store::inline_owner(result) {
                    for payload in [error.as_ref(), result.as_ref()] {
                        if !payload.is_copy_value()
                            && !matches!(payload, Type::Package { .. } | Type::Enum { .. })
                            && !dynamic_store::inline_owner(payload)
                        {
                            return Err(unsupported(
                                "ordinary inline Store error-union arms require finite Copy, Product, Enum or sealed inline Store owners",
                                span,
                            ));
                        }
                    }
                } else if matches!(result.as_ref(), Type::Managed(_)) {
                    resources::charge_payload(error, budget)?;
                    if !error.is_copy_value() {
                        return Err(unsupported(
                            "direct Managed error-union success type requires a finite Copy error",
                            span,
                        ));
                    }
                } else {
                    for payload in [error.as_ref(), result.as_ref()] {
                        if !matches!(payload, Type::Package { .. } | Type::Enum { .. }) {
                            resources::charge_payload(payload, budget)?;
                            if !payload.is_copy_value() {
                                return Err(unsupported(
                                    "ordinary error-union payload slot types require finite Copy, Product or Enum values",
                                    span,
                                ));
                            }
                        }
                    }
                }
                pending.push(error);
                pending.push(result);
            }
            _ => {
                return Err(bad(
                    span,
                    "ordinary Sum payload must be Copy, canonical Managed, Product or finite Sum",
                ));
            }
        }
    }
    Ok(())
}

// This only exposes the tag/content skeleton to the resource ledger. Shared
// address leaves carry no resource debt; actual source/loan permission stays
// in the independently authenticated stored proof.
fn shared_enum_physical<'a>(ty: &'a Type, budget: &mut Budget) -> Check<&'a Type> {
    if let Type::BorrowedStorage { storage, .. } = ty {
        let limits = budget.limits.clone();
        if crate::stored_flow::failure_shape(ty, budget)? {
            return Ok(storage);
        }
        if budget.store_option_results
            && crate::stored_flow::option_address::shape(ty, &limits, budget)?
            || budget.stored_enum_addresses
                && crate::stored_flow::enum_address::shape(ty, &limits, budget)?
        {
            return Ok(storage);
        }
    }
    Ok(ty)
}

fn shared_enum_content(ty: &Type, budget: &mut Budget) -> Check<bool> {
    if !budget.store_option_results && !budget.stored_enum_addresses {
        return Ok(false);
    }
    budget.spend(1)?;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        budget.spend(1)?;
        if !std::ptr::eq(shared_enum_physical(ty, budget)?, ty) {
            return Ok(true);
        }
        match ty {
            Type::ErrorUnion { error, result } => {
                budget.spend(2)?;
                pending.extend([error.as_ref(), result.as_ref()]);
            }
            Type::Tuple(fields) => {
                budget.spend(fields.len())?;
                pending.extend(fields);
            }
            Type::Package { fields, .. } => {
                budget.spend(fields.len())?;
                pending.extend(fields.iter().map(|(_, field)| field));
            }
            Type::Enum { variants, .. } => {
                budget.spend(variants.len())?;
                for (_, fields) in variants {
                    budget.spend(fields.len())?;
                    pending.extend(fields);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

fn projected_type<'a>(
    f: &'a TypedFunction,
    local: LocalId,
    path: &[VariantProjection],
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<&'a Type> {
    let mut ty = shared_enum_physical(local_type(f, local, span)?, budget)?;
    budget.spend(path.len().saturating_add(1))?;
    for projection in path {
        let Type::Enum { variants, .. } = ty else {
            return Err(bad(
                span,
                "VariantProjection may only traverse an actual Enum",
            ));
        };
        ty = variants
            .get(projection.variant as usize)
            .and_then(|(_, fields)| fields.get(projection.index as usize))
            .ok_or_else(|| bad(span, "VariantProjection index is outside its exact Enum"))?;
    }
    Ok(ty)
}

pub(super) fn place_type<'a>(
    f: &'a TypedFunction,
    local: LocalId,
    path: &[PlaceProjection],
    span: Option<Span>,
    budget: &mut Budget,
) -> Check<&'a Type> {
    let mut at = shared_enum_physical(local_type(f, local, span)?, budget)?;
    if path.is_empty() || path.len() > budget.limits.max_call_depth {
        return Err(bad(
            span,
            "selective transfer requires a nonempty bounded actual path",
        ));
    }
    budget.spend(path.len().saturating_add(1))?;
    for step in path {
        at = match (step, at) {
            (PlaceProjection::Field(index), Type::Tuple(fields)) => fields.get(*index as usize),
            (PlaceProjection::Field(index), Type::Package { fields, .. }) => {
                fields.get(*index as usize).map(|(_, t)| t)
            }
            (PlaceProjection::Variant(p), Type::Enum { variants, .. }) => variants
                .get(p.variant as usize)
                .and_then(|(_, fs)| fs.get(p.index as usize)),
            (PlaceProjection::ErrorArm(e), Type::ErrorUnion { error, result }) => {
                Some(if *e { error.as_ref() } else { result.as_ref() })
            }
            _ => None,
        }
        .ok_or_else(|| {
            bad(
                span,
                "selective path crosses absent, non-inline or differently typed storage",
            )
        })?;
    }
    Ok(at)
}
pub(super) fn place_instruction(
    module: &TypedModule,
    f: &TypedFunction,
    i: &Instruction,
    b: &mut Budget,
) -> Check<Option<Type>> {
    b.mark_sum(Some(f.id.0 as usize))?;
    let span = Some(i.span);
    let (local, path) = match &i.kind {
        InstructionKind::BorrowPlaceLocal { local, path, .. }
        | InstructionKind::PlaceTagLocal { local, path } => (*local, path),
        _ => unreachable!(),
    };
    let root = local_type(f, local, span)?;
    check_type_for(module, root, span, b)?;
    let endpoint = if path.is_empty() {
        root
    } else {
        place_type(f, local, path, span, b)?
    };
    resources::charge_payload(endpoint, b)?;
    match &i.kind {
        InstructionKind::BorrowPlaceLocal { mutable, .. } => {
            if path.is_empty() || (*mutable && !f.locals[local.0 as usize].mutable) {
                return Err(bad(
                    span,
                    "precise mixed borrow requires a nonempty path and actual mutable authority",
                ));
            }
            if *mutable && matches!(endpoint, Type::Managed(_)) {
                return Err(bad(span, "ordinary resource head address remains shared"));
            }
            check_type_for(module, endpoint, span, b)?;
            Ok(Some(Type::Address {
                mutable: *mutable,
                inner: Box::new(endpoint.clone()),
            }))
        }
        InstructionKind::PlaceTagLocal { .. } => {
            if !matches!(endpoint, Type::Enum { .. } | Type::ErrorUnion { .. }) {
                return Err(bad(
                    span,
                    "mixed tag read must end at an actual Enum or ErrorUnion",
                ));
            }
            Ok(Some(Type::I32))
        }
        _ => unreachable!(),
    }
}
pub(super) fn check_instruction(
    f: &TypedFunction,
    i: &Instruction,
    limits: &Limits,
    budget: &mut Budget,
) -> Check<Option<Type>> {
    let span = Some(i.span);
    if crate::stored_flow::failure_borrow_only_unpack(f, i, budget)? {
        return Ok(None);
    }
    budget.mark_sum(Some(f.id.0 as usize))?;
    match &i.kind {
        InstructionKind::TakeTupleField { local, path }
        | InstructionKind::TakeVariantField { local, path } => {
            let root = local_type(f, *local, span)?;
            resources::charge_payload(root, budget)?;
            if root.is_copy_value()
                || !match i.kind {
                    InstructionKind::TakeTupleField { .. } => matches!(root, Type::Tuple(_)),
                    _ => matches!(root, Type::Enum { .. } | Type::ErrorUnion { .. }),
                }
            {
                return Err(bad(
                    span,
                    "selective transfer requires the exact owning Tuple or Enum root",
                ));
            }
            let endpoint = place_type(f, *local, path, span, budget)?;
            resources::charge_payload(endpoint, budget)?;
            Ok(Some(endpoint.clone()))
        }
        InstructionKind::MakeOwnedTuple { fields } => {
            let ty = i
                .result
                .and_then(|v| f.values.get(v.0 as usize))
                .ok_or_else(|| bad(span, "owned Tuple result absent"))?;
            let Type::Tuple(types) = ty else {
                return Err(bad(span, "owned Tuple result requires actual Tuple"));
            };
            resources::charge_payload(ty, budget)?;
            if ty.is_copy_value() || fields.len() != types.len() {
                return Err(bad(span, "owned Tuple fields or ownership mismatch"));
            }
            for (v, t) in fields.iter().zip(types) {
                if value_type(f, *v, span)? != t {
                    return Err(bad(span, "owned Tuple exact field Type mismatch"));
                }
            }
            Ok(Some(ty.clone()))
        }
        InstructionKind::UnpackTuple { local, fields } => {
            let ty = local_type(f, *local, span)?;
            let Type::Tuple(types) = shared_enum_physical(ty, budget)? else {
                return Err(bad(span, "UnpackTuple requires actual Tuple"));
            };
            resources::charge_payload(ty, budget)?;
            budget.spend(fields.len().saturating_mul(fields.len() + 2))?;
            if ty.is_copy_value() || fields.len() != types.len() {
                return Err(bad(span, "UnpackTuple requires all owning Tuple fields"));
            }
            let mut seen = BTreeSet::new();
            for (l, t) in fields.iter().zip(types) {
                if *l == *local || !seen.insert(*l) || local_type(f, *l, span)? != t {
                    return Err(bad(span, "UnpackTuple exact distinct targets mismatch"));
                }
            }
            Ok(None)
        }
        InstructionKind::DropTuple { local } => {
            let ty = local_type(f, *local, span)?;
            if !matches!(shared_enum_physical(ty, budget)?, Type::Tuple(_)) || ty.is_copy_value() {
                return Err(bad(span, "DropTuple requires owning Tuple"));
            }
            Ok(None)
        }
        InstructionKind::VariantTagLocal { local, path }
        | InstructionKind::ReadVariantLocal { local, path } => {
            if path.len() > limits.max_call_depth {
                return Err(limit("Sum projection depth exceeds the selected limit"));
            }
            let ty = projected_type(f, *local, path, span, budget)?;
            if matches!(i.kind, InstructionKind::VariantTagLocal { .. }) {
                if !matches!(ty, Type::Enum { .. }) {
                    return Err(bad(span, "TagLocal projection must end at an Enum"));
                }
                Ok(Some(Type::I32))
            } else {
                if path.is_empty() || !ty.is_copy_value() {
                    return Err(bad(
                        span,
                        "ReadVariantLocal requires a nonempty path ending at finite Copy",
                    ));
                }
                resources::charge_payload(ty, budget)?;
                check_type(ty, span)?;
                Ok(Some(ty.clone()))
            }
        }
        InstructionKind::BorrowVariantLocal {
            local,
            path,
            mutable,
        } => {
            if path.is_empty() || path.len() > limits.max_call_depth {
                return Err(bad(
                    span,
                    "payload borrow requires a nonempty bounded Enum path",
                ));
            }
            let ty = projected_type(f, *local, path, span, budget)?;
            resources::charge_payload(ty, budget)?;
            if (!ty.is_copy_value()
                && !matches!(
                    ty,
                    Type::Tuple(_)
                        | Type::Package { .. }
                        | Type::Enum { .. }
                        | Type::ErrorUnion { .. }
                ))
                || (*mutable && !f.locals[local.0 as usize].mutable)
            {
                return Err(bad(
                    span,
                    "payload borrow requires finite Copy and actual mutable authority",
                ));
            }
            Ok(Some(Type::Address {
                mutable: *mutable,
                inner: Box::new(ty.clone()),
            }))
        }
        InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } => {
            let ty = local_type(f, *local, span)?;
            let Type::Enum { variants, .. } = ty else {
                return Err(bad(span, "UnpackVariant requires an owned Enum local"));
            };
            if !owned_enum(ty) {
                return Err(bad(span, "UnpackVariant requires a non-Copy Enum"));
            }
            let payload = variants
                .get(*variant as usize)
                .ok_or_else(|| bad(span, "Unpack variant is out of bounds"))?;
            budget.spend(fields.len().saturating_mul(fields.len().saturating_add(2)))?;
            if fields.len() != payload.1.len() {
                return Err(bad(
                    span,
                    "Unpack must initialize every selected payload field",
                ));
            }
            let mut destinations = BTreeSet::new();
            for (target, expected) in fields.iter().zip(&payload.1) {
                resources::charge_payload(expected, budget)?;
                if *target == *local
                    || !destinations.insert(*target)
                    || local_type(f, *target, span)? != expected
                {
                    return Err(bad(
                        span,
                        "Unpack targets must be distinct from the source and each other, with exact field Types",
                    ));
                }
            }
            Ok(None)
        }
        InstructionKind::DropEnum { local } => {
            if !owned_enum(local_type(f, *local, span)?) {
                return Err(bad(span, "DropEnum requires an owned Enum local"));
            }
            Ok(None)
        }
        _ => unreachable!("Sum opcode dispatch"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Fresh,
    Input(u32, Vec<u32>),
    Captured(u32, Vec<u32>),
    ReturnedFresh,
    ReturnedInput(u32, Vec<u32>),
    Unknown,
}

/// Tags are per carrier; alternatives are not cross-multiplied with other Sums.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Content {
    Missing(Box<Content>),
    Copy,
    Leaf(BTreeSet<Source>),
    Product(Vec<Content>),
    Sum(BTreeMap<u32, Vec<Content>>, Option<(u32, Vec<u32>)>),
}

impl Content {
    fn no_cleanup_debt(&self, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        match self {
            Content::Copy | Content::Missing(_) => Ok(true),
            Content::Leaf(_) => Ok(false),
            Content::Product(children) => {
                b.spend(children.len())?;
                for child in children {
                    if !child.no_cleanup_debt(b)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Content::Sum(alternatives, _) => {
                if alternatives.is_empty() {
                    return Ok(false);
                }
                b.spend(alternatives.len())?;
                for children in alternatives.values() {
                    b.spend(children.len())?;
                    for child in children {
                        if !child.no_cleanup_debt(b)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
        }
    }
    fn live_async(&self, ty: &Type, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        let ty = shared_enum_physical(ty, b)?;
        match (self, ty) {
            (Content::Missing(_), _) => Ok(false),
            (Content::Leaf(_), Type::Managed(t)) => {
                Ok(matches!(t.as_ref(), Type::AsyncRegistration { .. }))
            }
            (Content::Product(children), Type::Tuple(fields)) => {
                for (index, child) in children.iter().enumerate() {
                    if child.live_async(&fields[index], b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (Content::Product(children), Type::Package { fields, .. }) => {
                for (index, child) in children.iter().enumerate() {
                    if child.live_async(&fields[index].1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (
                Content::Sum(variants, _),
                Type::Enum {
                    variants: types, ..
                },
            ) => {
                for (index, children) in variants {
                    for (field, child) in children.iter().enumerate() {
                        if child.live_async(&types[*index as usize].1[field], b)? {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            (Content::Sum(variants, _), Type::ErrorUnion { error, result }) => {
                for (index, children) in variants {
                    if let Some(child) = children.first()
                        && child.live_async(if *index == 0 { error } else { result }, b)?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    fn live_task(&self, ty: &Type, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        let ty = shared_enum_physical(ty, b)?;
        match (self, ty) {
            (Content::Missing(_), _) => Ok(false),
            (Content::Leaf(_), Type::Managed(_)) => Ok(true),
            (Content::Product(children), Type::Tuple(fields)) => {
                for (index, child) in children.iter().enumerate() {
                    if child.live_task(&fields[index], b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (Content::Product(children), Type::Package { fields, .. }) => {
                for (index, child) in children.iter().enumerate() {
                    if child.live_task(&fields[index].1, b)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            (
                Content::Sum(variants, _),
                Type::Enum {
                    variants: types, ..
                },
            ) => {
                for (index, children) in variants {
                    for (field, child) in children.iter().enumerate() {
                        if child.live_task(&types[*index as usize].1[field], b)? {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            (Content::Sum(variants, _), Type::ErrorUnion { error, result }) => {
                for (index, children) in variants {
                    if let Some(child) = children.first()
                        && child.live_task(if *index == 0 { error } else { result }, b)?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            // Opaque owner/closure storage is outside the selected Sum proof.
            // Preserve the existing full actual-origin cleanup obligation.
            _ => Ok(!ty.is_copy_value()),
        }
    }
    fn mark_captured(
        &mut self,
        capture: u32,
        path: &mut Vec<u32>,
        budget: &mut Budget,
    ) -> Check<()> {
        budget.spend(path.len().saturating_add(1))?;
        match self {
            Self::Leaf(sources) => {
                sources.clear();
                sources.insert(Source::Captured(capture, path.clone()));
            }
            Self::Product(fields) => {
                budget.spend(fields.len())?;
                for (index, field) in fields.iter_mut().enumerate() {
                    budget.spend(path.len().saturating_add(2))?;
                    path.extend([0, index as u32]);
                    field.mark_captured(capture, path, budget)?;
                    path.truncate(path.len() - 2);
                }
            }
            Self::Sum(variants, _) => {
                budget.spend(variants.len())?;
                for (tag, fields) in variants {
                    budget.spend(fields.len())?;
                    for (index, field) in fields.iter_mut().enumerate() {
                        budget.spend(path.len().saturating_add(3))?;
                        path.extend([1, *tag, index as u32]);
                        field.mark_captured(capture, path, budget)?;
                        path.truncate(path.len() - 3);
                    }
                }
            }
            Self::Missing(child) => child.mark_captured(capture, path, budget)?,
            Self::Copy => {}
        }
        Ok(())
    }
    fn cost(&self, budget: &mut Budget) -> Check<()> {
        budget.spend(1)?;
        match self {
            Self::Missing(child) => child.cost(budget)?,
            Self::Leaf(sources) => {
                budget.spend(sources.len())?;
                for source in sources {
                    if let Source::Input(_, path)
                    | Source::Captured(_, path)
                    | Source::ReturnedInput(_, path) = source
                    {
                        budget.spend(path.len())?;
                    }
                }
            }
            Self::Product(fields) => {
                budget.spend(fields.len())?;
                for field in fields {
                    field.cost(budget)?;
                }
            }
            Self::Sum(variants, route) => {
                if let Some((_, path)) = route {
                    budget.spend(path.len().saturating_add(1))?;
                }
                budget.spend(variants.len())?;
                for fields in variants.values() {
                    budget.spend(fields.len())?;
                    for field in fields {
                        field.cost(budget)?;
                    }
                }
            }
            Self::Copy => {}
        }
        Ok(())
    }
    fn duplicate(&self, budget: &mut Budget) -> Check<Self> {
        self.cost(budget)?;
        Ok(self.clone())
    }
    fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        other.cost(budget)?;
        let mut changed = false;
        if matches!(other, Self::Missing(_)) && !matches!(self, Self::Missing(_)) {
            self.cost(budget)?;
            let previous = std::mem::replace(self, Self::Copy);
            *self = Self::Missing(Box::new(previous));
            changed = true;
        }
        match (self, other) {
            (Self::Missing(a), Self::Missing(b)) => {
                changed |= a.merge(b, budget)?;
            }
            (Self::Missing(a), b) => {
                changed |= a.merge(b, budget)?;
            }
            (Self::Leaf(a), Self::Leaf(b)) => {
                let old = a.len();
                a.extend(b.iter().cloned());
                changed = old != a.len();
            }
            (Self::Product(a), Self::Product(b)) => {
                for (a, b) in a.iter_mut().zip(b) {
                    changed |= a.merge(b, budget)?;
                }
            }
            (Self::Sum(a, route), Self::Sum(b, other_route)) => {
                if a.is_empty() && !b.is_empty() {
                    *route = other_route.clone();
                    changed = true;
                } else if !b.is_empty() && route != other_route && route.take().is_some() {
                    changed = true;
                }
                for (&tag, fields) in b {
                    if let Some(target) = a.get_mut(&tag) {
                        for (a, b) in target.iter_mut().zip(fields) {
                            changed |= a.merge(b, budget)?;
                        }
                    } else {
                        for field in fields {
                            field.cost(budget)?;
                        }
                        budget.spend(fields.len())?;
                        a.insert(tag, fields.clone());
                        changed = true;
                    }
                }
            }
            (Self::Copy, Self::Copy) => {}
            _ => return Err(bad(None, "Sum source shapes differ at transfer")),
        }
        Ok(changed)
    }
    fn retain_missing(&mut self, old: &Content, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        if let Self::Missing(child) = old {
            if !matches!(self, Self::Missing(_)) {
                b.spend(1)?;
                let previous = std::mem::replace(self, Self::Copy);
                *self = Self::Missing(Box::new(previous));
            }
            let Self::Missing(at) = self else {
                unreachable!()
            };
            return at.retain_missing(child, b);
        }
        match (self, old) {
            (Self::Product(a), Self::Product(c)) => {
                for (a, c) in a.iter_mut().zip(c) {
                    a.retain_missing(c, b)?;
                }
            }
            (Self::Sum(a, _), Self::Sum(c, _)) => {
                for (tag, fs) in c {
                    b.spend(a.len().saturating_add(1))?;
                    if let Some(at) = a.get_mut(tag) {
                        for (a, c) in at.iter_mut().zip(fs) {
                            a.retain_missing(c, b)?;
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    // Exact fixed point of this kernel's own constructor and missing restore.
    // This proves no route, tag, source or missing wrapper would change; it
    // never establishes Copy eligibility or a resource permission.
    fn tags_unknown(&self, ty: &Type, b: &mut Budget) -> Check<bool> {
        b.spend(1)?;
        if let Self::Missing(child) = self {
            return child.tags_unknown(ty, b);
        }
        match (self, ty) {
            (Self::Copy, ty) => Ok(!matches!(
                ty,
                Type::Managed(_)
                    | Type::Tuple(_)
                    | Type::Package { .. }
                    | Type::Enum { .. }
                    | Type::ErrorUnion { .. }
                    | Type::BorrowedStorage { .. }
            )),
            (Self::Leaf(_), _) => Ok(false),
            (Self::Product(children), Type::Tuple(types)) => {
                if children.len() != types.len() {
                    return Ok(false);
                }
                b.spend(children.len())?;
                for (child, ty) in children.iter().zip(types) {
                    if !child.tags_unknown(ty, b)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Self::Product(children), Type::Package { fields, .. }) => {
                if children.len() != fields.len() {
                    return Ok(false);
                }
                b.spend(children.len())?;
                for (child, (_, ty)) in children.iter().zip(fields) {
                    if !child.tags_unknown(ty, b)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            (Self::Sum(arms, route), Type::Enum { variants, .. }) => {
                if route.is_some() || arms.len() != variants.len() {
                    return Ok(false);
                }
                b.spend(arms.len())?;
                for ((tag, children), (index, (_, types))) in
                    arms.iter().zip(variants.iter().enumerate())
                {
                    b.spend(1)?;
                    if *tag as usize != index || children.len() != types.len() {
                        return Ok(false);
                    }
                    b.spend(children.len())?;
                    for (child, ty) in children.iter().zip(types) {
                        if !child.tags_unknown(ty, b)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            (Self::Sum(arms, route), Type::ErrorUnion { error, result }) => {
                if route.is_some() || arms.len() != 2 {
                    return Ok(false);
                }
                b.spend(2)?;
                // Independent Verify representation: arm zero is error.
                for ((tag, children), (expected, ty)) in
                    arms.iter().zip([(0, error.as_ref()), (1, result.as_ref())])
                {
                    b.spend(1)?;
                    if *tag != expected || children.len() != 1 {
                        return Ok(false);
                    }
                    if !children[0].tags_unknown(ty, b)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    fn forget_copy_tags(&mut self, ty: &Type, b: &mut Budget) -> Check<()> {
        b.spend(1)?;
        let ty = shared_enum_physical(ty, b)?;
        if let Self::Missing(child) = self {
            return child.forget_copy_tags(ty, b);
        }
        let copy_sum = match self {
            Self::Sum(_, _)
                if indexed_invalidation(b)
                    && matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) =>
            {
                b.addressed_copy_sum(ty)?
            }
            Self::Product(_) if indexed_invalidation(b) => false,
            Self::Copy | Self::Leaf(_) if indexed_invalidation(b) => {
                return Ok(());
            }
            _ => {
                resources::charge_payload(ty, b)?;
                matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. }) && ty.is_copy_value()
            }
        };
        if copy_sum {
            if self.tags_unknown(ty, b)? {
                return Ok(());
            }
            let old = self.duplicate(b)?;
            let mut unknown = content(ty, None, &mut Vec::new(), false, b)?;
            unknown.retain_missing(&old, b)?;
            *self = unknown;
            return Ok(());
        }
        match (self, ty) {
            (Self::Product(fs), Type::Tuple(ts)) => {
                for (f, t) in fs.iter_mut().zip(ts) {
                    f.forget_copy_tags(t, b)?;
                }
            }
            (Self::Product(fs), Type::Package { fields, .. }) => {
                for (f, (_, t)) in fs.iter_mut().zip(fields) {
                    f.forget_copy_tags(t, b)?;
                }
            }
            (Self::Sum(vs, route), Type::Enum { variants, .. }) => {
                *route = None;
                for (v, fs) in vs {
                    for (f, t) in fs.iter_mut().zip(&variants[*v as usize].1) {
                        f.forget_copy_tags(t, b)?;
                    }
                }
            }
            (Self::Sum(vs, route), Type::ErrorUnion { error, result }) => {
                *route = None;
                for (v, fs) in vs {
                    if let Some(f) = fs.first_mut() {
                        f.forget_copy_tags(if *v == 0 { error } else { result }, b)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn forget_place_copy_tags(
        &mut self,
        path: &[PlaceProjection],
        ty: &Type,
        b: &mut Budget,
    ) -> Check<()> {
        b.spend(1)?;
        let ty = shared_enum_physical(ty, b)?;
        if let Self::Missing(child) = self {
            return child.forget_place_copy_tags(path, ty, b);
        }
        let Some((first, rest)) = path.split_first() else {
            return self.forget_copy_tags(ty, b);
        };
        match (first, self, ty) {
            (PlaceProjection::Field(i), Self::Product(fs), Type::Tuple(ts)) => {
                fs[*i as usize].forget_place_copy_tags(rest, &ts[*i as usize], b)
            }
            (PlaceProjection::Field(i), Self::Product(fs), Type::Package { fields, .. }) => {
                fs[*i as usize].forget_place_copy_tags(rest, &fields[*i as usize].1, b)
            }
            (PlaceProjection::Variant(p), Self::Sum(vs, route), Type::Enum { variants, .. }) => {
                *route = None;
                if let Some(fs) = vs.get_mut(&p.variant) {
                    fs[p.index as usize].forget_place_copy_tags(
                        rest,
                        &variants[p.variant as usize].1[p.index as usize],
                        b,
                    )?;
                }
                Ok(())
            }
            (
                PlaceProjection::ErrorArm(e),
                Self::Sum(vs, route),
                Type::ErrorUnion { error, result },
            ) => {
                *route = None;
                if let Some(fs) = vs.get_mut(&if *e { 0 } else { 1 }) {
                    fs[0].forget_place_copy_tags(rest, if *e { error } else { result }, b)?;
                }
                Ok(())
            }
            _ => Err(bad(
                None,
                "mutable mixed invalidation differs from actual storage",
            )),
        }
    }
    fn viewed<'a>(&'a self, path: &[PlaceProjection], b: &mut Budget) -> Check<&'a Content> {
        b.spend(1)?;
        if matches!(self, Self::Missing(_)) {
            return Err(bad(
                None,
                "place path crosses a subtree already moved on an incoming path",
            ));
        }
        let Some((first, rest)) = path.split_first() else {
            return Ok(self);
        };
        let child = match (first, self) {
            (PlaceProjection::Field(i), Self::Product(fs)) => fs.get(*i as usize),
            (PlaceProjection::Variant(p), Self::Sum(vs, _)) => {
                vs.get(&p.variant).and_then(|fs| fs.get(p.index as usize))
            }
            (PlaceProjection::ErrorArm(e), Self::Sum(vs, _)) => {
                vs.get(&if *e { 0 } else { 1 }).and_then(|fs| fs.first())
            }
            _ => None,
        }
        .ok_or_else(|| bad(None, "place path differs from actual initialized data"))?;
        child.viewed(rest, b)
    }
    fn take_place(
        &mut self,
        path: &[PlaceProjection],
        validate: bool,
        b: &mut Budget,
    ) -> Check<Content> {
        b.spend(1)?;
        if let Self::Missing(child) = self {
            if validate {
                return Err(bad(
                    None,
                    "selective transfer prefix or subtree is already moved on an incoming path",
                ));
            }
            return if path.is_empty() {
                child.duplicate(b)
            } else {
                child.take_place(path, false, b)
            };
        }
        let Some((first, rest)) = path.split_first() else {
            if validate && !self.complete(b)? {
                return Err(bad(
                    None,
                    "selective transfer requires the complete selected initialized subtree",
                ));
            }
            let output = self.duplicate(b)?;
            b.spend(1)?;
            let previous = std::mem::replace(self, Self::Copy);
            *self = Self::Missing(Box::new(previous));
            return Ok(output);
        };
        match (first, self) {
            (PlaceProjection::Field(index), Self::Product(fields)) => fields
                .get_mut(*index as usize)
                .ok_or_else(|| bad(None, "selective initialization field absent"))?
                .take_place(rest, validate, b),
            (PlaceProjection::Variant(p), Self::Sum(variants, _)) => variants
                .get_mut(&p.variant)
                .and_then(|fs| fs.get_mut(p.index as usize))
                .ok_or_else(|| bad(None, "selective payload has no current content"))?
                .take_place(rest, validate, b),
            (PlaceProjection::ErrorArm(e), Self::Sum(variants, _)) => variants
                .get_mut(&if *e { 0 } else { 1 })
                .and_then(|fs| fs.first_mut())
                .ok_or_else(|| bad(None, "selective error/success content absent"))?
                .take_place(rest, validate, b),
            _ => Err(bad(
                None,
                "selective initialization path differs from actual content",
            )),
        }
    }
    fn complete(&self, budget: &mut Budget) -> Check<bool> {
        budget.spend(1)?;
        match self {
            Self::Missing(_) => Ok(false),
            Self::Product(fs) => {
                for f in fs {
                    if !f.complete(budget)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Self::Sum(vs, _) => {
                if vs.is_empty() {
                    return Ok(false);
                }
                for fs in vs.values() {
                    for f in fs {
                        if !f.complete(budget)? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            _ => Ok(true),
        }
    }
    fn fresh(&self) -> u8 {
        match self {
            Self::Leaf(sources)
                if !sources.is_empty() && sources.iter().all(|s| *s == Source::Fresh) =>
            {
                1
            }
            _ => 2,
        }
    }
    /// Ordinary Value results have no per-leaf create contract (§8.1.1).
    /// Keep their actual roots/routes, but never retain fresh authority.
    fn returned(&mut self, budget: &mut Budget) -> Check<()> {
        self.cost(budget)?;
        match self {
            Self::Missing(child) => child.returned(budget)?,
            Self::Leaf(sources) => {
                *sources = sources
                    .iter()
                    .map(|source| match source {
                        Source::Fresh => Source::ReturnedFresh,
                        Source::Input(p, path) => Source::ReturnedInput(*p, path.clone()),
                        source => source.clone(),
                    })
                    .collect();
            }
            Self::Product(fields) => {
                for field in fields {
                    field.returned(budget)?;
                }
            }
            Self::Sum(variants, _) => {
                for fields in variants.values_mut() {
                    for field in fields {
                        field.returned(budget)?;
                    }
                }
            }
            Self::Copy => {}
        }
        Ok(())
    }
}

// Only an independently qualified, exact fixed point can preserve a shared
// root. All unknown cases execute the complete eager transfer below.
fn forget_shared_copy_tags(
    value: &mut Arc<Content>,
    ty: &Type,
    budget: &mut Budget,
    cached: Option<(&std::cell::RefCell<tag_cache::Cache<'_>>, usize)>,
) -> Check<()> {
    budget.spend(1)?;
    if Arc::strong_count(value) > 1
        && matches!(value.as_ref(), Content::Sum(..))
        && matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
    {
        // Keep the original entry and qualification order. In particular,
        // charge_payload rejects invalid Task contexts before any shape proof.
        budget.spend(1)?;
        let actual = shared_enum_physical(ty, budget)?;
        let unknown = if let Some((cache, local)) = cached
            && indexed_invalidation(budget)
        {
            budget.spend(1)?; // actual RefCell borrow before its slot work
            cache.borrow_mut().known(local, value, actual, budget)?
        } else {
            let copy_sum = if indexed_invalidation(budget) {
                budget.addressed_copy_sum(actual)?
            } else {
                resources::charge_payload(actual, budget)?;
                actual.is_copy_value()
            };
            copy_sum && value.tags_unknown(actual, budget)?
        };
        if unknown {
            return Ok(());
        }
    }
    value.cost(budget)?;
    Arc::make_mut(value).forget_copy_tags(ty, budget)
}

fn content(
    ty: &Type,
    parameter: Option<u32>,
    path: &mut Vec<u32>,
    bottom: bool,
    budget: &mut Budget,
) -> Check<Content> {
    budget.spend(path.len().saturating_add(2))?;
    if let Type::BorrowedStorage { .. } = ty {
        let physical = shared_enum_physical(ty, budget)?;
        if !std::ptr::eq(physical, ty) {
            return content(physical, parameter, path, bottom, budget);
        }
    }
    Ok(match ty {
        Type::Managed(_) => Content::Leaf(if bottom {
            BTreeSet::new()
        } else {
            BTreeSet::from([parameter.map_or(Source::Unknown, |p| Source::Input(p, path.clone()))])
        }),
        Type::Tuple(fields) => {
            budget.spend(fields.len())?;
            let mut children = Vec::with_capacity(fields.len());
            for (index, ty) in fields.iter().enumerate() {
                path.extend([0, index as u32]);
                children.push(content(ty, parameter, path, bottom, budget)?);
                path.truncate(path.len() - 2);
            }
            Content::Product(children)
        }
        Type::Package { fields, .. } => {
            budget.spend(fields.len())?;
            let mut children = Vec::with_capacity(fields.len());
            for (index, (_, ty)) in fields.iter().enumerate() {
                path.extend([0, index as u32]);
                children.push(content(ty, parameter, path, bottom, budget)?);
                path.truncate(path.len() - 2);
            }
            Content::Product(children)
        }
        Type::Enum { variants, .. } => {
            if bottom {
                return Ok(Content::Sum(BTreeMap::new(), None));
            }
            budget.spend(variants.len())?;
            let mut alternatives = BTreeMap::new();
            for (variant, (_, fields)) in variants.iter().enumerate() {
                budget.spend(fields.len())?;
                let mut children = Vec::with_capacity(fields.len());
                for (index, ty) in fields.iter().enumerate() {
                    path.extend([1, variant as u32, index as u32]);
                    children.push(content(ty, parameter, path, bottom, budget)?);
                    path.truncate(path.len() - 3);
                }
                if !bottom {
                    alternatives.insert(variant as u32, children);
                }
            }
            Content::Sum(alternatives, parameter.map(|p| (p, path.clone())))
        }
        Type::ErrorUnion { error, result } => {
            if bottom {
                return Ok(Content::Sum(BTreeMap::new(), None));
            }
            budget.spend(2)?;
            let mut alternatives = BTreeMap::new();
            for (variant, ty) in [(0, error.as_ref()), (1, result.as_ref())] {
                path.extend([1, variant, 0]);
                let child = content(ty, parameter, path, bottom, budget)?;
                path.truncate(path.len() - 3);
                if !bottom {
                    alternatives.insert(variant, vec![child]);
                }
            }
            Content::Sum(alternatives, parameter.map(|p| (p, path.clone())))
        }
        _ => Content::Copy,
    })
}

type Prefix = Vec<(u32, u32)>;
type Fact = (LocalId, Prefix, u32);
#[derive(Clone, PartialEq, Eq)]
struct State {
    initialized: initialization::Initialized, // exact 0..3; 1 vacant, 2 initialized; may-union.
    locals: Vec<Arc<Content>>,
    values: values::Values,
    captures: Vec<Option<Arc<Content>>>,
    facts: BTreeSet<Fact>,
    observations: BTreeSet<ValueId>,
    partial: Option<partial_places::State>,
}

impl State {
    fn cost(&self, budget: &mut Budget) -> Check<()> {
        budget.spend(
            self.initialized
                .copy_work()
                .saturating_add(self.locals.len())
                .saturating_add(self.values.page_count())
                .saturating_add(self.captures.len())
                .saturating_add(self.observations.len())
                .saturating_add(self.facts.len()),
        )?;
        if let Some(p) = &self.partial {
            p.cost(budget)?;
        }
        for (_, path, _) in &self.facts {
            budget.spend(path.len())?;
        }
        Ok(())
    }
    fn duplicate(&self, budget: &mut Budget) -> Check<Self> {
        self.cost(budget)?;
        Ok(self.clone())
    }
    fn merge(&mut self, other: &Self, budget: &mut Budget) -> Check<bool> {
        other.cost(budget)?;
        let mut changed = false;
        if let (Some(a), Some(b)) = (&mut self.partial, &other.partial) {
            changed |= a.join(b, budget)?;
        }
        changed |= self.initialized.merge(&other.initialized, budget)?;
        for (a, b) in self.locals.iter_mut().zip(&other.locals) {
            if !Arc::ptr_eq(a, b) {
                if Arc::strong_count(a) > 1 {
                    a.cost(budget)?;
                }
                changed |= Arc::make_mut(a).merge(b, budget)?;
            }
        }
        changed |= self.values.merge(&other.values, budget)?;
        for (a, b) in self.captures.iter_mut().zip(&other.captures) {
            if let (Some(a), Some(b)) = (a, b)
                && !Arc::ptr_eq(a, b)
            {
                if Arc::strong_count(a) > 1 {
                    a.cost(budget)?;
                }
                changed |= Arc::make_mut(a).merge(b, budget)?;
            }
        }
        let old = self.facts.len();
        self.facts.retain(|f| other.facts.contains(f));
        changed |= old != self.facts.len();
        let old = self.observations.len();
        self.observations.retain(|v| other.observations.contains(v));
        changed |= old != self.observations.len();
        Ok(changed)
    }
}

#[derive(Default)]
pub(super) struct Proofs {
    pub(super) rows: Vec<Vec<Vec<Bridge>>>,
    // Kernel-proved impossible edges, retained only for affected functions.
    pub(super) excluded_edges: BTreeMap<usize, ExcludedEdges>,
    pub(super) replacement_writes: Vec<BTreeSet<resources::Root>>,
    pub(super) replacement_targets: BTreeMap<usize, Vec<crate::place_replacements::actual::Target>>,
    // Actual may-Commit positions not already retained for Store transfer.
    // These are retirement effects only, never must-write/fresh authority.
    pub(super) retirement_targets: BTreeMap<usize, Vec<crate::place_replacements::actual::Target>>,
}
type ExcludedEdges = BTreeSet<(usize, usize)>;
type Solved = (Content, Vec<Vec<Bridge>>, ExcludedEdges);
type FieldFresh = Vec<(usize, u8)>;
type UnpackedFresh = (LocalId, u8, FieldFresh);
#[derive(Clone, Default)]
pub(super) struct Bridge {
    // Derived from this kernel's selected pre-drop Content, never producer metadata.
    pub(super) task_cleanup: Option<bool>,
    // Exact selected pre-drop contents; absent keeps ordinary order obligations.
    pub(super) empty_failure_cleanup: Option<bool>,
    pub(super) result: Option<u8>,
    pub(super) package: FieldFresh,
    pub(super) unpack: Vec<UnpackedFresh>,
}

fn direct_fields(value: &Content, budget: &mut Budget) -> Check<Vec<(usize, u8)>> {
    let mut result = Vec::new();
    if let Content::Product(fields) = value {
        budget.spend(fields.len())?;
        for (index, field) in fields.iter().enumerate() {
            if let Content::Leaf(sources) = field {
                budget.spend(sources.len())?;
                result.push((index, field.fresh()));
            }
        }
    }
    Ok(result)
}

struct Flow<'a> {
    f: &'a TypedFunction,
    module: &'a TypedModule,
    definitions: Vec<Option<(usize, usize)>>,
    observers: Vec<Option<(LocalId, Prefix)>>,
    summaries: &'a [Content],
    partial: bool,
    place_meta: Option<partial_places::Meta>,
    // This follows Content's own construction, not Copy permission. Records,
    // arrays and Managed endpoints carry Copy/Leaf content; Tuple/Package
    // children and every Enum/ErrorUnion can retain a Sum route or tag.
    tagged_locals: Option<Vec<usize>>,
    // Positive shape proofs only. The function borrow fixes each local Type;
    // strong references make cached Content identities immutable until eviction.
    tag_cache: Option<std::cell::RefCell<tag_cache::Cache<'a>>>,
    address_replacement_tags: bool,
    owned_creation: bool,
    replacements: Option<&'a replacements::Context<'a>>,
}

impl Flow<'_> {
    fn kill(&self, state: &mut State, local: LocalId, budget: &mut Budget) -> Check<()> {
        budget.spend(state.facts.len().saturating_add(state.observations.len()))?;
        state.facts.retain(|(root, _, _)| *root != local);
        state.observations.retain(|v| {
            self.observers[v.0 as usize]
                .as_ref()
                .is_none_or(|(root, _)| *root != local)
        });
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn selected<'a>(
        &self,
        state: &'a State,
        local: LocalId,
        path: &[VariantProjection],
        last: Option<u32>,
        validate: bool,
        span: Span,
        budget: &mut Budget,
    ) -> Check<&'a Content> {
        let mut current = state.locals[local.0 as usize].as_ref();
        let mut prefix = Vec::new();
        budget.spend(path.len().saturating_mul(path.len().saturating_add(2)))?;
        if validate && state.initialized.get(local.0 as usize, budget)? != 2 {
            return Err(bad(
                Some(span),
                "Sum observation/consume requires a complete live current local",
            ));
        }
        for step in path {
            self.require_tag(
                state,
                local,
                &prefix,
                current,
                step.variant,
                validate,
                span,
                budget,
            )?;
            let Content::Sum(variants, _) = current else {
                return Err(bad(Some(span), "Sum path is not an Enum"));
            };
            let Some(fields) = variants.get(&step.variant) else {
                return Ok(current);
            };
            current = &fields[step.index as usize];
            prefix.push((step.variant, step.index));
        }
        if validate && self.partial && matches!(current, Content::Missing(_)) {
            return Err(bad(Some(span), "Sum endpoint has already been moved"));
        }
        if let Some(tag) = last {
            self.require_tag(state, local, &prefix, current, tag, validate, span, budget)?;
        }
        Ok(current)
    }
    #[allow(clippy::too_many_arguments)]
    fn require_tag(
        &self,
        state: &State,
        local: LocalId,
        path: &Prefix,
        value: &Content,
        tag: u32,
        validate: bool,
        span: Span,
        budget: &mut Budget,
    ) -> Check<()> {
        budget.spend(
            path.len()
                .saturating_add(state.facts.len())
                .saturating_add(1),
        )?;
        let known = matches!(value,Content::Sum(variants, _) if variants.len()==1 && variants.contains_key(&tag));
        if validate && !known && !state.facts.contains(&(local, path.clone(), tag)) {
            return Err(bad(
                Some(span),
                "payload requires the exact selected tag of this current local and complete prefix on every incoming path",
            ));
        }
        Ok(())
    }
    fn edge(
        &self,
        state: &mut State,
        block: usize,
        next: usize,
        budget: &mut Budget,
    ) -> Check<bool> {
        if let (Some(meta), Some(p)) = (&self.place_meta, &mut state.partial)
            && let Some((local, path, tag, equal)) =
                meta.edge(p, &self.f.blocks[block].terminator, next, budget)?
            && path.is_empty()
            && matches!(self.f.locals[local.0 as usize].ty, Type::ErrorUnion { .. })
        {
            let actual = state.locals[local.0 as usize].as_ref();
            if state.initialized.get(local.0 as usize, budget)? == 2
                && let Content::Sum(arms, _) = actual
                && arms.len() == 1
                && actual.complete(budget)?
            {
                budget.spend(1)?;
                let (&arm, _) = arms.first_key_value().expect("one actual arm");
                // Runtime is_error is 0/1; Content keeps error=0/ok=1.
                // Only a live observation and complete actual singleton
                // justify excluding a contradictory CFG edge. Bottom or
                // moved content must still reach ordinary validation.
                if arm <= 1 && (u32::from(arm == 0) == tag) != equal {
                    return Ok(false);
                }
            }
            if self.address_replacement_tags || self.owned_creation {
                replacements::refine_actual_error_arm(state, local, tag, equal, budget)?;
            }
        }
        let Terminator::Branch {
            condition,
            then_block,
            else_block,
        } = self.f.blocks[block].terminator
        else {
            return Ok(true);
        };
        if then_block == else_block || then_block.0 as usize != next {
            return Ok(true);
        }
        let Some((b, i)) = self.definitions[condition.0 as usize] else {
            return Ok(true);
        };
        let InstructionKind::Binary {
            op: BinaryOp::Equal,
            left,
            right,
        } = self.f.blocks[b].instructions[i].kind
        else {
            return Ok(true);
        };
        for (tag, constant) in [(left, right), (right, left)] {
            if !state.observations.contains(&tag) {
                continue;
            }
            let Some((cb, ci)) = self.definitions[constant.0 as usize] else {
                continue;
            };
            let InstructionKind::Constant(Constant::I32(variant)) =
                self.f.blocks[cb].instructions[ci].kind
            else {
                continue;
            };
            if variant < 0 {
                continue;
            }
            if let Some((root, path)) = &self.observers[tag.0 as usize] {
                budget.spend(
                    path.len()
                        .saturating_add(state.facts.len())
                        .saturating_add(1),
                )?;
                state.facts.insert((*root, path.clone(), variant as u32));
            }
        }
        Ok(true)
    }
    fn instantiate(
        &self,
        value: &Content,
        args: &[ValueId],
        state: &State,
        budget: &mut Budget,
    ) -> Check<Content> {
        value.cost(budget)?;
        Ok(match value {
            Content::Leaf(sources) => {
                let mut result = BTreeSet::new();
                for source in sources {
                    match source {
                        Source::Input(parameter, path) | Source::ReturnedInput(parameter, path) => {
                            let returned = matches!(source, Source::ReturnedInput(..));
                            budget.spend(path.len().saturating_add(1))?;
                            let mut source = state
                                .values
                                .get(args[*parameter as usize].0 as usize, budget)?
                                .as_ref();
                            let mut cursor = 0;
                            while cursor < path.len() {
                                match (path[cursor], source) {
                                    (0, Content::Product(fields)) => {
                                        source = &fields[path[cursor + 1] as usize];
                                        cursor += 2;
                                    }
                                    (1, Content::Sum(variants, _)) => {
                                        let Some(fields) = variants.get(&path[cursor + 1]) else {
                                            break;
                                        };
                                        source = &fields[path[cursor + 2] as usize];
                                        cursor += 3;
                                    }
                                    _ => break,
                                }
                            }
                            if cursor == path.len() {
                                if let Content::Leaf(origins) = source {
                                    budget.spend(origins.len())?;
                                    result.extend(origins.iter().map(|origin| {
                                        if returned {
                                            match origin {
                                                Source::Fresh => Source::ReturnedFresh,
                                                Source::Input(p, path) => {
                                                    Source::ReturnedInput(*p, path.clone())
                                                }
                                                source => source.clone(),
                                            }
                                        } else {
                                            origin.clone()
                                        }
                                    }));
                                } else {
                                    result.insert(Source::Unknown);
                                }
                            } else {
                                result.insert(Source::Unknown);
                            }
                        }
                        source => {
                            result.insert(source.clone());
                        }
                    }
                }
                Content::Leaf(result)
            }
            Content::Product(fields) => Content::Product(
                fields
                    .iter()
                    .map(|c| self.instantiate(c, args, state, budget))
                    .collect::<Check<_>>()?,
            ),
            Content::Sum(_, Some((parameter, path))) => {
                budget.spend(path.len().saturating_add(1))?;
                let mut actual = state
                    .values
                    .get(args[*parameter as usize].0 as usize, budget)?
                    .as_ref();
                let mut cursor = 0;
                while cursor < path.len() {
                    match (path[cursor], actual) {
                        (0, Content::Product(fields)) => {
                            actual = &fields[path[cursor + 1] as usize];
                            cursor += 2;
                        }
                        (1, Content::Sum(variants, _)) => {
                            let Some(fields) = variants.get(&path[cursor + 1]) else {
                                break;
                            };
                            actual = &fields[path[cursor + 2] as usize];
                            cursor += 3;
                        }
                        _ => break,
                    }
                }
                if cursor == path.len() {
                    actual.duplicate(budget)?
                } else {
                    // The actual caller cannot take this inactive path. Keep
                    // the summary's conservative shape and leaf sources.
                    let Content::Sum(variants, _) = value else {
                        unreachable!()
                    };
                    let without_route = Content::Sum(variants.clone(), None);
                    self.instantiate(&without_route, args, state, budget)?
                }
            }
            Content::Sum(variants, None) => Content::Sum(
                variants
                    .iter()
                    .map(|(&v, fields)| {
                        Ok((
                            v,
                            fields
                                .iter()
                                .map(|c| self.instantiate(c, args, state, budget))
                                .collect::<Check<_>>()?,
                        ))
                    })
                    .collect::<Check<_>>()?,
                None,
            ),
            Content::Missing(child) => {
                Content::Missing(Box::new(self.instantiate(child, args, state, budget)?))
            }
            Content::Copy => Content::Copy,
        })
    }
    fn block(
        &self,
        block: usize,
        state: &mut State,
        validate: bool,
        record: bool,
        budget: &mut Budget,
    ) -> Check<Vec<Bridge>> {
        if record {
            budget.spend(self.f.blocks[block].instructions.len())?;
        }
        let mut bridges = Vec::new();
        for instruction in &self.f.blocks[block].instructions {
            budget.spend(1)?;
            let mut result = None;
            let mut bridge = Bridge::default();
            if validate
                && self.partial
                && let InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local } = instruction.kind
                && (state.initialized.get(local.0 as usize, budget)? != 2
                    || !state.locals[local.0 as usize].complete(budget)?)
            {
                return Err(bad(
                    Some(instruction.span),
                    "whole borrow requires complete initialized storage",
                ));
            }

            if let (Some(meta), Some(p)) = (&self.place_meta, &mut state.partial) {
                meta.step(p, instruction, budget)?;
                if partial_places::invalidates(&instruction.kind) {
                    if let Some(locals) = &self.tagged_locals {
                        budget.spend(locals.len())?;
                        for &local in locals {
                            let content = &mut state.locals[local];
                            forget_shared_copy_tags(
                                content,
                                &self.f.locals[local].ty,
                                budget,
                                self.tag_cache.as_ref().map(|cache| (cache, local)),
                            )?;
                        }
                    } else {
                        for (local, content) in state.locals.iter_mut().enumerate() {
                            forget_shared_copy_tags(
                                content,
                                &self.f.locals[local].ty,
                                budget,
                                self.tag_cache.as_ref().map(|cache| (cache, local)),
                            )?;
                        }
                    }
                }
            }
            if let Some(context) = self.replacements
                && let Some(value) = context.step(self, state, instruction, validate, budget)?
            {
                result = Some(value);
            }
            match &instruction.kind {
                InstructionKind::BorrowCapture { index, .. }
                    if state
                        .captures
                        .get(*index as usize)
                        .is_some_and(Option::is_some) =>
                {
                    let captured = state.captures[*index as usize]
                        .as_ref()
                        .expect("actual direct Sum capture");
                    if validate && !captured.complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "direct Sum borrow reads an unfilled captured owner",
                        ));
                    }
                }
                InstructionKind::TakeCapture { index }
                    if state
                        .captures
                        .get(*index as usize)
                        .is_some_and(Option::is_some) =>
                {
                    let current = state.captures[*index as usize]
                        .as_ref()
                        .expect("actual direct Sum capture");
                    if validate && !current.complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "direct Sum capture was already transferred",
                        ));
                    }
                    result = Some(current.duplicate(budget)?);
                    let ty = &self.module.closures
                        [self.f.environment.expect("capture entry").0 as usize]
                        .captures[*index as usize];
                    state.captures[*index as usize] =
                        Some(Arc::new(content(ty, None, &mut Vec::new(), true, budget)?));
                }
                InstructionKind::WriteCapture { index, value }
                    if state
                        .captures
                        .get(*index as usize)
                        .is_some_and(Option::is_some) =>
                {
                    let current = state.captures[*index as usize]
                        .as_ref()
                        .expect("actual direct Sum capture");
                    if validate && current.complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "direct Sum capture refill would overwrite an active owner",
                        ));
                    }
                    let incoming = state
                        .values
                        .get(value.0 as usize, budget)?
                        .duplicate(budget)?;
                    if validate && !incoming.complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "direct Sum capture refill requires a complete actual value",
                        ));
                    }
                    state.captures[*index as usize] = Some(Arc::new(incoming));
                }
                InstructionKind::BorrowLocalMut { local }
                    if matches!(
                        shared_enum_physical(&self.f.locals[local.0 as usize].ty, budget)?,
                        Type::Enum { .. }
                    ) =>
                {
                    fn forget(at: &mut Content, t: &Type, b: &mut Budget) -> Check<()> {
                        resources::charge_payload(t, b)?;
                        if matches!(t, Type::Enum { .. }) && t.is_copy_value() {
                            *at = content(t, None, &mut Vec::new(), false, b)?;
                            return Ok(());
                        }
                        match (at, t) {
                            (Content::Sum(vs, _), Type::Enum { variants, .. }) => {
                                for (tag, fields) in vs {
                                    for (child, t) in
                                        fields.iter_mut().zip(&variants[*tag as usize].1)
                                    {
                                        forget(child, t, b)?;
                                    }
                                }
                            }
                            (Content::Product(children), Type::Package { fields, .. }) => {
                                for (child, (_, t)) in children.iter_mut().zip(fields) {
                                    forget(child, t, b)?;
                                }
                            }
                            _ => {}
                        }
                        Ok(())
                    }
                    state.locals[local.0 as usize].cost(budget)?;
                    let physical =
                        shared_enum_physical(&self.f.locals[local.0 as usize].ty, budget)?;
                    forget(
                        Arc::make_mut(&mut state.locals[local.0 as usize]),
                        physical,
                        budget,
                    )?;
                    self.kill(state, *local, budget)?;
                }
                InstructionKind::MakeManaged { .. }
                | InstructionKind::ByteBufferNew
                | InstructionKind::OwnedStoreNew { .. } => {
                    result = Some(Content::Leaf(BTreeSet::from([Source::Fresh])))
                }
                InstructionKind::MakePackage { fields }
                | InstructionKind::MakeOwnedTuple { fields } => {
                    budget.spend(fields.len())?;
                    result = Some(Content::Product(
                        fields
                            .iter()
                            .map(|v| state.values.get(v.0 as usize, budget)?.duplicate(budget))
                            .collect::<Check<_>>()?,
                    ));
                }
                InstructionKind::TaskCreateOwned { arguments, .. } => {
                    budget.spend(arguments.len().saturating_add(12))?;
                    let inputs = Content::Product(
                        arguments
                            .iter()
                            .map(|v| state.values.get(v.0 as usize, budget)?.duplicate(budget))
                            .collect::<Check<_>>()?,
                    );
                    let failure = Content::Product(vec![Content::Copy, inputs]);
                    result = Some(Content::Sum(
                        BTreeMap::from([
                            (0, vec![failure]),
                            (1, vec![Content::Leaf(BTreeSet::from([Source::Fresh]))]),
                        ]),
                        None,
                    ));
                }
                InstructionKind::WithScopedThread { argument, .. }
                    if tasks::transfer::task_owner(&self.f.values[argument.0 as usize]) =>
                {
                    budget.spend(8)?;
                    let source = state
                        .values
                        .get(argument.0 as usize, budget)?
                        .duplicate(budget)?;
                    result = Some(Content::Sum(
                        BTreeMap::from([
                            (0, vec![source]),
                            (1, vec![Content::Copy, Content::Copy]),
                        ]),
                        None,
                    ));
                }
                InstructionKind::MakeVariant { variant, fields } => {
                    budget.spend(fields.len())?;
                    result = Some(Content::Sum(
                        BTreeMap::from([(
                            *variant,
                            fields
                                .iter()
                                .map(|v| state.values.get(v.0 as usize, budget)?.duplicate(budget))
                                .collect::<Check<_>>()?,
                        )]),
                        None,
                    ));
                }
                InstructionKind::MakeErrorUnion { is_error, value } => {
                    result = Some(Content::Sum(
                        BTreeMap::from([(
                            if *is_error { 0 } else { 1 },
                            vec![
                                state
                                    .values
                                    .get(value.0 as usize, budget)?
                                    .duplicate(budget)?,
                            ],
                        )]),
                        None,
                    ))
                }
                InstructionKind::TakeErrorUnion { value, is_error } => {
                    if let Content::Sum(variants, _) =
                        state.values.get(value.0 as usize, budget)?.as_ref()
                        && let Some(fields) = variants.get(&(if *is_error { 0 } else { 1 }))
                    {
                        result = Some(fields[0].duplicate(budget)?);
                    }
                }
                InstructionKind::Store { local, value } => {
                    self.kill(state, *local, budget)?;
                    budget.spend(1)?;
                    state.locals[local.0 as usize] =
                        state.values.get(value.0 as usize, budget)?.clone();
                    state.initialized.set(local.0 as usize, 2, budget)?;
                }
                InstructionKind::PlaceTagLocal { local, path }
                | InstructionKind::BorrowPlaceLocal { local, path, .. } => {
                    if validate {
                        if state.initialized.get(local.0 as usize, budget)? != 2 {
                            return Err(bad(
                                Some(instruction.span),
                                "mixed place root is not initialized",
                            ));
                        }
                        state
                            .partial
                            .as_ref()
                            .ok_or_else(|| bad(None, "mixed place tag state absent"))?
                            .require(*local, path, budget)?;
                        let endpoint = state.locals[local.0 as usize].viewed(path, budget)?;
                        if matches!(instruction.kind, InstructionKind::BorrowPlaceLocal { .. })
                            && !endpoint.complete(budget)?
                        {
                            return Err(bad(
                                Some(instruction.span),
                                "mixed borrow requires every selected descendant initialized",
                            ));
                        }
                    }
                    if matches!(
                        instruction.kind,
                        InstructionKind::BorrowPlaceLocal { mutable: true, .. }
                    ) {
                        state.locals[local.0 as usize].cost(budget)?;
                        Arc::make_mut(&mut state.locals[local.0 as usize]).forget_place_copy_tags(
                            path,
                            &self.f.locals[local.0 as usize].ty,
                            budget,
                        )?;
                    }
                }
                InstructionKind::BorrowLocal { local }
                | InstructionKind::BorrowLocalMut { local }
                    if self.partial =>
                {
                    if validate && !state.locals[local.0 as usize].complete(budget)? {
                        return Err(bad(
                            Some(instruction.span),
                            "whole borrow requires complete initialized storage",
                        ));
                    }
                }
                InstructionKind::TakeTupleField { local, path }
                | InstructionKind::TakeVariantField { local, path } => {
                    if validate {
                        if state.initialized.get(local.0 as usize, budget)? != 2 {
                            return Err(bad(
                                Some(instruction.span),
                                "selective source storage is not initialized",
                            ));
                        }
                        state
                            .partial
                            .as_ref()
                            .ok_or_else(|| bad(None, "selective tag state is absent"))?
                            .require(*local, path, budget)?;
                    }
                    state.locals[local.0 as usize].cost(budget)?;
                    let selected = Arc::make_mut(&mut state.locals[local.0 as usize])
                        .take_place(path, validate, budget);
                    result = Some(match selected {
                        Ok(value) => value,
                        Err(e)
                            if !validate && e.category == zil_base::ResultCategory::SafeReject =>
                        {
                            let ty =
                                place_type(self.f, *local, path, Some(instruction.span), budget)?;
                            content(ty, None, &mut Vec::new(), true, budget)?
                        }
                        Err(e) => return Err(e),
                    });
                    self.kill(state, *local, budget)?;
                }
                InstructionKind::Load(local) | InstructionKind::TakeLocal { local } => {
                    if validate
                        && self.partial
                        && !state.locals[local.0 as usize].complete(budget)?
                    {
                        return Err(bad(
                            Some(instruction.span),
                            "whole local transfer/read requires every subtree initialized",
                        ));
                    }
                    result = Some(state.locals[local.0 as usize].duplicate(budget)?);
                    if matches!(instruction.kind, InstructionKind::TakeLocal { .. }) {
                        self.kill(state, *local, budget)?;
                        state.initialized.set(local.0 as usize, 1, budget)?;
                    }
                }
                InstructionKind::TakePackageField { local, index }
                | InstructionKind::ReadPackageField { local, index } => {
                    if matches!(instruction.kind, InstructionKind::TakePackageField { .. }) {
                        state.locals[local.0 as usize].cost(budget)?;
                        result = Some(
                            Arc::make_mut(&mut state.locals[local.0 as usize]).take_place(
                                &[PlaceProjection::Field(*index)],
                                validate,
                                budget,
                            )?,
                        );
                    } else if let Content::Product(fields) = state.locals[local.0 as usize].as_ref()
                    {
                        result = Some(fields[*index as usize].duplicate(budget)?);
                    }
                    self.kill(state, *local, budget)?;
                }
                InstructionKind::StorePackageField {
                    local,
                    index,
                    value,
                } => {
                    self.kill(state, *local, budget)?;
                    if Arc::strong_count(&state.locals[local.0 as usize]) > 1 {
                        state.locals[local.0 as usize].cost(budget)?;
                    }
                    if let Content::Product(fields) =
                        Arc::make_mut(&mut state.locals[local.0 as usize])
                    {
                        fields[*index as usize] = state
                            .values
                            .get(value.0 as usize, budget)?
                            .duplicate(budget)?;
                    }
                }
                InstructionKind::VariantTagLocal { local, path }
                | InstructionKind::ReadVariantLocal { local, path }
                | InstructionKind::BorrowVariantLocal { local, path, .. } => {
                    self.selected(
                        state,
                        *local,
                        path,
                        None,
                        validate,
                        instruction.span,
                        budget,
                    )?;
                    if matches!(
                        instruction.kind,
                        InstructionKind::BorrowVariantLocal { mutable: true, .. }
                    ) {
                        let ty =
                            projected_type(self.f, *local, path, Some(instruction.span), budget)?;
                        if matches!(ty, Type::Enum { .. } | Type::ErrorUnion { .. })
                            && ty.is_copy_value()
                        {
                            fn forget(
                                at: &mut Content,
                                path: &[VariantProjection],
                                ty: &Type,
                                budget: &mut Budget,
                            ) -> Check<()> {
                                if path.is_empty() {
                                    *at = content(ty, None, &mut Vec::new(), false, budget)?;
                                    return Ok(());
                                }
                                let Content::Sum(vs, _) = at else {
                                    return Err(bad(None, "mutable payload path crosses non-Enum"));
                                };
                                if let Some(fs) = vs.get_mut(&path[0].variant) {
                                    let child = fs
                                        .get_mut(path[0].index as usize)
                                        .ok_or_else(|| bad(None, "mutable payload field absent"))?;
                                    forget(child, &path[1..], ty, budget)?;
                                }
                                Ok(())
                            }
                            state.locals[local.0 as usize].cost(budget)?;
                            forget(
                                Arc::make_mut(&mut state.locals[local.0 as usize]),
                                path,
                                ty,
                                budget,
                            )?;
                            budget.spend(
                                path.len()
                                    .saturating_add(state.facts.len())
                                    .saturating_add(state.observations.len()),
                            )?;
                            let prefix = path
                                .iter()
                                .map(|p| (p.variant, p.index))
                                .collect::<Vec<_>>();
                            state.facts.retain(|(root, observed, _)| {
                                *root != *local || !observed.starts_with(&prefix)
                            });
                            state.observations.retain(|v| {
                                self.observers[v.0 as usize].as_ref().is_none_or(
                                    |(root, observed)| {
                                        *root != *local || !observed.starts_with(&prefix)
                                    },
                                )
                            });
                        }
                    }
                    if matches!(instruction.kind, InstructionKind::VariantTagLocal { .. }) {
                        budget.spend(state.observations.len().saturating_add(1))?;
                        state
                            .observations
                            .insert(instruction.result.expect("typed tag"));
                    }
                }
                InstructionKind::UnpackVariant {
                    local,
                    variant: _,
                    fields,
                }
                | InstructionKind::UnpackTuple { local, fields } => {
                    let variant =
                        if let InstructionKind::UnpackVariant { variant, .. } = instruction.kind {
                            Some(variant)
                        } else {
                            None
                        };
                    if variant.is_some() {
                        self.selected(
                            state,
                            *local,
                            &[],
                            variant,
                            validate,
                            instruction.span,
                            budget,
                        )?;
                    }
                    if validate
                        && self.partial
                        && !state.locals[local.0 as usize].complete(budget)?
                    {
                        return Err(bad(
                            Some(instruction.span),
                            "atomic unpack requires every subtree initialized",
                        ));
                    }
                    if validate && state.initialized.get(local.0 as usize, budget)? != 2 {
                        return Err(bad(
                            Some(instruction.span),
                            "Unpack source is not initialized",
                        ));
                    }
                    budget.spend(fields.len())?;
                    if validate {
                        for local in fields {
                            if state.initialized.get(local.0 as usize, budget)? != 1 {
                                return Err(bad(
                                    Some(instruction.span),
                                    "every Unpack destination must be vacant on every incoming path before the atomic transfer",
                                ));
                            }
                        }
                    }
                    let payload = match state.locals[local.0 as usize].as_ref() {
                        Content::Sum(variants, _) if variant.is_some() => {
                            variants.get(&variant.unwrap())
                        }
                        Content::Product(fields) if variant.is_none() => Some(fields),
                        _ => None,
                    };
                    let mut prepared = Vec::with_capacity(fields.len());
                    for (index, target) in fields.iter().enumerate() {
                        prepared.push(if let Some(payload) = payload {
                            payload[index].duplicate(budget)?
                        } else {
                            content(
                                &self.f.locals[target.0 as usize].ty,
                                None,
                                &mut Vec::new(),
                                false,
                                budget,
                            )?
                        });
                    }
                    self.kill(state, *local, budget)?;
                    state.initialized.set(local.0 as usize, 1, budget)?;
                    for (&target, value) in fields.iter().zip(prepared) {
                        self.kill(state, target, budget)?;
                        if record {
                            bridge.unpack.push((
                                target,
                                value.fresh(),
                                if matches!(
                                    self.f.locals[target.0 as usize].ty,
                                    Type::Package { .. }
                                ) {
                                    direct_fields(&value, budget)?
                                } else {
                                    Vec::new()
                                },
                            ));
                        }
                        state.locals[target.0 as usize] = Arc::new(value);
                        state.initialized.set(target.0 as usize, 2, budget)?;
                    }
                }
                InstructionKind::DropTuple { local }
                | InstructionKind::DropEnum { local }
                | InstructionKind::DropManaged { local }
                | InstructionKind::DropPackage { local }
                | InstructionKind::DropErrorUnion { local }
                | InstructionKind::DropClosure { local }
                | InstructionKind::DropOwnedArray { local }
                | InstructionKind::StorageLive(local)
                | InstructionKind::StorageDead(local) => {
                    if validate
                        && self.f.call_kind == CallKind::Direct
                        && state.initialized.get(local.0 as usize, budget)? & 2 != 0
                        && matches!(
                            instruction.kind,
                            InstructionKind::DropPackage { .. }
                                | InstructionKind::DropTuple { .. }
                                | InstructionKind::DropEnum { .. }
                                | InstructionKind::DropErrorUnion { .. }
                        )
                        && state.locals[local.0 as usize]
                            .live_async(&self.f.locals[local.0 as usize].ty, budget)?
                    {
                        return Err(bad(
                            Some(instruction.span),
                            "Direct aggregate cannot implicitly drain a live asynchronous registration",
                        ));
                    }
                    if record
                        && self.owned_creation
                        && !matches!(
                            instruction.kind,
                            InstructionKind::StorageLive(_) | InstructionKind::StorageDead(_)
                        )
                    {
                        if crate::stored_flow::failure_shape(
                            &self.f.locals[local.0 as usize].ty,
                            budget,
                        )? {
                            bridge.empty_failure_cleanup =
                                Some(state.locals[local.0 as usize].no_cleanup_debt(budget)?);
                        }
                        bridge.task_cleanup = Some(
                            state.initialized.get(local.0 as usize, budget)? & 2 != 0
                                && state.locals[local.0 as usize]
                                    .live_task(&self.f.locals[local.0 as usize].ty, budget)?,
                        );
                    }
                    self.kill(state, *local, budget)?;
                    state.initialized.set(local.0 as usize, 1, budget)?;
                }
                InstructionKind::Call {
                    function,
                    arguments,
                } => {
                    let result_ty = &self.module.functions[function.0 as usize].return_type;
                    resources::charge_payload(result_ty, budget)?;
                    let mut returned = if result_ty.is_copy_value() {
                        // Copy callees have no owning-source summary. Its
                        // bottom placeholder is not an impossible runtime
                        // value: rebuild all actual Copy alternatives.
                        content(result_ty, None, &mut Vec::new(), false, budget)?
                    } else {
                        self.instantiate(
                            &self.summaries[function.0 as usize],
                            arguments,
                            state,
                            budget,
                        )?
                    };
                    returned.returned(budget)?;
                    if self.module.functions[function.0 as usize].result_mode == PassingMode::Create
                        && errors::managed_success(
                            &self.module.functions[function.0 as usize].return_type,
                        )
                        && let Content::Sum(variants, _) = &mut returned
                        && let Some(fields) = variants.get_mut(&1)
                    {
                        fields[0] = Content::Leaf(BTreeSet::from([Source::Fresh]));
                    }
                    result = Some(returned);
                    if matches!(
                        self.f
                            .values
                            .get(instruction.result.map_or(usize::MAX, |v| v.0 as usize)),
                        Some(Type::Managed(_))
                    ) && self.module.functions[function.0 as usize].result_mode
                        == PassingMode::Create
                    {
                        result = Some(Content::Leaf(BTreeSet::from([Source::Fresh])));
                    }
                }
                _ => {}
            }
            if let Some(output) = instruction.result {
                let value = if let Some(result) = result {
                    result
                } else {
                    content(
                        &self.f.values[output.0 as usize],
                        None,
                        &mut Vec::new(),
                        false,
                        budget,
                    )?
                };
                if record {
                    bridge.result = matches!(value, Content::Leaf(_)).then(|| value.fresh());
                    if matches!(self.f.values[output.0 as usize], Type::Package { .. }) {
                        bridge.package = direct_fields(&value, budget)?;
                    }
                }
                state
                    .values
                    .set(output.0 as usize, Arc::new(value), budget)?;
            }
            if record {
                bridges.push(bridge);
            }
        }
        Ok(bridges)
    }
}

fn run(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    summaries: &[Content],
    validate: bool,
    budget: &mut Budget,
) -> Check<Solved> {
    run_with_replacements(module, f, graph, summaries, validate, budget, None)
}
#[allow(clippy::too_many_arguments)]
fn run_with_replacements(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    summaries: &[Content],
    validate: bool,
    budget: &mut Budget,
    replacements: Option<&replacements::Context<'_>>,
) -> Check<Solved> {
    run_retaining(
        module,
        f,
        graph,
        summaries,
        validate,
        budget,
        replacements,
        false,
    )
    .map(|(solved, _)| solved)
}
#[allow(clippy::too_many_arguments)]
fn run_retaining<'m>(
    module: &'m TypedModule,
    f: &'m TypedFunction,
    graph: &'m FunctionGraph,
    summaries: &[Content],
    validate: bool,
    budget: &mut Budget,
    replacements: Option<&replacements::Context<'_>>,
    retain: bool,
) -> Check<(Solved, Option<replacements::checkpoint::Entry<'m>>)> {
    let checkpoint_start = if retain {
        Some(replacements::checkpoint::Start::new(budget)?)
    } else {
        None
    };

    let mut locals = Vec::new();
    budget.spend(
        f.locals
            .len()
            .saturating_add(f.blocks.len().saturating_mul(3)),
    )?;
    for local in &f.locals {
        locals.push(Arc::new(content(
            &local.ty,
            None,
            &mut Vec::new(),
            true,
            budget,
        )?));
    }
    let values = values::Values::build(f.values.len(), budget, |index, budget| {
        Ok(Arc::new(content(
            &f.values[index],
            None,
            &mut Vec::new(),
            true,
            budget,
        )?))
    })?;
    let mut captures = Vec::new();
    if let Some(environment) = f.environment
        && budget.direct_sum_capture_closures.contains(&environment.0)
    {
        let definition = &module.closures[environment.0 as usize];
        budget.spend(definition.captures.len())?;
        let mut direct_sum = false;
        for ty in &definition.captures {
            if budget.inline_store_root(ty)? {
                direct_sum |= dynamic_store::inline_sum_capture(ty, budget)?;
            }
        }
        if direct_sum {
            budget.spend(definition.captures.len())?;
            captures = vec![None; definition.captures.len()];
            for (index, ty) in definition.captures.iter().enumerate() {
                if budget.inline_store_root(ty)? && dynamic_store::inline_sum_capture(ty, budget)? {
                    let mut actual = content(ty, None, &mut Vec::new(), false, budget)?;
                    actual.mark_captured(index as u32, &mut Vec::new(), budget)?;
                    captures[index] = Some(Arc::new(actual));
                }
            }
        }
    }
    let mut state = State {
        initialized: initialization::Initialized::new(f.locals.len(), 1, budget)?,
        locals,
        values,
        captures,
        facts: BTreeSet::new(),
        observations: BTreeSet::new(),
        partial: None,
    };
    for (index, local) in f.parameters.iter().enumerate() {
        state.initialized.set(local.0 as usize, 2, budget)?;
        state.locals[local.0 as usize] = Arc::new(content(
            &f.locals[local.0 as usize].ty,
            Some(index as u32),
            &mut Vec::new(),
            false,
            budget,
        )?);
    }
    let mut partial = false;
    let mut address_tag = false;
    let mut address_replacement = false;
    let mut owned_creation = false;
    let mut definitions = vec![None; f.values.len()];
    let mut observers = vec![None; f.values.len()];
    for (b, block) in f.blocks.iter().enumerate() {
        for (i, instruction) in block.instructions.iter().enumerate() {
            budget.spend(1)?;
            address_tag |= matches!(instruction.kind, InstructionKind::PlaceTagAddress { .. });
            owned_creation |= matches!(instruction.kind, InstructionKind::TaskCreateOwned { .. });
            address_replacement |= matches!(
                instruction.kind,
                InstructionKind::ReservePlaceAddress { .. }
            );
            partial |= matches!(
                instruction.kind,
                InstructionKind::BorrowPlaceLocal { .. }
                    | InstructionKind::PlaceTagLocal { .. }
                    | InstructionKind::TakeTupleField { .. }
                    | InstructionKind::TakeVariantField { .. }
            );
            if let Some(v) = instruction.result {
                definitions[v.0 as usize] = Some((b, i));
                if let InstructionKind::VariantTagLocal { local, path } = &instruction.kind {
                    budget.spend(path.len())?;
                    observers[v.0 as usize] =
                        Some((*local, path.iter().map(|p| (p.variant, p.index)).collect()));
                }
            }
        }
    }
    let address_replacement_tags = address_tag && address_replacement;
    partial |= address_replacement_tags;
    if partial {
        state.partial = Some(partial_places::State::default());
    }
    let tagged_locals = if indexed_invalidation(budget) && partial {
        // Scan and index slots are prepaid before allocation. Each queued
        // child is paid before growth, including nested owning Products.
        budget.spend(f.locals.len().saturating_mul(2).saturating_add(1))?;
        let mut tagged = Vec::with_capacity(f.locals.len());
        for (local, declaration) in f.locals.iter().enumerate() {
            budget.spend(1)?;
            let mut pending = vec![&declaration.ty];
            while let Some(ty) = pending.pop() {
                budget.spend(1)?;
                match ty {
                    Type::Enum { .. } | Type::ErrorUnion { .. } => {
                        tagged.push(local);
                        break;
                    }
                    Type::BorrowedStorage { .. }
                        if budget.store_option_results || budget.stored_enum_addresses =>
                    {
                        let actual = shared_enum_physical(ty, budget)?;
                        if !std::ptr::eq(actual, ty) {
                            pending.push(actual);
                        }
                    }
                    Type::Tuple(fields) => {
                        budget.spend(fields.len())?;
                        pending.extend(fields);
                    }
                    Type::Package { fields, .. } => {
                        budget.spend(fields.len())?;
                        pending.extend(fields.iter().map(|(_, field)| field));
                    }
                    // Exactly the terminal forms in this kernel's Content
                    // constructor: Copy or Leaf, with no tag/route state.
                    _ => {}
                }
            }
        }
        Some(tagged)
    } else {
        None
    };
    let flow = Flow {
        f,
        module,
        definitions,
        observers,
        summaries,
        partial,
        replacements,
        address_replacement_tags,
        owned_creation,
        tagged_locals,
        tag_cache: (partial && indexed_invalidation(budget))
            .then(|| std::cell::RefCell::new(tag_cache::Cache::new(f))),
        place_meta: if partial {
            Some(partial_places::Meta::new(f, budget)?)
        } else {
            None
        },
    };
    let mut inputs = vec![None; f.blocks.len()];
    inputs[f.entry.0 as usize] = Some(state);
    let mut pending = resources::schedule::Worklist::new(
        graph,
        f.entry.0 as usize,
        budget.pod_stores || (budget.store_option_results && budget.ordinary_error_places),
        budget,
    )?;
    let mut queued = vec![false; f.blocks.len()];
    queued[f.entry.0 as usize] = true;
    let mut excluded_edges = BTreeSet::new();
    while let Some(block) = pending.pop(budget)? {
        queued[block] = false;
        let mut output = inputs[block].as_ref().unwrap().duplicate(budget)?;
        flow.block(block, &mut output, false, false, budget)?;
        for &next in &graph.successors[block] {
            let mut edge = output.duplicate(budget)?;
            if !flow.edge(&mut edge, block, next, budget)? {
                budget.spend(excluded_edges.len().saturating_add(1))?;
                excluded_edges.insert((block, next));
                continue;
            }
            if !excluded_edges.is_empty() {
                budget.spend(excluded_edges.len().saturating_add(1))?;
                // A later fixed-point input can widen a singleton tag. The
                // final proof must not retain an earlier provisional exclusion.
                excluded_edges.remove(&(block, next));
            }
            let changed = if let Some(input) = &mut inputs[next] {
                input.merge(&edge, budget)?
            } else {
                inputs[next] = Some(edge);
                true
            };
            if changed && !queued[next] {
                queued[next] = true;
                pending.push(next, budget)?;
            }
        }
    }
    #[cfg(test)]
    TEST_SUM_INPUTS.with(|capture| {
        let mut capture = capture.borrow_mut();
        if capture.is_some() {
            *capture = Some(inputs.clone());
        }
    });
    let checkpoint = if let Some(start) = checkpoint_start {
        start.save(
            module,
            f,
            graph,
            &flow,
            &inputs,
            &excluded_edges,
            replacements,
            budget,
        )?
    } else {
        None
    };
    let (result, rows) = replay_inputs(&flow, inputs, validate, budget)?;
    let checkpoint = if let Some(checkpoint) = checkpoint {
        checkpoint.finish(flow, budget)?
    } else {
        None
    };
    Ok(((result, rows, excluded_edges), checkpoint))
}

fn replay_inputs(
    flow: &Flow<'_>,
    inputs: Vec<Option<State>>,
    validate: bool,
    budget: &mut Budget,
) -> Check<(Content, Vec<Vec<Bridge>>)> {
    let f = flow.f;
    let module = flow.module;
    let mut result = content(&f.return_type, None, &mut Vec::new(), true, budget)?;
    let mut rows = vec![Vec::new(); f.blocks.len()];
    for (block, input) in inputs.into_iter().enumerate() {
        if let Some(mut state) = input {
            rows[block] = flow.block(block, &mut state, validate, validate, budget)?;
            if validate
                && matches!(f.blocks[block].terminator, Terminator::Return(_))
                && f.environment
                    .is_some_and(|id| module.closures[id.0 as usize].kind != CallableKind::FnOnce)
            {
                budget.spend(state.captures.len())?;
                for capture in state.captures.iter().flatten() {
                    if !capture.complete(budget)? {
                        return Err(bad(
                            Some(f.span),
                            "Fn/FnMut return leaves an unfilled direct Sum capture",
                        ));
                    }
                }
            }
            if let Terminator::Return(Some(value)) = f.blocks[block].terminator {
                result.merge(state.values.get(value.0 as usize, budget)?, budget)?;
            }
        }
    }
    Ok((result, rows))
}

#[cfg(test)]
thread_local! {
    static TEST_SUM_INPUTS: std::cell::RefCell<Option<Vec<Option<State>>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) fn test_schedule_equivalence(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
    limits: &Limits,
) -> bool {
    let solve = |profile| {
        TEST_SUM_INPUTS.with(|capture| *capture.borrow_mut() = Some(Vec::new()));
        let mut budget = Budget::new(limits);
        budget.pod_stores = profile == 1;
        budget.store_option_results = profile == 2;
        budget.ordinary_error_places = profile == 2;
        let found = run(module, f, graph, &[Content::Copy], true, &mut budget);
        let inputs = TEST_SUM_INPUTS.with(|capture| capture.borrow_mut().take().unwrap());
        (found, inputs)
    };
    let (old_result, old_inputs) = solve(0);
    let accepted = old_result.is_ok();
    for profile in [1, 2] {
        let (new_result, new_inputs) = solve(profile);
        assert!(
            old_inputs == new_inputs,
            "complete Sum block-input states differ"
        );
        match (old_result.clone(), new_result) {
            (Ok((left, left_rows, left_edges)), Ok((right, right_rows, right_edges))) => {
                assert_eq!(left_edges, right_edges, "Sum edge exclusions differ");
                assert!(left == right, "Sum return content differs");
                let rows = |rows: Vec<Vec<Bridge>>| {
                    rows.into_iter()
                        .map(|row| {
                            row.into_iter()
                                .map(|bridge| (bridge.result, bridge.package, bridge.unpack))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(rows(left_rows), rows(right_rows), "Sum bridge rows differ");
            }
            (Err(left), Err(right)) => assert_eq!(left, right, "Sum diagnostics differ"),
            _ => panic!("Sum acceptance differs"),
        }
    }
    accepted
}

#[cfg(test)]
pub(super) fn test_tagged_invalidation_budget(
    module: &TypedModule,
    f: &TypedFunction,
    graph: &FunctionGraph,
) {
    let limits = Limits::default();
    let solve = |steps| {
        let selected = Limits {
            max_analysis_steps: steps,
            ..limits.clone()
        };
        let mut budget = Budget::new(&selected);
        budget.pod_stores = true;
        run(module, f, graph, &[Content::Copy], true, &mut budget)
    };
    let (mut low, mut high) = (0usize, limits.max_analysis_steps);
    assert!(solve(high).is_ok());
    while low < high {
        let mid = low + (high - low) / 2;
        if solve(mid).is_ok() {
            high = mid;
        } else {
            low = mid + 1;
        }
    }
    assert!(solve(low).is_ok());
    assert_eq!(
        solve(low - 1).err().unwrap().category,
        zil_base::ResultCategory::ResourceLimit
    );
    assert!(solve(low).is_ok());
    println!("partial Sum indexed invalidation N={low}; N-1 ResourceLimit; fresh N pass");
}

pub(super) fn check_module(
    module: &TypedModule,
    graphs: &[FunctionGraph],
    budget: &mut Budget,
) -> Check<Proofs> {
    if budget.replacements {
        return replacements::check(module, graphs, budget);
    }
    if !budget.sums {
        return Ok(Proofs::default());
    }
    budget.spend(module.functions.len().saturating_mul(2))?;
    let mut summaries = Vec::new();
    for f in &module.functions {
        summaries.push(content(
            &f.return_type,
            None,
            &mut Vec::new(),
            true,
            budget,
        )?);
    }
    let count = module.functions.len();
    for f in &module.functions {
        resources::charge_payload(&f.return_type, budget)?;
    }
    budget.spend(count.saturating_mul(6))?;
    let needed = module
        .functions
        .iter()
        .map(|f| {
            Ok(f.foreign.is_none()
                && (matches!(f.return_type, Type::Managed(_) | Type::Package { .. })
                    || owned(&f.return_type)
                    || shared_enum_content(&f.return_type, budget)?))
        })
        .collect::<Check<Vec<_>>>()?;
    let mut users = vec![BTreeSet::new(); count];
    let mut callees = vec![BTreeSet::new(); count];
    for (index, f) in module.functions.iter().enumerate() {
        if !needed[index] {
            continue;
        }
        for block in &f.blocks {
            for instruction in &block.instructions {
                budget.spend(1)?;
                if let InstructionKind::Call { function, .. } = instruction.kind {
                    let callee = function.0 as usize;
                    if needed[callee] {
                        budget.spend(
                            users[callee]
                                .len()
                                .saturating_add(callees[index].len())
                                .saturating_add(2),
                        )?;
                        users[callee].insert(index);
                        callees[index].insert(callee);
                    }
                }
            }
        }
    }
    let mut visited = vec![0u8; count];
    let mut order = VecDeque::new();
    for index in 0..count {
        if !needed[index] || visited[index] != 0 {
            continue;
        }
        let mut stack = vec![(index, false)];
        while let Some((node, finish)) = stack.pop() {
            budget.spend(1)?;
            if finish {
                visited[node] = 2;
                order.push_back(node);
            } else if visited[node] == 0 {
                visited[node] = 1;
                budget.spend(callees[node].len().saturating_add(1))?;
                stack.push((node, true));
                for &callee in &callees[node] {
                    if visited[callee] == 0 {
                        stack.push((callee, false));
                    }
                }
            }
        }
    }
    let mut queued = needed.clone();
    while let Some(index) = order.pop_front() {
        queued[index] = false;
        let (result, _, _) = run(
            module,
            &module.functions[index],
            &graphs[index],
            &summaries,
            false,
            budget,
        )?;
        if summaries[index].merge(&result, budget)? {
            budget.spend(users[index].len())?;
            for &user in &users[index] {
                if !queued[user] {
                    queued[user] = true;
                    order.push_back(user);
                }
            }
        }
    }
    let mut rows = Vec::new();
    let mut excluded_edges = BTreeMap::new();
    for (index, f) in module.functions.iter().enumerate() {
        if f.foreign.is_some() || !budget.sum_function(f.id) {
            rows.push(Vec::new());
        } else {
            let (_, bridges, excluded) = run(module, f, &graphs[index], &summaries, true, budget)?;
            rows.push(bridges);
            if !excluded.is_empty() {
                budget.spend(excluded_edges.len().saturating_add(1))?;
                excluded_edges.insert(index, excluded);
            }
        }
    }
    Ok(Proofs {
        rows,
        excluded_edges,
        ..Proofs::default()
    })
}
