//! Projection keeps the selected child's complete class domain, including
//! phantom classes. Equal parent images do not renumber or merge child classes.
use super::*;

fn mapped(
    world: &mut World<'_, '_>,
    parent: &SemanticTypeValue,
    path: &[LifeTypeProjection],
    count: u32,
) -> R<Vec<SemanticClassExpr>> {
    let SemanticTypeValue::Resolved { life, classes, .. } = parent else {
        return Err(bad("projection parent retains an unresolved whole type"));
    };
    world.pay(
        (count as usize)
            .saturating_mul(life.retained.len().saturating_add(1))
            .saturating_add(
                life.retained
                    .len()
                    .saturating_mul(path.len().saturating_add(1)),
            ),
    )?;
    for row in &life.retained {
        if row.path == path && row.class >= count {
            return Err(bad(
                "projection invents a child intrinsic class beyond its complete domain",
            ));
        }
    }
    let mut binders = std::collections::BTreeSet::new();
    let mut images = Vec::with_capacity(count as usize);
    for class in 0..count {
        let mut rows = life
            .retained
            .iter()
            .filter(|row| row.path == path && row.class == class);
        let row = rows
            .next()
            .ok_or_else(|| bad("projection omits a complete child intrinsic class"))?;
        if rows.next().is_some() || !binders.insert(row.binder) {
            return Err(bad(
                "projection repeats or collapses a child intrinsic class",
            ));
        }
        let image = classes
            .get(row.binder as usize)
            .ok_or_else(|| bad("projection class exceeds its parent domain"))?;
        crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
        images.push(image.clone());
    }
    Ok(images)
}

