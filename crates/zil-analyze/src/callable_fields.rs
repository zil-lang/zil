//! Concrete callback fields have no external Hold/Call dependencies. This
//! producer admission reads the actual capture graph and never an empty claim.
use super::*;

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
        for (index, capture) in definition.captures.iter().enumerate() {
            budget.type_work(capture, true, "callback State capture shape")?;
            if let Type::Address {
                mutable: false,
                inner,
            } = capture
                && let Some(life) = &definition.storage_life
            {
                budget.spend(
                    life.static_bindings.len(),
                    "actual closed constant occurrence",
                )?;
                if life.fixed_binder_count == 0
                    && life.static_bindings.iter().any(|position| {
                        position.base == zil_ir::StorageBase::Capture(index as u32)
                            && position.path.is_empty()
                    })
                    && (inner.is_copy_value()
                        || matches!(inner.as_ref(), Type::Slice { element } if element.is_copy_value()))
                {
                    // A conditional lifetime invariant, never SyncDrop or
                    // Transfer. The mandatory whole-module source pass proves
                    // every actual constructor/write of this exact field.
                    crate::signature_life::charge(life, budget)?;
                    continue;
                }
            }
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
                        self_contained(module, *id, budget)?;
                    }
                }
            }
        }
    }
    Ok(())
}
