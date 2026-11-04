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

/// A concrete call returns only invocation input storage in this profile.
/// This classifier triggers a full actual-return proof; it grants no origins.
pub(super) fn invocation(parameters: &[Type], result: &Type) -> bool {
    matches!(result, Type::Address { .. })
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
    (d.lending_captures.is_empty() && matches!(d.result, Type::Address { .. })).then_some((d, args))
}

/// This is kernel-local work accounting, independent of producer checks.
pub(super) fn signature_work(parameters: &[Type], result: &Type, budget: &mut Budget) -> Check<()> {
    for ty in parameters.iter().chain(std::iter::once(result)) {
        // Reserve this walk, recursive eligibility, equality and a possible
        // repeated structural comparison before any of those operations.
        resources::charge_payload(ty, budget)?;
        resources::charge_payload(ty, budget)?;
    }
    Ok(())
}
