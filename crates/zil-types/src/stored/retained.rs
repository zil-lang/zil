//! Physical retained boundaries: pure values only; no target roots.
use super::*;

pub(super) fn expected(
    ty: &Type,
    limit: usize,
    span: Span,
    left: &mut usize,
) -> R<Vec<(Vec<LifeTypeProjection>, u32)>> {
    let mut out = Vec::new();
    scan(ty, &mut Vec::new(), 0, limit, span, left, &mut out)?;
    Ok(out)
}

fn scan(
    ty: &Type,
    path: &mut Vec<LifeTypeProjection>,
    depth: usize,
    limit: usize,
    span: Span,
    left: &mut usize,
    out: &mut Vec<(Vec<LifeTypeProjection>, u32)>,
) -> R<()> {
    fee(left, 1, span)?;
    if depth > limit {
        return Err(error(
            "E-STORED-IDENTITY",
            span,
            "physical retained type depth exceeded",
        ));
    }
    match ty {
        Type::BorrowedStorage { storage, life } => {
            if !matches!(storage.as_ref(), Type::Package { .. })
                && !shared_record_child(ty, left, span)?
                && !pure(ty, span, left)?
            {
                return Err(error(
                    "E-STORED-IDENTITY",
                    span,
                    "retained physical boundary has an address or ownership leaf",
                ));
            }
            shape(ty, left, limit.saturating_sub(depth + 1), span)?;
            fee(left, life.binder_count as usize, span)?;
            for class in 0..life.binder_count {
                fee(left, path.len().saturating_add(1), span)?;
                out.push((path.clone(), class));
            }
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (i, (_, child)) in fields.iter().enumerate() {
                fee(left, 1, span)?;
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(i as u32)));
                scan(child, path, depth + 1, limit, span, left, out)?;
                path.pop();
            }
        }
        Type::Tuple(fields) => {
            for (i, child) in fields.iter().enumerate() {
                fee(left, 1, span)?;
                path.push(LifeTypeProjection::Place(PlaceProjection::Field(i as u32)));
                scan(child, path, depth + 1, limit, span, left, out)?;
                path.pop();
            }
        }
        Type::Enum { variants, .. } => {
            for (variant, (_, fields)) in variants.iter().enumerate() {
                fee(left, 1, span)?;
                for (index, child) in fields.iter().enumerate() {
                    fee(left, 1, span)?;
                    path.push(LifeTypeProjection::Place(PlaceProjection::Variant(
                        zil_ir::VariantProjection {
                            variant: variant as u32,
                            index: index as u32,
                        },
                    )));
                    scan(child, path, depth + 1, limit, span, left, out)?;
                    path.pop();
                }
            }
        }
        Type::Array { element, .. } => {
            path.push(LifeTypeProjection::Element);
            scan(element, path, depth + 1, limit, span, left, out)?;
            path.pop();
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn pure(ty: &Type, span: Span, left: &mut usize) -> R<bool> {
    fee(left, 1, span)?;
    let mut todo = vec![ty];
    while let Some(ty) = todo.pop() {
        fee(left, 1, span)?;
        match ty {
            Type::BorrowedStorage { storage, .. } => {
                fee(left, 1, span)?;
                todo.push(storage);
            }
            Type::Tuple(fields) => {
                fee(left, fields.len(), span)?;
                todo.extend(fields);
            }
            Type::Record { fields, .. } => {
                fee(left, fields.len(), span)?;
                todo.extend(fields.iter().map(|(_, t)| t));
            }
            Type::Array { element, .. } => {
                fee(left, 1, span)?;
                todo.push(element);
            }
            Type::Enum { variants, .. } => {
                for (_, fields) in variants {
                    fee(left, fields.len().saturating_add(1), span)?;
                    todo.extend(fields);
                }
            }
            Type::ErrorUnion { error, result } => {
                fee(left, 2, span)?;
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
