//! Independent finite numeric aggregate qualification. No analyzer predicate,
//! stored summary or claimed Copy flag admits a target-bearing value.
use super::*;

pub(super) fn operation(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::ExtractField { .. }
            | InstructionKind::ExtractIndex { .. }
            | InstructionKind::MakeVariant { .. }
            | InstructionKind::VariantTag { .. }
            | InstructionKind::ReadVariant { .. }
    )
}

fn ordinary(ty: &Type, b: &mut Budget, depth: usize) -> Check<()> {
    b.spend(1)?;
    if depth > b.limits.max_call_depth {
        return Err(limit("ordinary pure Copy type depth exceeded"));
    }
    match ty {
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
        Type::Record { fields, .. } => {
            for (_, field) in fields {
                ordinary(field, b, depth + 1)?;
            }
        }
        Type::Tuple(fields) => {
            for field in fields {
                ordinary(field, b, depth + 1)?;
            }
        }
        Type::Array { element, .. } => ordinary(element, b, depth + 1)?,
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                b.spend(1)?;
                for field in fields {
                    ordinary(field, b, depth + 1)?;
                }
            }
        }
        _ => {
            return Err(unsupported(
                "pure Copy operation contains an Address, borrowed carrier, capability or nonordinary payload",
                None,
            ));
        }
    }
    Ok(())
}

pub(super) fn types(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> Check<()> {
    let output = vt(
        f,
        i.result
            .ok_or_else(|| err("ordinary value operation omits its SSA result"))?,
    )?;
    ordinary(output, b, 0)?;
    let projected = match &i.kind {
        InstructionKind::MakeVariant { variant, fields } => {
            let Type::Enum { variants, .. } = output else {
                return Err(err("ordinary variant output has no enum layout"));
            };
            let payload = &variants
                .get(*variant as usize)
                .ok_or_else(|| err("ordinary variant tag is outside the actual nominal"))?
                .1;
            b.spend(payload.len().saturating_add(fields.len()))?;
            if payload.len() != fields.len() {
                return Err(err("ordinary variant omits actual payload fields"));
            }
            let limits = b.limits.clone();
            for (input, field) in fields.iter().zip(payload) {
                type_work(vt(f, *input)?, &limits, b)?;
                type_work(field, &limits, b)?;
                if vt(f, *input)? != field {
                    return Err(err(
                        "ordinary variant input changes the full actual payload type",
                    ));
                }
            }
            output
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            let aggregate = vt(f, *value)?;
            ordinary(aggregate, b, 0)?;
            let Type::Enum { variants, .. } = aggregate else {
                return Err(err("ordinary variant projection has no enum owner"));
            };
            variants
                .get(*variant as usize)
                .and_then(|(_, fields)| fields.get(*index as usize))
                .ok_or_else(|| err("ordinary variant projection selects no actual field"))?
        }
        InstructionKind::VariantTag { value } => {
            let aggregate = vt(f, *value)?;
            ordinary(aggregate, b, 0)?;
            if !matches!(aggregate, Type::Enum { .. }) {
                return Err(err("ordinary tag source is not an enum"));
            }
            &Type::I32
        }
        InstructionKind::ExtractField { aggregate, index } => {
            let aggregate = vt(f, *aggregate)?;
            ordinary(aggregate, b, 0)?;
            match aggregate {
                Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, field)| field),
                Type::Tuple(fields) => fields.get(*index as usize),
                _ => None,
            }
            .ok_or_else(|| err("ordinary field projection crosses its actual inline layout"))?
        }
        InstructionKind::ExtractIndex { aggregate, index } => {
            let aggregate = vt(f, *aggregate)?;
            ordinary(aggregate, b, 0)?;
            if !vt(f, *index)?.is_integer() {
                return Err(err("ordinary array index is not an integer"));
            }
            let Type::Array { element, .. } = aggregate else {
                return Err(err("ordinary index source is not a fixed array"));
            };
            element.as_ref()
        }
        _ => {
            return Err(err(
                "operation is outside ordinary pure Copy reconstruction",
            ));
        }
    };
    let limits = b.limits.clone();
    type_work(projected, &limits, b)?;
    type_work(output, &limits, b)?;
    if projected != output {
        return Err(err(
            "ordinary Copy result differs from its complete actual projection",
        ));
    }
    Ok(())
}
