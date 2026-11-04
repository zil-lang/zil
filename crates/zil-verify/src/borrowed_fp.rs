//! Kernel-local structural admission and fresh positional substitution.
use super::*;

pub(super) fn relation(
    parameters: &[Type],
    result: &Type,
    life: &StorageLifeContract,
    span: Option<Span>,
) -> Check<()> {
    if life.fixed_binder_count != 0 {
        return Err(unsupported(
            "borrowed pointer or role Fixed classes require genuine enclosing semantic context",
            span,
        ));
    }
    super::signature_life::relation(parameters, result, life, span)
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
    budget.spend(
        parameters
            .len()
            .saturating_mul(life.bindings.len().saturating_add(2))
            .saturating_add(1),
    )?;
    let mut inputs = Vec::new();
    if matches!(result.as_ref(), Type::Address { .. }) {
        for (index, parameter) in parameters.iter().enumerate() {
            if super::signature_life::outer(life, StorageBase::Parameter(index as u32))
                == super::signature_life::outer(life, StorageBase::Result)
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
