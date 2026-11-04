//! Kernel-local structural admission and fresh positional substitution.
use super::*;

pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    life: &FunctionLifeContract,
    span: Option<Span>,
) -> Check<()> {
    let invalid = || {
        reject(
            "E-VERIFY-FUNCTION-LIFE",
            span,
            "malformed invocation-local pointer signature",
        )
    };
    if life.binder_count == 0 || life.parameter_lives.len() != parameters.len() {
        return Err(invalid());
    }
    let mut introduced = 0;
    for (index, parameter) in parameters.iter().enumerate() {
        let label = life.parameter_lives[index];
        if let Type::Address { inner, .. } = parameter {
            if !inner.is_address_pointee()
                || label.is_none_or(|label| label >= life.binder_count || label > introduced)
            {
                return Err(invalid());
            }
            if label == Some(introduced) {
                introduced += 1;
            }
        } else if label.is_some() || !parameter.is_copy_value() {
            return Err(invalid());
        }
    }
    if introduced != life.binder_count {
        return Err(invalid());
    }
    if let Type::Address { inner, mutable } = result {
        let Some(label) = life.result_life else {
            return Err(invalid());
        };
        if !inner.is_address_pointee() || label >= life.binder_count {
            return Err(invalid());
        }
        let mut grounded = false;
        for (index, parameter) in parameters.iter().enumerate() {
            grounded |= life.parameter_lives[index] == Some(label)
                && matches!(parameter, Type::Address { mutable: authority, .. } if !mutable || *authority);
        }
        if !grounded {
            return Err(invalid());
        }
    } else if life.result_life.is_some() || !result.is_copy_value() {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn inputs(ty: &Type, budget: &mut Budget) -> Check<Vec<usize>> {
    let Type::BorrowedFunctionPointer {
        parameters,
        result,
        life,
    } = ty
    else {
        return Ok(Vec::new());
    };
    budget.spend(parameters.len().saturating_mul(2).saturating_add(1))?;
    let mut inputs = Vec::new();
    if matches!(result.as_ref(), Type::Address { .. }) {
        for (index, parameter) in parameters.iter().enumerate() {
            if life.parameter_lives[index] == life.result_life
                && matches!(parameter, Type::Address { mutable, .. } if *mutable || !matches!(result.as_ref(), Type::Address { mutable: true, .. }))
            {
                inputs.push(index);
            }
        }
    }
    Ok(inputs)
}
