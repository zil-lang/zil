//! Invocation-local relations for thin ordinary code pointers. These checks
//! create untrusted IR; neither source annotations nor this module prove roots.
use super::*;

pub(super) fn from_life(life: &CoreLifeContract) -> FunctionLifeContract {
    let mut result = FunctionLifeContract {
        binder_count: life.binders.len() as u32,
        parameter_lives: life.parameter_lives.clone(),
        result_life: life.result_life,
    };
    let mut remap = vec![None; life.binders.len()];
    let mut next = 0;
    for label in result
        .parameter_lives
        .iter_mut()
        .chain(std::iter::once(&mut result.result_life))
    {
        if let Some(old) = *label {
            let Some(entry) = remap.get_mut(old as usize) else {
                return FunctionLifeContract {
                    binder_count: life.binders.len() as u32,
                    parameter_lives: life.parameter_lives.clone(),
                    result_life: life.result_life,
                };
            };
            let dense = *entry.get_or_insert_with(|| {
                let dense = next;
                next += 1;
                dense
            });
            *label = Some(dense);
        }
    }
    result.binder_count = next;
    result
}

pub(super) fn default_life(parameters: &[Type], result: &Type) -> FunctionLifeContract {
    FunctionLifeContract {
        binder_count: 1,
        parameter_lives: parameters
            .iter()
            .map(|ty| matches!(ty, Type::Address { .. }).then_some(0))
            .collect(),
        result_life: matches!(result, Type::Address { .. }).then_some(0),
    }
}

pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    life: &FunctionLifeContract,
    span: Span,
) -> R<()> {
    let invalid = || {
        error(
            "E-FUNCTION-LIFE",
            span,
            "borrowed function pointer requires a complete invocation-local address relation",
        )
    };
    if life.binder_count == 0 || life.parameter_lives.len() != parameters.len() {
        return Err(invalid());
    }
    let mut next = 0;
    for (parameter, tag) in parameters.iter().zip(&life.parameter_lives) {
        match parameter {
            Type::Address { inner, .. } if inner.is_address_pointee() => {
                let Some(tag) = tag.filter(|tag| *tag < life.binder_count && *tag <= next) else {
                    return Err(invalid());
                };
                if tag == next {
                    next += 1;
                }
            }
            ty if ty.is_copy_value() && tag.is_none() => {}
            _ => return Err(invalid()),
        }
    }
    if next != life.binder_count {
        return Err(invalid());
    }
    match result {
        Type::Address { mutable, inner } if inner.is_address_pointee() => {
            let Some(result) = life.result_life.filter(|tag| *tag < life.binder_count) else {
                return Err(invalid());
            };
            if !parameters.iter().zip(&life.parameter_lives).any(|(parameter,tag)|
                *tag == Some(result) && matches!(parameter, Type::Address { mutable: input_mutable, .. } if !mutable || *input_mutable)) {
                return Err(invalid());
            }
        }
        ty if ty.is_copy_value() && life.result_life.is_none() => {}
        _ => return Err(invalid()),
    }
    Ok(())
}

pub(super) fn metadata_work(life: &FunctionLifeContract) -> usize {
    (life.binder_count as usize)
        .saturating_add(life.parameter_lives.len().saturating_mul(2))
        .saturating_add(2)
}

pub(super) fn function_reference(
    signature: &Signature,
    expected: Option<&Type>,
    span: Span,
) -> R<Option<Type>> {
    let Some(Type::BorrowedFunctionPointer {
        parameters,
        result,
        life,
    }) = expected
    else {
        return Ok(None);
    };
    relation(parameters, result, life, span)?;
    if signature.id.is_none()
        || signature.foreign
        || signature.cleanup
        || signature.type_parameter.is_some()
        || !signature.callable.is_empty()
        || signature.parameters != *parameters
        || signature.result != **result
    {
        return Err(error(
            "E-FUNCTION-LIFE",
            span,
            "borrowed function pointer target has a different complete signature or requires an environment",
        ));
    }
    Ok(Some(expected.expect("borrowed pointer context").clone()))
}

/// A concrete call returns only invocation input storage in this profile.
/// This classifier triggers a full actual-return proof; it grants no origins.
pub(super) fn invocation(parameters: &[Type], result: &Type) -> bool {
    matches!(result,Type::Address{inner,..} if inner.is_address_pointee())
        && parameters.iter().any(|p| matches!(p, Type::Address { .. }))
}

pub(super) fn invocation_call<'a>(
    module: &'a TypedModule,
    f: &TypedFunction,
    kind: &'a InstructionKind,
) -> Option<(&'a ClosureDefinition, &'a [ValueId])> {
    let (ty, args) = match kind {
        InstructionKind::CallClosure { callee, arguments }
        | InstructionKind::CallClosureOnce { callee, arguments }
        | InstructionKind::CallClosureBorrowed {
            callee, arguments, ..
        } => (f.values.get(callee.0 as usize)?, arguments),
        InstructionKind::CallClosureShared { callee, arguments }
        | InstructionKind::CallClosureMut { callee, arguments } => {
            (&f.locals.get(callee.0 as usize)?.ty, arguments)
        }
        _ => return None,
    };
    let ty = if let Type::Address { inner, .. } = ty {
        inner.as_ref()
    } else {
        ty
    };
    let Type::Closure(id) = ty else { return None };
    let d = module.closures.get(id.0 as usize)?;
    (d.lending_captures.is_empty() && invocation(&d.parameters, &d.result)).then_some((d, args))
}