/// Canonical actual whole-type shape. This is a projection recipe only;
/// original construction, retained child graphs and parent images are checked
/// before any resulting value can participate in the world.
pub(super) fn type_shape(world: &mut World<'_, '_>, ty: &Type) -> R<StorageLifeShape> {
    type_shape_at(world, ty, 0)
}
pub(super) fn type_shape_at(
    world: &mut World<'_, '_>,
    ty: &Type,
    depth: usize,
) -> R<StorageLifeShape> {
    world.pay(1)?;
    if depth > world.limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "actual carrier projection depth exceeded",
            None,
        ));
    }
    Ok(match ty {
        Type::BorrowedStorage { life, .. } => {
            world.pay(
                life.slots
                    .len()
                    .saturating_add(life.retained.len())
                    .saturating_add(life.nominal_arguments.len()),
            )?;
            for row in &life.slots {
                world.pay(row.path.len())?;
            }
            for row in &life.retained {
                world.pay(row.path.len())?;
            }
            for row in &life.nominal_arguments {
                world.pay(row.path.len())?;
            }
            life.clone()
        }
        Type::Closure(id) => {
            let (definition, _) = world.child(*id)?;
            let header = definition
                .storage_life
                .as_ref()
                .ok_or_else(|| pending("projected closure lacks its complete domain"))?;
            StorageLifeShape {
                binder_count: header.fixed_binder_count,
                ..empty()
            }
        }
        Type::Task {
            result,
            retained_life,
        } => {
            retained_snapshot(world, retained_life)?;
            let child = type_shape_at(world, result, depth + 1)?;
            world.pay(
                (child.binder_count as usize)
                    .saturating_mul(retained_life.retained.len().saturating_add(1)),
            )?;
            let mut images = std::collections::BTreeSet::new();
            for class in 0..child.binder_count {
                let mut rows = retained_life.retained.iter().filter(|row| {
                    row.path.as_slice() == [LifeTypeProjection::Result] && row.class == class
                });
                let row = rows.next().ok_or_else(|| {
                    bad("Task projection omits a complete Result intrinsic class")
                })?;
                if rows.next().is_some()
                    || row.binder >= retained_life.binder_count
                    || !images.insert(row.binder)
                {
                    return Err(bad(
                        "Task projection repeats or collapses a Result intrinsic class",
                    ));
                }
            }
            for row in &retained_life.retained {
                if row.path.as_slice() == [LifeTypeProjection::Result]
                    && row.class >= child.binder_count
                {
                    return Err(bad("Task projection invents a Result intrinsic class"));
                }
            }
            StorageLifeShape {
                binder_count: retained_life.binder_count,
                ..empty()
            }
        }
        Type::MutexGuard {
            payload,
            retained_life,
        } => {
            retained_snapshot(world, retained_life)?;
            let child = type_shape_at(world, payload, depth + 1)?;
            if child.binder_count != 0 {
                return Err(pending(
                    "guard projection requires its complete original payload port graph",
                ));
            }
            StorageLifeShape {
                binder_count: retained_life.binder_count,
                ..empty()
            }
        }
        Type::Dyn { interface } | Type::OwnedDyn { interface } => {
            world.pay(world.module.dyn_interfaces.len())?;
            let mut rows = world
                .module
                .dyn_interfaces
                .iter()
                .filter(|row| row.id == *interface);
            let row = rows
                .next()
                .ok_or_else(|| bad("dynamic projection has no actual interface"))?;
            if rows.next().is_some() {
                return Err(bad("dynamic projection has ambiguous interface identity"));
            }
            retained_snapshot(world, &row.retained_life)?;
            StorageLifeShape {
                binder_count: row.retained_life.binder_count,
                slots: Vec::new(),
                retained: row.retained_life.retained.clone(),
                nominal_arguments: row.retained_life.nominal_arguments.clone(),
            }
        }
        Type::Managed(child)
        | Type::Mutex { payload: child }
        | Type::OwnedStore { element: child }
        | Type::Registration { environment: child }
        | Type::AsyncRegistration { environment: child } => {
            let child = type_shape_at(world, child, depth + 1)?;
            world.pay(child.binder_count as usize)?;
            let selector = if matches!(ty, Type::OwnedStore { .. }) {
                LifeTypeProjection::Element
            } else {
                LifeTypeProjection::Pointee
            };
            let mut shape = StorageLifeShape {
                binder_count: child.binder_count,
                ..empty()
            };
            let map = (0..child.binder_count).collect::<Vec<_>>();
            super::super::materialize::inherit_child_shape(
                &mut shape,
                &child,
                selector,
                &map,
                world.limits,
                world.left,
            )?;
            shape
        }
        _ => world.zero_shape_at(ty, depth + 1)?,
    })
}
fn retained_snapshot(world: &mut World<'_, '_>, shape: &RetainedLifeShape) -> R<()> {
    world.pay(
        shape
            .ports
            .len()
            .saturating_add(shape.retained.len())
            .saturating_add(shape.nominal_arguments.len())
            .saturating_add(1),
    )?;
    for port in &shape.ports {
        inference::charge_type_snapshot(&port.leaf, Span::default(), world.limits, world.left, 1)?;
        if port.binder >= shape.binder_count {
            return Err(bad(
                "retained port class exceeds the complete carrier domain",
            ));
        }
    }
    for row in &shape.retained {
        world.pay(row.path.len())?;
        if row.binder >= shape.binder_count {
            return Err(bad(
                "retained child class exceeds the complete carrier domain",
            ));
        }
    }
    for row in &shape.nominal_arguments {
        world.pay(row.path.len())?;
        if row.binder >= shape.binder_count {
            return Err(bad(
                "retained nominal class exceeds the complete carrier domain",
            ));
        }
    }
    Ok(())
}
/// A raw nested Address has no embedded life annotation. Recover its child
/// scope only from the already checked parent's complete retained directory;
/// physical pointer depth alone cannot decide whether an outer role is Static.
fn address_child_shape(
    world: &mut World<'_, '_>,
    parent: &StorageLifeShape,
    prefix: &mut Vec<LifeTypeProjection>,
    ty: &Type,
    depth: usize,
) -> R<StorageLifeShape> {
    world.pay(1)?;
    if depth > world.limits.max_call_depth || prefix.len() > world.limits.max_call_depth {
        return Err(Diagnostic::limit(
            Phase::Types,
            "nested pointee domain depth exceeded",
            None,
        ));
    }
    let (payload, mutable) = match ty {
        Type::Address { mutable, inner } => (inner.as_ref(), *mutable),
        Type::Desc(inner) => (inner.as_ref(), true),
        _ => return type_shape_at(world, ty, depth + 1),
    };
    let mut binders = BTreeMap::new();
    let mut reverse = BTreeMap::new();
    for row in &parent.retained {
        world.pay(
            row.path
                .len()
                .saturating_add(prefix.len())
                .saturating_add(1),
        )?;
        if row.path != *prefix {
            continue;
        }
        world.pay(index::search(binders.len()).saturating_add(index::search(reverse.len())))?;
        if row.binder >= parent.binder_count
            || binders.insert(row.class, row.binder).is_some()
            || reverse.insert(row.binder, row.class).is_some()
        {
            return Err(bad(
                "nested pointee repeats or collapses its complete child domain",
            ));
        }
    }
    let count =
        u32::try_from(binders.len()).map_err(|_| bad("nested pointee class count overflows"))?;
    if count as usize > world.limits.max_nodes {
        return Err(Diagnostic::limit(
            Phase::Types,
            "nested pointee node limit exceeded",
            None,
        ));
    }
    for (ordinal, class) in binders.keys().enumerate() {
        world.pay(1)?;
        if *class != ordinal as u32 {
            return Err(bad("nested pointee omits a full child class ordinal"));
        }
    }
    prefix.push(LifeTypeProjection::Pointee);
    let child = if matches!(payload, Type::Address { .. } | Type::Desc(_)) {
        address_child_shape(world, parent, prefix, payload, depth + 1)?
    } else {
        type_shape_at(world, payload, depth + 1)?
    };
    // Verify the full immediate payload block before inferring its one outer
    // occurrence. Its class count includes phantom and nominal-only classes.
    let outer = count
        .checked_sub(child.binder_count)
        .filter(|extra| *extra <= 1)
        .ok_or_else(|| bad("nested pointee changes its whole intrinsic child count"))?;
    if outer == 0 && mutable {
        return Err(bad("nested mutable pointee invents a Static outer role"));
    }
    for class in 0..child.binder_count {
        let mut found = None;
        for row in &parent.retained {
            world.pay(
                row.path
                    .len()
                    .saturating_add(prefix.len())
                    .saturating_add(1),
            )?;
            if row.path == *prefix && row.class == class {
                if found.replace(row.binder).is_some() {
                    return Err(bad("nested pointee repeats its payload class mapping"));
                }
            }
        }
        let binder =
            found.ok_or_else(|| bad("nested pointee has no complete payload class mapping"))?;
        if reverse.get(&binder)
            != Some(
                &outer
                    .checked_add(class)
                    .ok_or_else(|| bad("nested pointee payload ordinal overflows"))?,
            )
        {
            return Err(bad(
                "nested pointee changes its independent payload domain order",
            ));
        }
    }
    prefix.pop();
    let mut shape = StorageLifeShape {
        binder_count: count,
        ..empty()
    };
    if outer == 1 {
        shape.slots.push(StorageLifeSlot {
            path: Vec::new(),
            binder: 0,
            mode: match ty {
                Type::Desc(_) => StoredBorrowMode::WritableDescription,
                Type::Address { mutable: true, .. } => StoredBorrowMode::ExclusiveAddress,
                _ => StoredBorrowMode::SharedAddress,
            },
        });
    }
    for row in &parent.retained {
        world.pay(
            row.path
                .len()
                .saturating_add(prefix.len())
                .saturating_add(1),
        )?;
        if row.path.len() <= prefix.len() || !row.path.starts_with(prefix) {
            continue;
        }
        let binder = *reverse
            .get(&row.binder)
            .ok_or_else(|| bad("nested retained occurrence leaves its complete child domain"))?;
        shape.retained.push(StorageRetainedBinding {
            path: row.path[prefix.len()..].to_vec(),
            class: row.class,
            binder,
        });
    }
    for row in &parent.nominal_arguments {
        world.pay(
            row.path
                .len()
                .saturating_add(prefix.len())
                .saturating_add(1),
        )?;
        if !row.path.starts_with(prefix) {
            continue;
        }
        let binder = *reverse
            .get(&row.binder)
            .ok_or_else(|| bad("nested nominal occurrence leaves its complete child domain"))?;
        shape.nominal_arguments.push(NominalLifeArgument {
            path: row.path[prefix.len()..].to_vec(),
            declaration: row.declaration,
            argument: row.argument,
            binder,
        });
    }
    let mut canonical = StorageLifeShape {
        binder_count: count,
        slots: shape.slots.clone(),
        ..empty()
    };
    world.pay(child.binder_count as usize + shape.slots.len())?;
    let map = (0..child.binder_count)
        .map(|class| class + outer)
        .collect::<Vec<_>>();
    super::super::materialize::inherit_child_shape(
        &mut canonical,
        &child,
        LifeTypeProjection::Pointee,
        &map,
        world.limits,
        world.left,
    )?;
    world.pay(
        shape
            .slots
            .len()
            .saturating_mul(3)
            .saturating_add(shape.retained.len().saturating_mul(3))
            .saturating_add(shape.nominal_arguments.len().saturating_mul(5))
            .saturating_add(1),
    )?;
    for row in &shape.retained {
        world.pay(row.path.len())?;
    }
    for row in &shape.nominal_arguments {
        world.pay(row.path.len())?;
    }
    if shape != canonical {
        return Err(bad(
            "nested pointee changes its complete retained or nominal occurrence forest",
        ));
    }
    Ok(shape)
}

