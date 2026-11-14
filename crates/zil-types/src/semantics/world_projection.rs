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
            StorageLifeShape {
                binder_count: child.binder_count,
                slots: Vec::new(),
                retained: (0..child.binder_count)
                    .map(|class| StorageRetainedBinding {
                        path: vec![selector],
                        class,
                        binder: class,
                    })
                    .collect(),
                nominal_arguments: Vec::new(),
            }
        }
        _ => {
            let value = world.zero_at(ty, depth + 1)?;
            let SemanticTypeValue::Resolved { life, .. } = value else {
                unreachable!()
            };
            life
        }
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
pub(super) fn pointee(
    world: &mut World<'_, '_>,
    parent: &SemanticTypeValue,
) -> R<SemanticTypeValue> {
    let inner = match World::type_of(parent)? {
        Type::Address { inner, .. } | Type::Desc(inner) | Type::Managed(inner) => inner,
        _ => {
            return Err(bad(
                "carrier projection has no actual address, description or owner",
            ));
        }
    };
    let shape = type_shape(world, inner)?;
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
) -> R<SemanticTypeValue> {
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
    match field {
        Type::Address { inner, .. } => {
            let SemanticTypeValue::Resolved { life, classes, .. } = parent else {
                unreachable!()
            };
            world.pay(life.slots.len())?;
            let mut rows = life.slots.iter().filter(|row| row.path == [selector]);
            let row = rows.next().ok_or_else(|| {
                pending("projected outer address requires its original complete occurrence")
            })?;
            if rows.next().is_some() {
                return Err(bad("field projection repeats an outer occurrence"));
            }
            let image = classes
                .get(row.binder as usize)
                .ok_or_else(|| bad("field outer class exceeds parent domain"))?;
            crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
            let shape = type_shape(world, inner)?;
            let images = mapped(
                world,
                parent,
                &[prefix, LifeTypeProjection::Pointee],
                shape.binder_count,
            )?;
            let child = world.resolved(inner, shape, images)?;
            world.address(field, image.clone(), &child)
        }
        _ => {
            let shape = type_shape(world, field)?;
            if shape.binder_count == 0 {
                return world.resolved(field, shape, Vec::new());
            }
            let images = mapped(world, parent, &[prefix], shape.binder_count)?;
            world.resolved(field, shape, images)
        }
    }
}

pub(super) fn field(
    world: &mut World<'_, '_>,
    parent: &SemanticTypeValue,
    index: u32,
) -> R<SemanticTypeValue> {
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
) -> R<SemanticTypeValue> {
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
