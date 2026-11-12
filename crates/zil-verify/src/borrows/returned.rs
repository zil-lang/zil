//! Instantiate only independently checked callable source relations. A relation
//! selects an actual argument's existing roots; it never constructs a root.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
type Slots = BTreeMap<u32, BTreeSet<captures::Source>>;
fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-CALLABLE-SOURCE", None, message)
}
pub(super) fn inputs(
    module: &TypedModule,
    function: FunctionId,
    budget: &mut Budget,
) -> Check<BTreeSet<u32>> {
    let mut out = BTreeSet::new();
    let Some(claim) = &module.functions[function.0 as usize].callable_return_contract else {
        return Ok(out);
    };
    let sources = claim
        .capture_sources
        .as_ref()
        .ok_or_else(|| bad("returned environment sources are unknown"))?;
    budget.spend(sources.len())?;
    for binding in sources {
        budget.spend(binding.sources.len())?;
        for source in &binding.sources {
            let StorageBase::Parameter(index) = source.base else {
                return Err(bad("returned environment source is not an input"));
            };
            out.insert(index);
        }
    }
    Ok(out)
}
pub(super) fn instantiate(
    module: &TypedModule,
    f: &TypedFunction,
    function: FunctionId,
    arguments: &[ValueId],
    ordinary: &[Roots],
    budget: &mut Budget,
) -> Check<Slots> {
    let callee = &module.functions[function.0 as usize];
    let Some(claim) = &callee.callable_return_contract else {
        return Ok(Slots::new());
    };
    let Some(bindings) = &claim.capture_sources else {
        return Err(bad("returned environment sources are unknown"));
    };
    let mut slots = Slots::new();
    budget.spend(bindings.len())?;
    for binding in bindings {
        let StorageBase::Capture(capture) = binding.position.base else {
            return Err(bad("returned environment target is not a capture"));
        };
        if !binding.position.path.is_empty() {
            return Err(unsupported(
                "nested returned environment slots require actual nested slot transport",
                Some(f.span),
            ));
        }
        let capture_type = &module.closures[claim.closure.0 as usize].captures[capture as usize];
        let mut roots = Roots::new();
        for source in &binding.sources {
            budget.spend(source.path.len().saturating_add(1))?;
            let StorageBase::Parameter(parameter) = source.base else {
                return Err(bad("returned environment source is not an input"));
            };
            let argument = *arguments
                .get(parameter as usize)
                .ok_or_else(|| bad("returned environment actual argument is absent"))?;
            let actual = &ordinary[argument.0 as usize];
            if source.path.is_empty() {
                roots_join(&mut roots, actual, budget)?;
                continue;
            }
            let projections = source
                .path
                .iter()
                .map(|p| match p {
                    ContractProjection::Place(p) => Ok(*p),
                    _ => Err(unsupported(
                        "returned environment source needs actual nested/retained slot transport",
                        Some(f.span),
                    )),
                })
                .collect::<Check<Vec<_>>>()?;
            let (endpoint, _) =
                crate::retained::sources::source_type(module, callee, source, budget)?;
            // A stored pointer's target is not its physical pointer slot.
            if endpoint == capture_type {
                return Err(unsupported(
                    "returned stored-borrow leaves require independently reconstructed target transport",
                    Some(f.span),
                ));
            }
            let Type::Address { .. } = f.values[argument.0 as usize] else {
                return Err(unsupported(
                    "returned field source is not an actual address projection",
                    Some(f.span),
                ));
            };
            let mut normalized = Roots::new();
            for root in actual {
                let root = match *root {
                    Root::Field(local, index) => roots::mixed_root(
                        f,
                        local,
                        &[PlaceProjection::Field(index)],
                        false,
                        budget,
                    )?,
                    other => other,
                };
                roots_join(&mut normalized, &[root], budget)?;
            }
            let selected =
                roots::mixed_address_roots(f, argument, &projections, false, &normalized, budget)?;
            roots_join(&mut roots, &selected, budget)?;
        }
        budget.spend(roots.len())?;
        slots
            .entry(capture)
            .or_insert_with(BTreeSet::new)
            .extend(roots.into_iter().map(captures::Source::Root));
    }
    Ok(slots)
}
pub(super) fn roots(
    module: &TypedModule,
    f: &TypedFunction,
    function: FunctionId,
    arguments: &[ValueId],
    ordinary: &[Roots],
    budget: &mut Budget,
) -> Check<Roots> {
    let mut out = Roots::new();
    for sources in instantiate(module, f, function, arguments, ordinary, budget)?.values() {
        for source in sources {
            roots_join(&mut out, &[source.position()], budget)?;
        }
    }
    Ok(out)
}
