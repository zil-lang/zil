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
    world.pay((count as usize).saturating_mul(life.retained.len().saturating_add(1)))?;
    let mut images = Vec::with_capacity(count as usize);
    for class in 0..count {
        let mut rows = life
            .retained
            .iter()
            .filter(|row| row.path == path && row.class == class);
        let row = rows
            .next()
            .ok_or_else(|| bad("projection omits a complete child intrinsic class"))?;
        if rows.next().is_some() {
            return Err(bad("projection repeats a child intrinsic class"));
        }
        let image = classes
            .get(row.binder as usize)
            .ok_or_else(|| bad("projection class exceeds its parent domain"))?;
        crate::dyn_metadata::semantic_operand_snapshot(image, world.limits, world.left, 1)?;
        images.push(image.clone());
    }
    Ok(images)
}

fn child_shape(world: &mut World<'_, '_>, ty: &Type) -> R<StorageLifeShape> {
    Ok(match ty {
        Type::BorrowedStorage { life, .. } => {
            world.pay(
                life.slots
                    .len()
                    .saturating_add(life.retained.len())
                    .saturating_add(life.nominal_arguments.len()),
            )?;
            for path in life
                .slots
                .iter()
                .map(|row| row.path.len())
                .chain(life.retained.iter().map(|row| row.path.len()))
                .chain(life.nominal_arguments.iter().map(|row| row.path.len()))
            {
                world.pay(path)?;
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
        _ => {
            let value = world.zero(ty)?;
            let SemanticTypeValue::Resolved { life, .. } = value else {
                unreachable!()
            };
            life
        }
    })
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
    }
    .ok_or_else(|| bad("actual projection is outside its complete product or sum"))?;
    let prefix = LifeTypeProjection::Place(selector);
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
            let shape = child_shape(world, inner)?;
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
            let shape = child_shape(world, field)?;
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
    place(world, parent, PlaceProjection::Field(index))
}
