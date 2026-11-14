//! Independent interpretation of original dynamic declarations and witnesses.
//! Metadata traversal supplies bounds only. This module never consumes Analyze
//! summaries, candidate-plan roots, or the producer's obligation solver.
use super::*;
mod compound;
pub(crate) mod constants;
mod constructors;
mod declarations;
mod definitions;
mod graph;
mod implicit;
mod instantiate;
mod keys;
mod matching;
mod queries;
mod scopes;
mod structural;
mod terms;
mod values;

fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-WITNESS", None, message)
}

/// Establish original scoped grammar and an acyclic declaration-level super
/// graph before actual application substitution. This is only one prerequisite
/// of the implementation proof; it does not authorize a table or method.
pub(super) fn declaration_graph(
    module: &TypedModule,
    budget: &mut Budget,
) -> Check<requests::Pending> {
    declarations::check(module, budget)?;
    definitions::check(module, budget)?;
    graph::claims(module, budget)
}

mod bridge;
mod callables;
mod context_ports;
mod contracts;
mod external;
mod headers;
mod inventory;
pub(super) mod requests;
mod signatures;
mod symbols;

pub(crate) mod semantic_external;
mod semantic_capabilities;

/// Complete actual Typed domain only. Pending sources remain pending; neither
/// the cardinality nor successful shape validation grants memory authority.
pub(crate) fn intrinsic_domain(
    module: &TypedModule,
    ty: &Type,
    budget: &mut Budget,
) -> Check<(u32, bool)> {
    let domain = values::intrinsic(module, ty, budget)?;
    Ok((domain.count, domain.pending_sources))
}

/// Reuse the independent full Typed inventory while the semantic context
/// separately validates every class expression and its original scope.
pub(crate) fn check_type_value(
    module: &TypedModule,
    value: &DynTypeValue,
    scope: DynInstantiationScope,
    budget: &mut Budget,
) -> Check<()> {
    values::check(module, value, scope, budget).map(|_| ())
}