pub(super) fn pointee(world: &mut World<'_, '_>, parent: &SemanticTypeValue) -> R<SharedType> {
    let inner = match World::type_of(parent)? {
        Type::Address { inner, .. } | Type::Desc(inner) | Type::Managed(inner) => inner,
        _ => {
            return Err(bad(
                "carrier projection has no actual address, description or owner",
            ));
        }
    };
    let shape = if matches!(inner.as_ref(), Type::Address { .. } | Type::Desc(_)) {
        let SemanticTypeValue::Resolved { life, .. } = parent else {
            return Err(bad("nested pointee has no complete parent scope"));
        };
        address_child_shape(
            world,
            life,
            &mut vec![LifeTypeProjection::Pointee],
            inner,
            0,
        )?
    } else {
        type_shape(world, inner)?
    };
    let images = mapped(
        world,
        parent,
        &[LifeTypeProjection::Pointee],
        shape.binder_count,
    )?;
    world.resolved(inner, shape, images)
}

pub(super) fn place(
    world: &mut World<'_, '_>,
    parent: &SemanticTypeValue,
    selector: PlaceProjection,
) -> R<SharedType> {
    let ty = World::type_of(parent)?;
    let field = match (stored::physical(ty), selector) {
        (
            Type::Record { fields, .. } | Type::Package { fields, .. },
            PlaceProjection::Field(index),
        ) => fields.get(index as usize).map(|(_, ty)| ty),
        (Type::Tuple(fields), PlaceProjection::Field(index)) => fields.get(index as usize),
        (Type::Array { element, length }, PlaceProjection::Field(index)) if index < *length => {
            Some(element.as_ref())
        }
        (
            Type::Enum { variants, .. },
            PlaceProjection::Variant(VariantProjection { variant, index }),
        ) => variants
            .get(variant as usize)
            .and_then(|(_, fields)| fields.get(index as usize)),
        (Type::ErrorUnion { error, result }, PlaceProjection::ErrorArm(is_error)) => {
            Some(if is_error {
                error.as_ref()
            } else {
                result.as_ref()
            })
        }
        _ => None,
    };
    let field = match field {
        Some(field) => field,
        None => {
            inference::charge_type_snapshot(ty, Span::default(), world.limits, world.left, 1)?;
            let mut error = bad("actual projection is outside its complete product or sum");
            error.notes.push(format!(
                "actual projection point {:?}; selector {selector:?}; parent complete type {ty:?}",
                world.current_point
            ));
            return Err(error);
        }
    };
    let prefix = if matches!(stored::physical(ty), Type::Array { .. }) {
        LifeTypeProjection::Element
    } else {
        LifeTypeProjection::Place(selector)
    };
    // The parent is an already checked complete value. Extract its full
    // selected subdomain; a raw physical Type cannot decide Static versus a
    // variable outer address, or reconstruct an erased phantom child domain.
    let SemanticTypeValue::Resolved { life, classes, .. } = parent else {
        return Err(bad("actual field parent retains an unresolved whole type"));
    };
    let mut binders = BTreeMap::new();
    let mut reverse = BTreeMap::new();
    for row in &life.retained {
        world.pay(row.path.len().saturating_mul(3).saturating_add(1))?;
        if row.path.as_slice() != [prefix] { continue; }
        world.pay(index::search(binders.len()).saturating_add(index::search(reverse.len())).saturating_add(2))?;
        if binders.len() >= world.limits.max_nodes || reverse.len() >= world.limits.max_nodes {
            return Err(Diagnostic::limit(Phase::Types, "projected complete field domain exceeds node limit", None));
        }
        if row.binder >= life.binder_count
            || binders.insert(row.class, row.binder).is_some()
            || reverse.insert(row.binder, row.class).is_some()
        {
            return Err(bad("field repeats or collapses its complete intrinsic class map"));
        }
    }
    // A qualified nominal Type argument can expose its one outer Address
    // occurrence through the exact physical slot. Its complete parent domain
    // is anchored at TypeArgument, so the equivalent Place boundary row is
    // optional (nominal_catalog::field_type_class checks the same convention).
    // This recovers only that real outer class; an inner or phantom class
    // still requires its complete retained projection.
    let mut slot_only_shape = None;
    if binders.is_empty() {
        let address = match field {
            Type::Address { mutable, inner } => Some((
                inner.as_ref(),
                if *mutable { StoredBorrowMode::ExclusiveAddress }
                else { StoredBorrowMode::SharedAddress },
            )),
            Type::Desc(inner) => Some((inner.as_ref(), StoredBorrowMode::WritableDescription)),
            _ => None,
        };
        if let Some((inner, mode)) = address {
            let mut selected = None;
            for slot in &life.slots {
                world.pay(slot.path.len().saturating_add(1))?;
                if slot.path.as_slice() == [selector] {
                    if slot.mode != mode || slot.binder >= life.binder_count
                        || selected.replace(slot.binder).is_some()
                    {
                        return Err(bad("field outer slot changes its exact mode or class"));
                    }
                }
            }
            if let Some(binder) = selected {
                world.view.ordinary_zero(
                    inner, world.limits, world.left, &world.ledger.ordinary_stats,
                )?;
                let child = type_shape(world, inner)?;
                if child.binder_count != 0 {
                    return Err(bad("slot-only field omits its complete nonzero child domain"));
                }
                if world.limits.max_nodes == 0 {
                    return Err(Diagnostic::limit(Phase::Types, "projected complete field domain exceeds node limit", None));
                }
                world.pay(index::search(binders.len()).saturating_add(index::search(reverse.len())).saturating_add(3))?;
                binders.insert(0, binder);
                reverse.insert(binder, 0);
                let mut canonical = StorageLifeShape {
                    binder_count: 1,
                    slots: vec![StorageLifeSlot { path: Vec::new(), binder: 0, mode }],
                    ..empty()
                };
                super::super::materialize::inherit_child_shape(
                    &mut canonical, &child, LifeTypeProjection::Pointee, &[],
                    world.limits, world.left,
                )?;
                slot_only_shape = Some(canonical);
            }
        }
    }
    let count = u32::try_from(binders.len()).map_err(|_| bad("field class count overflows"))?;
    let mut images = Vec::new();
    world.pay(count as usize)?;
    for class in 0..count {
        world.pay(index::search(binders.len()))?;
        let binder = *binders.get(&class).ok_or_else(|| bad("field intrinsic class ordinals are sparse"))?;
        let image = classes.get(binder as usize).ok_or_else(|| bad("field intrinsic class leaves parent images"))?;
        crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
        images.push(image.clone());
    }
    let mut shape = StorageLifeShape { binder_count: count, ..empty() };
    for row in &life.slots {
        world.pay(row.path.len().saturating_mul(3).saturating_add(1))?;
        let Some(tail) = row.path.strip_prefix(&[selector]) else { continue; };
        world.pay(index::search(reverse.len()))?;
        let binder = *reverse.get(&row.binder).ok_or_else(|| bad("field physical occurrence leaves its complete class map"))?;
        shape.slots.push(StorageLifeSlot { path: tail.to_vec(), binder, mode: row.mode });
    }
    for row in &life.retained {
        world.pay(row.path.len().saturating_mul(3).saturating_add(1))?;
        let Some(tail) = row.path.strip_prefix(&[prefix]) else { continue; };
        if tail.is_empty() { continue; }
        world.pay(index::search(reverse.len()))?;
        let binder = *reverse.get(&row.binder).ok_or_else(|| bad("field child occurrence leaves its complete class map"))?;
        shape.retained.push(StorageRetainedBinding { path: tail.to_vec(), class: row.class, binder });
    }
    for row in &life.nominal_arguments {
        world.pay(row.path.len().saturating_mul(3).saturating_add(1))?;
        let Some(tail) = row.path.strip_prefix(&[prefix]) else { continue; };
        world.pay(index::search(reverse.len()))?;
        let binder = *reverse.get(&row.binder).ok_or_else(|| bad("field nominal occurrence leaves its complete class map"))?;
        shape.nominal_arguments.push(NominalLifeArgument { path: tail.to_vec(), declaration: row.declaration, argument: row.argument, binder });
    }
    // Explicit intrinsic carriers must agree with their immutable declaration
    // domain as well. Inline compounds inherit the complete parent subforest;
    // no context-free zero query is used for a raw Tuple containing addresses.
    let canonical = if slot_only_shape.is_some() { slot_only_shape } else { match field {
        Type::Address { .. } | Type::Desc(_) => Some(address_child_shape(world, life, &mut vec![prefix], field, 0)?),
        Type::Tuple(_) | Type::Record { .. } | Type::Package { .. }
            | Type::Array { .. } | Type::Enum { .. } | Type::ErrorUnion { .. } => None,
        _ => Some(type_shape(world, field)?),
    } };
    if let Some(canonical) = canonical {
        world.pay(shape.slots.len().saturating_mul(3).saturating_add(shape.retained.len().saturating_mul(3)).saturating_add(shape.nominal_arguments.len().saturating_mul(5)).saturating_add(1))?;
        for row in &shape.slots { world.pay(row.path.len())?; }
        for row in &shape.retained { world.pay(row.path.len())?; }
        for row in &shape.nominal_arguments { world.pay(row.path.len())?; }
        if canonical != shape {
            return Err(bad("field changes its complete original occurrence or child domain"));
        }
    }
    world.resolved(field, shape, images)

}

