//! Ordinary POD Store ownership qualification. This is producer-side admission;
//! the resource and Hold passes reconstruct actual transfer and live roots.
use zil_ir::{Type, TypedModule};

use super::{Budget, CompileResult, Diagnostic, Phase, dynamic_store, pod_store};

fn unsupported(message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::unsupported(Phase::Analyze, message, None)]
}

pub(super) fn pod_owned(ty: &Type) -> bool {
    matches!(ty, Type::Managed(payload)
        if matches!(payload.as_ref(), Type::OwnedStore { element }
            if !matches!(element.as_ref(), Type::Package { .. })))
}

/// A complete owning Store leaf in an already admitted Product or Sum. The
/// actual element and sealed resource are checked here, including unused types.
pub(super) fn ordinary_owned(
    module: &TypedModule,
    ty: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    let Type::Managed(payload) = ty else {
        return Err(unsupported(
            "ordinary Store requires an owning Managed carrier",
        ));
    };
    payload_definition(module, payload, budget)
}

/// Pure canonical payload qualification for both owning and weak signatures.
/// This creates no Managed value, input head, loan or cleanup responsibility.
pub(super) fn payload_definition(
    module: &TypedModule,
    payload: &Type,
    budget: &mut Budget,
) -> CompileResult<()> {
    let Type::OwnedStore { element } = payload else {
        return Err(unsupported(
            "ordinary Store requires an actual Store payload",
        ));
    };
    if matches!(element.as_ref(), Type::Package { .. }) {
        return dynamic_store::ordinary_payload(module, payload, budget);
    }
    let owner = module as *const TypedModule as usize;
    let key = if budget.geometry_module == Some(owner) && !super::nested_store::selected(element) {
        let element_id = super::semantic::geometry::type_identity(element, budget)?;
        let key = (owner, budget.partial_depth, budget.helper_nodes, element_id);
        budget.lookup(budget.formed_pod_payloads.len(), "complete POD Store payload lookup")?;
        if budget.formed_pod_payloads.contains(&key) { return Ok(()); }
        Some(key)
    } else { None };
    budget.type_work(payload, true, "ordinary POD Store complete payload")?;
    pod_store::carrier(module, element, budget)?;
    if let Some(key) = key {
        budget.lookup(budget.formed_pod_payloads.len(), "complete POD Store payload publication")?;
        budget.formed_pod_payloads.insert(key);
    }
    Ok(())
}

/// Concrete environments may own the same Store as an ordinary value. The
/// enclosing capture graph, MakeClosure operands and one-shot use are checked
/// independently by the existing closure and resource passes.
pub(super) fn capture(module: &TypedModule, ty: &Type, budget: &mut Budget) -> CompileResult<()> {
    match ty {
        Type::Managed(payload) if matches!(payload.as_ref(), Type::OwnedStore { .. }) => {
            ordinary_owned(module, ty, budget)
        }
        Type::Package { .. } => {
            budget.type_work(ty, true, "ordinary POD Store capture Product")?;
            super::resources::product::admit_owned(module, ty, budget)?;
            Ok(())
        }
        _ => Err(unsupported(
            "ordinary POD Store capture requires an owning value",
        )),
    }
}
