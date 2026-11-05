//! Ordinary finite values carry no stored targets. Admission checks the full
//! input/output types, including inactive enum variants, before empty transfer.
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

fn pure(ty: &Type, b: &mut Budget, depth: usize) -> CompileResult<bool> {
    fee(b, 1)?;
    if depth > b.partial_depth {
        return Err(vec![Diagnostic::limit(
            Phase::Analyze,
            "pure Copy type depth exceeded",
            None,
        )]);
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
        | Type::F64 => Ok(true),
        Type::Tuple(fields) => {
            for field in fields {
                if !pure(field, b, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Array { element, .. } => pure(element, b, depth + 1),
        Type::Record { fields, .. } => {
            for (_, field) in fields {
                if !pure(field, b, depth + 1)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                fee(b, 1)?;
                for field in fields {
                    if !pure(field, b, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        Type::BorrowedStorage { .. } => {
            physical(ty, b)?;
            retained::pure(ty, b)
        }
        _ => Ok(false),
    }
}

pub(super) fn types(f: &TypedFunction, i: &Instruction, b: &mut Budget) -> CompileResult<()> {
    let result = value_type(
        f,
        i.result
            .ok_or_else(|| failure("pure Copy operation has no result"))?,
    )?;
    if !pure(result, b, 0)? {
        return Err(vec![Diagnostic::unsupported(
            Phase::Analyze,
            "pure Copy operation result contains an address, capability or nonordinary payload",
            Some(i.span),
        )]);
    }
    let expected = match &i.kind {
        InstructionKind::ExtractField { aggregate, index } => {
            let source = value_type(f, *aggregate)?;
            if !pure(source, b, 0)? {
                return Err(failure(
                    "pure Copy field source contains a target-bearing payload",
                ));
            }
            match source {
                Type::Tuple(fields) => fields.get(*index as usize),
                Type::Record { fields, .. } => fields.get(*index as usize).map(|(_, ty)| ty),
                _ => None,
            }
            .ok_or_else(|| failure("pure Copy field has no actual inline layout"))?
        }
        InstructionKind::ExtractIndex { aggregate, index } => {
            let source = value_type(f, *aggregate)?;
            if !pure(source, b, 0)? || !value_type(f, *index)?.is_integer() {
                return Err(failure(
                    "pure Copy array source or integer index is invalid",
                ));
            }
            let Type::Array { element, .. } = source else {
                return Err(failure("pure Copy index requires a fixed array"));
            };
            element.as_ref()
        }
        InstructionKind::MakeVariant { variant, fields } => {
            let Type::Enum { variants, .. } =
                super::carrier(result).map_or(result, |(storage, _)| storage)
            else {
                return Err(failure("pure Copy variant result is not an enum"));
            };
            let (_, payload) = variants
                .get(*variant as usize)
                .ok_or_else(|| failure("pure Copy variant is outside actual enum"))?;
            fee(b, payload.len().saturating_add(fields.len()))?;
            if payload.len() != fields.len() {
                return Err(failure("pure Copy variant payload count differs"));
            }
            for (field, expected) in fields.iter().zip(payload) {
                b.type_work(expected, false, "pure Copy variant complete field equality")?;
                b.type_work(
                    value_type(f, *field)?,
                    false,
                    "pure Copy variant actual input",
                )?;
                if value_type(f, *field)? != expected {
                    return Err(failure("pure Copy variant changes a complete field type"));
                }
            }
            result
        }
        InstructionKind::VariantTag { value } => {
            let source = value_type(f, *value)?;
            if !matches!(
                super::carrier(source).map_or(source, |(storage, _)| storage),
                Type::Enum { .. }
            ) || !pure(source, b, 0)?
            {
                return Err(failure(
                    "pure Copy tag source is not a complete ordinary enum",
                ));
            }
            &Type::I32
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            let source = value_type(f, *value)?;
            if !pure(source, b, 0)? {
                return Err(failure("pure Copy variant source contains stored targets"));
            }
            let Type::Enum { variants, .. } =
                super::carrier(source).map_or(source, |(storage, _)| storage)
            else {
                return Err(failure("pure Copy projection source is not an enum"));
            };
            variants
                .get(*variant as usize)
                .and_then(|(_, fields)| fields.get(*index as usize))
                .ok_or_else(|| failure("pure Copy variant field is outside actual layout"))?
        }
        _ => return Err(failure("unknown pure Copy operation")),
    };
    b.type_work(expected, false, "pure Copy exact projected type")?;
    b.type_work(result, false, "pure Copy exact result type")?;
    if expected != result {
        return Err(failure(
            "pure Copy operation changes its complete actual type",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g4454_inactive_and_nested_payloads_never_hide_address_roots() {
        let limits = Limits::default();
        let address = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        let inactive = Type::Enum {
            name: "InactiveAddress".into(),
            variants: vec![
                ("Empty".into(), vec![]),
                ("Hidden".into(), vec![address.clone()]),
            ],
        };
        let nested = Type::Record {
            name: "NestedAddress".into(),
            fields: vec![(
                "payload".into(),
                Type::Tuple(vec![Type::Array {
                    element: Box::new(address),
                    length: 2,
                }]),
            )],
        };
        for ty in [inactive, nested, Type::ForeignHandle] {
            assert!(!pure(&ty, &mut Budget::new(&limits), 0).unwrap());
        }
    }
}
