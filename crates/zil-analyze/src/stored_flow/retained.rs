//! Physical retained boundaries: pure values only; no target roots.
use super::*;

pub(super) fn expected(
    ty: &Type,
    b: &mut Budget,
) -> CompileResult<Vec<(Vec<LifeTypeProjection>, u32)>> {
    let mut out = Vec::new();
    scan(ty, &mut Vec::new(), 0, b, &mut out)?;
    Ok(out)
}

fn scan(
    ty: &Type,
    path: &mut Vec<LifeTypeProjection>,
    depth: usize,
    b: &mut Budget,
    out: &mut Vec<(Vec<LifeTypeProjection>, u32)>,
) -> CompileResult<()> {
    fee(b, 1)?;
    if depth > b.partial_depth {
        return Err(failure("physical retained type depth exceeded"));
    }
    match ty {
        Type::BorrowedStorage { life, .. } => {
            if !pure(ty, b)? {
                return Err(failure(
                    "retained physical boundary has an address or ownership leaf",
                ));
            }
            physical(ty, b)?;
            fee(b, life.binder_count as usize)?;
            for class in 0..life.binder_count {
                fee(b, path.len().saturating_add(1))?;
                out.push((path.clone(), class));
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (i, (_, child)) in fields.iter().enumerate() {
                fee(b, 1)?;
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(i as u32)));
                scan(child, path, depth + 1, b, out)?;
                path.pop();
            }
        }
        Type::Tuple(fields) => {
            for (i, child) in fields.iter().enumerate() {
                fee(b, 1)?;
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(i as u32)));
                scan(child, path, depth + 1, b, out)?;
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
                    scan(child, path, depth + 1, b, out)?;
                    path.pop();
                }
            }
        }
        Type::Array { element, .. } => {
            path.push(LifeTypeProjection::Element);
            scan(element, path, depth + 1, b, out)?;
            path.pop();
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn pure(ty: &Type, b: &mut Budget) -> CompileResult<bool> {
    fee(b, 1)?;
    let mut todo = vec![ty];
    while let Some(ty) = todo.pop() {
        fee(b, 1)?;
        match ty {
            Type::BorrowedStorage { storage, .. } => {
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
            | Type::I64
            | Type::U64
            | Type::Usize
            | Type::U8
            | Type::U16
            | Type::F32
            | Type::F64 => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}
