//! Concrete callback fields have no external Hold/Call dependencies. This
//! producer admission reads the actual capture graph and never an empty claim.
use super::*;

/// Qualification for inline frame memory, distinct from the Store element's
/// stronger self-contained theorem. The original/creator/header proof and
/// actual permissions remain mandatory for every externally rooted capture.
pub(super) fn environment(
    module: &TypedModule, id: ClosureId, budget: &mut Budget,
) -> CompileResult<bool> {
    budget.lookup(budget.semantic_closures_proven.len(), "complete inline callback environment")?;
    budget.spend(1, "actual inline callback identity")?;
    let Some(definition) = module.closures.get(id.0 as usize).filter(|d| d.id == id) else {
        return Ok(false);
    };
    if budget.semantic_closures_proven.contains(&id) {
        return Ok(crate::stored_callables::finite(module, id));
    }
    if definition.storage_life.as_ref().is_none_or(|life| life.fixed_binder_count != 0)
        || !crate::stored_callables::receiver_entry(module, definition)?
        || !crate::stored_callables::finite(module, id)
    {
        return Ok(false);
    }
    self_contained(module, id, budget)?;
    Ok(true)
}

pub(super) fn inline(
    module: &TypedModule, id: ClosureId, budget: &mut Budget,
) -> CompileResult<()> {
    if environment(module, id, budget)? { Ok(()) }
    else { self_contained(module, id, budget) }
}

pub(super) fn copy_value(
    module: &TypedModule, ty: &Type, budget: &mut Budget, depth: usize,
) -> CompileResult<bool> {
    budget.spend(1, "complete callback Copy node")?;
    if depth > budget.partial_depth {
        return Err(vec![Diagnostic::limit(Phase::Analyze, "callback Copy capture depth", None)]);
    }
    let Type::Closure(id) = ty else {
        budget.type_work(ty, true, "complete callback Copy leaf")?;
        return Ok(ty.is_copy_value());
    };
    if !environment(module, *id, budget)? { return Ok(false); }
    let definition = &module.closures[id.0 as usize];
    if !matches!(definition.kind, CallableKind::Fn | CallableKind::FnMut) { return Ok(false); }
    budget.spend(definition.captures.len(), "callback Copy capture directory")?;
    for capture in &definition.captures {
        if !copy_value(module, capture, budget, depth + 1)? { return Ok(false); }
    }
    Ok(true)
}

pub(super) fn self_contained(
    module: &TypedModule,
    root: ClosureId,
    budget: &mut Budget,
) -> CompileResult<()> {
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        budget.spend(1, "callback State environment")?;
        if !seen.insert(id) {
            continue;
        }
        let definition = module
            .closures
            .get(id.0 as usize)
            .ok_or_else(|| bad("callback State environment is absent"))?;
        budget.spend(definition.captures.len(), "callback State captures")?;
        for capture in &definition.captures {
            budget.type_work(capture, true, "callback State capture shape")?;
            match capture {
                Type::Closure(child) => pending.push(*child),
                Type::OwnedDyn { interface } => {
                    crate::dyn_cleanup::owner_interface(module, *interface, budget)?;
                }
                Type::Managed(_) if !dynamic_store::is_carrier(capture) => {}
                Type::Managed(_) if ownership_store::pod_owned(capture) => {
                    ownership_store::capture(module, capture, budget)?;
                }
                Type::Managed(_) if dynamic_store::is_owned_carrier(capture) => {
                    dynamic_store::owned_capture(module, capture, budget)?;
                }
                Type::Package { .. } if budget.ordinary_pod_store_type(capture)? => {
                    ownership_store::capture(module, capture, budget)?;
                }
                Type::Package { fields, .. }
                    if fields.iter().all(|(_, field)| {
                        field.is_copy_value()
                            || matches!(field,Type::Managed(_) if !dynamic_store::is_carrier(field))
                    }) => {}
                Type::Package { .. } if resources::product::nested(capture) => {
                    resources::product::admit(capture, budget)?;
                }
                Type::Package { .. } => {
                    dynamic_store::captured_package(module, capture, budget)?;
                }
                ty @ (Type::Tuple(_) | Type::Enum { .. } | Type::ErrorUnion { .. })
                    if budget.ordinary_store_sum_capture(id, ty)? =>
                {
                    if !resources::sums::admit(module, ty, budget)? {
                        return Err(bad(
                            "callback State owning Sum lacks an ordinary Store leaf",
                        ));
                    }
                }
                ty if ty.is_copy_value() => {}
                _ => {
                    return Err(vec![Diagnostic::unsupported(
                        Phase::Analyze,
                        "callback State requires a self-contained environment with supported owned captures",
                        None,
                    )]);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn check(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    for function in &module.functions {
        for ty in function
            .locals
            .iter()
            .map(|l| &l.ty)
            .chain(&function.values)
            .chain(std::iter::once(&function.return_type))
        {
            budget.spend(1, "callback State type occurrence")?;
            if let Type::Package { fields, .. } = ty {
                budget.spend(fields.len(), "callback State field table")?;
                for (_, field) in fields {
                    if let Type::Closure(id) = field {
                        inline(module, *id, budget)?;
                    }
                }
            }
        }
    }
    Ok(())
}
