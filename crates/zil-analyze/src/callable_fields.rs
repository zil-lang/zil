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
        for capture in &definition.captures {
            budget.type_work(capture, true, "callback State capture shape")?;
            match capture {
                Type::Closure(child) => pending.push(*child),
                Type::Managed(_) if !dynamic_store::is_carrier(capture) => {}
                Type::Package { fields, .. }
                    if fields.iter().all(|(_, field)| {
                        field.is_copy_value()
                            || matches!(field,Type::Managed(_) if !dynamic_store::is_carrier(field))
                    }) => {}
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
