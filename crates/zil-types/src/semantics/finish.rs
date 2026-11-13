//! The sole publication boundary for a complete Typed semantic program.
use crate::*;

/// Copies preserve the original program, including empty Some. Relocation is
/// necessary but never substitutes for whole-program recipe/W/partition proof.
pub(crate) fn program(
    cores: &[&CoreModule],
    module: &TypedModule,
    ledger: &super::emission::Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<SemanticModule>> {
    if cores.iter().all(|core| core.semantic.is_none()) {
        return Ok(None);
    }
    let mut result = SemanticModule {
        definitions: Vec::new(),
        instances: Vec::new(),
        sites: Vec::new(),
        dyn_scopes: Vec::new(),
    };
    for core in cores {
        let Some(program) = &core.semantic else {
            continue;
        };
        crate::dyn_metadata::semantic_snapshot(program, limits, left, 1)?;
        // Reserve the vector moves separately from the recursive snapshot.
        let rows = program
            .definitions
            .len()
            .saturating_add(program.instances.len())
            .saturating_add(program.sites.len())
            .saturating_add(program.dyn_scopes.len());
        *left = left
            .checked_sub(rows.saturating_mul(2).saturating_add(1))
            .ok_or_else(|| {
                Diagnostic::limit(Phase::Types, "semantic program merge work exceeded", None)
            })?;
        let copy = program.clone();
        result.definitions.extend(copy.definitions);
        result.instances.extend(copy.instances);
        result.sites.extend(copy.sites);
        result.dyn_scopes.extend(copy.dyn_scopes);
    }
    super::lower::program(&mut result, module, ledger, left)?;
    // Until every holding point is materialized and original class equations,
    // where premises, complete rigid/partition bodies and actual headers have
    // been independently checked, no Some program leaves this boundary.
    Err(Diagnostic::unsupported(
        Phase::Types,
        "complete semantic type materialization, original where and rigid/partition proofs are not implemented",
        None,
    ))
}
