//! Producer-side checking of declared return upper bounds, after real origins.
use super::*;
fn error(f: &TypedFunction, text: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::error(
        Phase::Analyze,
        "E-LIFE-RETURN",
        Some(f.span),
        text,
    )]
}
pub(super) fn headers(module: &TypedModule, budget: &mut Budget) -> CompileResult<()> {
    for f in &module.functions {
        let Some(bound) = &f.reference_return_bound else {
            continue;
        };
        budget.spend(
            bound.len().saturating_mul(2).saturating_add(1),
            "life return declaration",
        )?;
        budget.type_work(&f.return_type, true, "life return address qualification")?;
        if bound.is_empty()
            || bound.windows(2).any(|w| w[0] >= w[1])
            || f.environment.is_some()
            || f.foreign.is_some()
            || f.callable_contract.is_some()
            || !matches!(&f.return_type,Type::Address{inner,..} if inner.is_address_pointee())
        {
            return Err(error(
                f,
                "life return bound requires sorted unique address direct input positions",
            ));
        }
        for index in bound {
            let actual = f
                .parameters
                .get(*index as usize)
                .and_then(|local| f.locals.get(local.0 as usize));
            if let Some(actual) = actual {
                budget.type_work(&actual.ty, true, "life input address qualification")?;
            }
            if !actual.is_some_and(
                |l| matches!(&l.ty,Type::Address{inner,..} if inner.is_address_pointee()),
            ) {
                return Err(error(
                    f,
                    "life return bound names a missing or non-address parameter",
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn returns(
    module: &TypedModule,
    graphs: &[(Vec<Vec<usize>>, Vec<bool>)],
    facts: &[Vec<ValueHold>],
    budget: &mut Budget,
) -> CompileResult<()> {
    for ((f, (_, reachable)), values) in module.functions.iter().zip(graphs).zip(facts) {
        let Some(bound) = &f.reference_return_bound else {
            continue;
        };
        budget.spend(f.blocks.len(), "life return control edges")?;
        for (block, live) in f.blocks.iter().zip(reachable) {
            if *live && let Terminator::Return(Some(value)) = block.terminator {
                let row = values
                    .get(value.0 as usize)
                    .ok_or_else(|| error(f, "life return value is absent"))?;
                budget.spend(
                    row.roots
                        .len()
                        .saturating_mul(bound.len().saturating_add(1)),
                    "life return source inclusion",
                )?;
                if row.roots.is_empty()
                    || row.roots.iter().any(
                        |r| !matches!(r,HoldRoot::Parameter(i) if bound.binary_search(i).is_ok()),
                    )
                {
                    return Err(error(
                        f,
                        "actual return origins exceed the declared life input set",
                    ));
                }
                if matches!(f.return_type, Type::Address { mutable: true, .. }) {
                    // A declared upper bound may include shared parameters.
                    // Only actual exclusive input authority can ground this
                    // mutable result; the label itself grants no permission.
                    budget.spend(row.roots.len(), "mutable life actual input permissions")?;
                    if row.roots.iter().any(|root| {
                        let HoldRoot::Parameter(index) = root else {
                            return true;
                        };
                        !f.parameters
                            .get(*index as usize)
                            .and_then(|local| f.locals.get(local.0 as usize))
                            .is_some_and(|local| {
                                matches!(local.ty, Type::Address { mutable: true, .. })
                            })
                    }) {
                        return Err(error(
                            f,
                            "mutable life result requires exclusive actual input authority",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
