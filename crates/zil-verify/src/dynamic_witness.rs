//! Independent interpretation of original dynamic declarations and witnesses.
//! Metadata traversal supplies bounds only. This module never consumes Analyze
//! summaries, candidate-plan roots, or the producer's obligation solver.
use super::*;
mod declarations;
mod definitions;
mod constants;
mod scopes;
mod implicit;

fn bad(message: &str) -> Diagnostic {
    reject("E-VERIFY-DYN-WITNESS", None, message)
}

/// Establish original scoped grammar and an acyclic declaration-level super
/// graph before actual application substitution. This is only one prerequisite
/// of the implementation proof; it does not authorize a table or method.
pub(super) fn declaration_graph(module: &TypedModule, budget: &mut Budget) -> Check<()> {
    declarations::check(module, budget)?;
    definitions::check(module, budget)
}
