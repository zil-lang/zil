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
        if bound.is_empty()
            || bound.windows(2).any(|w| w[0] >= w[1])
            || f.environment.is_some()
            || f.foreign.is_some()
            || f.callable_contract.is_some()
            || !matches!(&f.return_type,Type::Address{mutable:false,inner} if inner.is_address_pointee())
        {
            return Err(error(
                f,
                "life return bound requires sorted unique shared direct input positions",
            ));
        }
        for index in bound {
            let actual = f
                .parameters
                .get(*index as usize)
                .and_then(|local| f.locals.get(local.0 as usize));
            if !actual.is_some_and(|l|matches!(&l.ty,Type::Address{mutable:false,inner} if inner.is_address_pointee())) {
                return Err(error(f,"life return bound names a missing or non-shared parameter"));
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
            }
        }
    }
    Ok(())
}