pub(super) fn field(
    world: &mut World<'_, '_>,
    parent: &SemanticTypeValue,
    index: u32,
) -> R<SharedType> {
    // Field operations on a managed head address its real payload. This is
    // the same single boundary accepted by aggregates::field_ref; its entire
    // child domain comes from the retained Pointee graph, never from a zero
    // shape or the emitted field's concrete type.
    if matches!(World::type_of(parent)?, Type::Managed(_)) {
        let payload = pointee(world, parent)?;
        place(world, &payload, PlaceProjection::Field(index))
    } else {
        place(world, parent, PlaceProjection::Field(index))
    }
}

fn capture_class(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    header: &StorageLifeContract,
    position: &SignatureLifeClass,
) -> R<SemanticClassExpr> {
    world.pay(
        header
            .classes
            .len()
            .saturating_mul(position.path.len().saturating_add(1)),
    )?;
    let mut rows = header
        .classes
        .iter()
        .filter(|row| row.position == *position);
    let row = rows
        .next()
        .ok_or_else(|| bad("capture omits a complete persistent child class occurrence"))?;
    if rows.next().is_some() || !matches!(row.binder, StorageBinder::Fixed(_)) {
        return Err(bad(
            "capture persistent class is ambiguous or invocation-local",
        ));
    }
    world.binding(instance, row.binder)
}
pub(super) fn capture(
    world: &mut World<'_, '_>,
    instance: &SemanticInstance,
    index: u32,
    ty: &Type,
    header: &StorageLifeContract,
) -> R<SharedType> {
    // A physical Static leaf inside an inline capture is not a variable slot.
    // Reuse the same exact creator/environment correspondence that formed the
    // header, rather than reconstructing this compound from its erased Type.
    world.pay(header.static_bindings.len())?;
    if header.static_bindings.iter().any(|position| position.base == StorageBase::Capture(index) && !position.path.is_empty()) {
        let value = super::super::headers::original_capture_value(
            world.program, &world.view, world.ledger, instance, index, ty,
            world.limits, world.left,
        )?;
        return Ok(std::rc::Rc::new(value));
    }
    let base = StorageBase::Capture(index);
    if let Type::Address { inner, .. } | Type::Desc(inner) = ty {
        let shape = type_shape(world, inner)?;
        world.pay(shape.binder_count as usize)?;
        let mut classes = Vec::new();
        for class in 0..shape.binder_count {
            classes.push(capture_class(
                world,
                instance,
                header,
                &SignatureLifeClass {
                    base,
                    path: vec![LifeTypeProjection::Pointee],
                    class,
                },
            )?);
        }
        let child = world.resolved(inner, shape, classes)?;
        world.pay(
            header
                .bindings
                .len()
                .saturating_add(header.static_bindings.len()),
        )?;
        let position = StoragePosition {
            base,
            path: Vec::new(),
        };
        let mut rows = header
            .bindings
            .iter()
            .filter(|row| row.position == position);
        if let Some(row) = rows.next() {
            if rows.next().is_some() || !matches!(row.binder, StorageBinder::Fixed(_)) {
                return Err(bad("capture outer head is ambiguous or invocation-local"));
            }
            let image = world.binding(instance, row.binder)?;
            return world.address(ty, image, &child);
        }
        if !matches!(ty, Type::Address { mutable: false, .. })
            || header
                .static_bindings
                .iter()
                .filter(|row| **row == position)
                .count()
                != 1
        {
            return Err(bad(
                "capture outer occurrence lacks its exact persistent or static source",
            ));
        }
        // A program-static head has no variable outer binder. The actual
        // constructor must still match this exact whole capture value.
        let parent = world.resolved(ty, empty(), Vec::new())?;
        return world.address_projection(ty, &parent, &child);
    }
    let shape = type_shape(world, ty)?;
    let whole = matches!(
        ty,
        Type::BorrowedStorage { .. }
            | Type::Closure(_)
            | Type::Task { .. }
            | Type::MutexGuard { .. }
            | Type::Dyn { .. }
            | Type::OwnedDyn { .. }
    );
    world.pay(
        (shape.binder_count as usize).saturating_mul(shape.retained.len().saturating_add(1)),
    )?;
    let mut images = Vec::new();
    for class in 0..shape.binder_count {
        let position = if whole {
            SignatureLifeClass {
                base,
                path: Vec::new(),
                class,
            }
        } else {
            let mut rows = shape.retained.iter().filter(|row| row.binder == class);
            let row = rows
                .next()
                .ok_or_else(|| bad("capture whole carrier omits a persistent class projection"))?;
            if rows.next().is_some() {
                return Err(pending(
                    "capture class has several distinct original component occurrences",
                ));
            }
            world.pay(row.path.len())?;
            SignatureLifeClass {
                base,
                path: row.path.clone(),
                class: row.class,
            }
        };
        images.push(capture_class(world, instance, header, &position)?);
    }
    world.resolved(ty, shape, images)
}
