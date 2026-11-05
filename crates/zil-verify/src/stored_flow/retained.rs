//! Physical retained boundaries: pure values only; no target roots.
use super::*;

pub(super) fn expected(
    ty: &Type,
    limits: &Limits,
    b: &mut Budget,
) -> Check<Vec<(Vec<LifeTypeProjection>, u32)>> {
    let mut out = Vec::new();
    scan(ty, &mut Vec::new(), 0, limits, b, &mut out)?;
    Ok(out)
}

fn scan(
    ty: &Type,
    path: &mut Vec<LifeTypeProjection>,
    depth: usize,
    limits: &Limits,
    b: &mut Budget,
    out: &mut Vec<(Vec<LifeTypeProjection>, u32)>,
) -> Check<()> {
    b.spend(1)?;
    if depth > limits.max_call_depth {
        return Err(err("physical retained type depth exceeded"));
    }
    match ty {
        Type::BorrowedStorage { life, .. } => {
            if !pure(ty, b)? {
                return Err(err(
                    "retained physical boundary has an address or ownership leaf",
                ));
            }
            physical(ty, limits, b)?;
            b.spend(life.binder_count as usize)?;
            for class in 0..life.binder_count {
                b.spend(path.len().saturating_add(1))?;
                out.push((path.clone(), class));
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (i, (_, child)) in fields.iter().enumerate() {
                b.spend(1)?;
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(i as u32)));
                scan(child, path, depth + 1, limits, b, out)?;
                path.pop();
            }
        }
        Type::Tuple(fields) => {
            for (i, child) in fields.iter().enumerate() {
                b.spend(1)?;
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(i as u32)));
                scan(child, path, depth + 1, limits, b, out)?;
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            for (variant, (_, fields)) in variants.iter().enumerate() {
                b.spend(1)?;
                for (index, child) in fields.iter().enumerate() {
                    b.spend(1)?;
                    path.push(LifeTypeProjection::Place(PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        },
                    )));
                    scan(child, path, depth + 1, limits, b, out)?;
                    path.pop();
                }
            }
        }
        Type::Array { element, .. } => {
            path.push(LifeTypeProjection::Element);
            scan(element, path, depth + 1, limits, b, out)?;
            path.pop();
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn pure(ty: &Type, b: &mut Budget) -> Check<bool> {
    b.spend(1)?;
    let mut todo = vec![ty];
    while let Some(ty) = todo.pop() {
        b.spend(1)?;
        match ty {
            Type::BorrowedStorage { storage, .. } => {
                b.spend(1)?;
                todo.push(storage);
            }
            Type::Tuple(fields) => {
                b.spend(fields.len())?;
                todo.extend(fields);
            }
            Type::Record { fields, .. } => {
                b.spend(fields.len())?;
                todo.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Array { element, .. } => {
                b.spend(1)?;
                todo.push(element);
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    b.spend(fields.len().saturating_add(1))?;
                    todo.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                b.spend(2)?;
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
