//! Producer-side prefix resource admission; Verify rebuilds it independently.
use zil_ir::{InstructionKind, Type, ValueId};

use super::{Budget, CompileResult, Diagnostic, Phase, TypedModule, callable_fields};

/// The structural preflight records only Store elements that actually contain
/// a concrete Closure. Ordinary Store inputs therefore keep their old work
/// bound. This pass reads the raw element and environment tables; no Types
/// classification or public candidate claim grants admission.
pub(super) fn callback_element(
    module: &TypedModule,
    element: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    const CONTEXT: &str = "callback Store element admission";
    let unsupported = || {
        vec![Diagnostic::unsupported(
            Phase::Analyze,
            "callback Store requires a complete State with one or two direct self-contained callbacks, finite Copy fields and direct Managed resources",
            None,
        )]
    };
    // Preflight has bounded every type and capture tree before this recursive
    // reservation. Reserve both the field scan and recursive Copy predicates.
    budget.type_work(element, true, CONTEXT)?;
    let Type::Package { fields, .. } = element else {
        return Err(unsupported());
    };
    budget.spend(fields.len().saturating_add(1), CONTEXT)?;
    let (mut callback, mut second, mut count, mut managed) = (None, None, 0usize, false);
    for (_, field) in fields {
        match field {
            Type::Closure(id) => {
                if count == 2 {
                    return Err(unsupported());
                }
                match callback {
                    None => callback = Some(*id),
                    Some(first) if first != *id => second = Some(*id),
                    _ => {}
                }
                count += 1;
            }
            Type::Managed(inner)
                if **inner == Type::ByteBuffer
                    || (matches!(inner.as_ref(), Type::Record { .. }) && inner.is_copy_value()) =>
            {
                managed = true;
            }
            other if other.is_copy_value() => {}
            _ => return Err(unsupported()),
        }
    }
    let Some(callback) = callback.filter(|_| managed) else {
        return Err(unsupported());
    };
    // Reserve the initial pending entry before the existing independent
    // capture-graph checker allocates it. Each actual capture is then charged
    // there before inspecting or enqueueing its type.
    if let Some(other) = second {
        // Only the new heterogeneous profile adds result constraints. The
        // existing same-environment lending path keeps its original work.
        for id in [callback, other] {
            budget.spend(1, "heterogeneous Store callback result")?;
            let definition = module
                .closures
                .get(id.0 as usize)
                .ok_or_else(|| super::bad("callback Store environment is absent"))?;
            budget.type_work(
                &definition.result,
                true,
                "heterogeneous Store result Copy predicate",
            )?;
            if !definition.result.is_copy_value() {
                return Err(vec![Diagnostic::unsupported(
                    Phase::Analyze,
                    "heterogeneous callback Store requires Copy results for both actual environments",
                    None,
                )]);
            }
        }
    }
    budget.spend(1, "callback Store environment root")?;
    callable_fields::self_contained(module, callback, budget)?;
    if let Some(other) = second {
        budget.spend(1, "callback Store second environment root")?;
        callable_fields::self_contained(module, other, budget)?;
    }
    Ok(())
}

pub(super) fn is_carrier(ty: &Type) -> bool {
    matches!(ty, Type::Managed(inner) | Type::Desc(inner) if matches!(inner.as_ref(), Type::OwnedStore { .. }))
}

pub(super) fn access(kind: &InstructionKind) -> Option<(ValueId, bool)> {
    match *kind {
        InstructionKind::OwnedStoreReserve { store, .. }
        | InstructionKind::OwnedStorePush { store, .. }
        | InstructionKind::OwnedStorePop { store }
        | InstructionKind::OwnedStoreBorrowMut { store, .. } => Some((store, true)),
        InstructionKind::OwnedStoreLen { store }
        | InstructionKind::OwnedStoreCapacity { store }
        | InstructionKind::OwnedStoreBorrow { store, .. } => Some((store, false)),
        _ => None,
    }
}
