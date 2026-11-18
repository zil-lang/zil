//! Complete physical retained boundaries. Target roots remain CFG obligations.
use super::*;

pub(super) fn closure_domain(module: &TypedModule, id: ClosureId, b: &mut Budget) -> CompileResult<u32> {
    fee(b, 1)?;
    if module.semantic.is_none() {
        return Err(failure("stored callable shape has no independent original context"));
    }
    let definition = module.closures.get(id.0 as usize).filter(|row| row.id == id)
        .ok_or_else(|| failure("stored callable shape has no exact actual definition"))?;
    let entry = module.functions.get(definition.function.0 as usize).filter(|row|
        row.id == definition.function && row.environment == Some(id))
        .ok_or_else(|| failure("stored callable shape has no exact actual entry"))?;
    let header = definition.storage_life.as_ref()
        .ok_or_else(|| failure("stored callable shape has no complete Fixed domain"))?;
    let actual = entry.storage_life.as_ref()
        .ok_or_else(|| failure("stored callable entry has no complete Fixed domain"))?;
    crate::signature_life::charge(header, b)?;
    crate::signature_life::charge(actual, b)?;
    if actual != header {
        return Err(failure("stored callable shape and entry have different complete domains"));
    }
    Ok(header.fixed_binder_count)
}

pub(super) fn expected(
    module: &TypedModule,
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<Vec<(Vec<LifeTypeProjection>, u32)>> {
    let mut out = Vec::new();
    scan(module, ty, &mut Vec::new(), 0, false, b, &mut out)?;
    Ok(out)
}

fn scan(
    module: &TypedModule,
    ty: &Type,
    path: &mut Vec<LifeTypeProjection>,
    depth: usize,
    owning_field: bool,
    b: &mut Budget,
    out: &mut Vec<(Vec<LifeTypeProjection>, u32)>,
) -> CompileResult<()> {
    fee(b, 1)?;
    if depth > b.partial_depth {
        return Err(failure("physical retained type depth exceeded"));
    }
    match ty {
        Type::BorrowedStorage { storage, life } => {
            if !matches!(storage.as_ref(), Type::Package { .. } | Type::Tuple(_))
                && !shared_record_child(ty, b)?
                && !pure(ty, b)?
            {
                return Err(failure(
                    "retained physical boundary has an address or ownership leaf",
                ));
            }
            physical(module, ty, b)?;
            fee(b, life.binder_count as usize)?;
            for class in 0..life.binder_count {
                fee(b, path.len().saturating_add(1))?;
                out.push((path.clone(), class));
            }
        }
        Type::Closure(id) if owning_field => {
            let count = closure_domain(module, *id, b)?;
            for class in 0..count {
                fee(b, path.len().saturating_add(1))?;
                out.push((path.clone(), class));
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (i, (_, child)) in fields.iter().enumerate() {
                fee(b, 1)?;
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(i as u32)));
                scan(module, child, path, depth + 1, matches!(ty, Type::Package { .. }), b, out)?;
                path.pop();
            }
        }
        Type::Tuple(fields) => {
            for (i, child) in fields.iter().enumerate() {
                fee(b, 1)?;
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(i as u32)));
                scan(module, child, path, depth + 1, false, b, out)?;
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            for (variant, (_, fields)) in variants.iter().enumerate() {
                fee(b, 1)?;
                for (index, child) in fields.iter().enumerate() {
                    fee(b, 1)?;
                    path.push(LifeTypeProjection::Place(PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        },
                    )));
                    scan(module, child, path, depth + 1, false, b, out)?;
                    path.pop();
                }
            }
        }
        Type::Array { element, .. } => {
            path.push(LifeTypeProjection::Element);
            scan(module, element, path, depth + 1, false, b, out)?;
            path.pop();
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn pure(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    pure_profile(ty, b, true)
}
/// A fixed shared array element is a thin address to closed finite storage.
/// Qualified pointees and fat Slice/dyn values do not enter this profile.
pub(crate) fn closed_copy(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    pure_profile(ty, b, false)
}
fn pure_profile(ty: &Type, b: &mut Budget, qualified: bool) -> CompileResult<bool> {
    fee(b, 1)?;
    let mut todo = vec![ty];
    while let Some(ty) = todo.pop() {
        fee(b, 1)?;
        match ty {
            Type::BorrowedStorage { storage, .. } if qualified => {
                fee(b, 1)?;
                todo.push(storage);
            }
            Type::Tuple(fields) => {
                fee(b, fields.len())?;
                todo.extend(fields);
            }
            Type::Record { fields, .. } => {
                fee(b, fields.len())?;
                todo.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Array { element, .. } => {
                fee(b, 1)?;
                todo.push(element);
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    fee(b, fields.len().saturating_add(1))?;
                    todo.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(b, 2)?;
                todo.push(error);
                todo.push(result);
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
            _ => return Ok(false),
        }
    }
    Ok(true)
}
