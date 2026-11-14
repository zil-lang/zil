//! The sole publication boundary for a complete Typed semantic program.
use crate::*;

/// Copies preserve the original program, including empty Some. Relocation is
/// necessary but never substitutes for whole-program recipe/W/partition proof.
pub(crate) fn program(
    cores: &[&CoreModule],
    module: &mut TypedModule,
    ledger: &super::emission::Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<Option<SemanticModule>> {
    if cores.iter().all(|core| core.semantic.is_none()) {
        if ledger.original.context().is_some() {
            return Err(error(
                "E-SEMANTIC-CONTEXT",
                Span::default(),
                "checked original Some input was erased before publication",
            ));
        }
        stored::ordinary_inventory(module, limits, left)?;
        return Ok(None);
    }
    let context = ledger.original.context().ok_or_else(|| {
        error(
            "E-SEMANTIC-CONTEXT",
            Span::default(),
            "semantic publication has no checked original context",
        )
    })?;
    context.verify_inputs(cores, ledger.frozen.as_ref(), limits, left)?;
    let mut result = context.publication_copy(limits, left)?;
    super::lower::program(&mut result, module, ledger, left)?;
    #[cfg(test)]
    {
        super::source_probe::record_stage(module, "after-lower")?;
        super::source_probe::record_semantic_stage(&result, "after-lower")?;
    }
    super::ghost_capture::populate(&mut result, module, ledger, limits, left)?;
    #[cfg(test)]
    {
        super::source_probe::record_stage(module, "after-ghost")?;
        super::source_probe::record_semantic_stage(&result, "after-ghost")?;
    }
    // Captured-head embeddings are derived from the actual entry's reads;
    // materializing a whole closure must not invent those persistent images.
    let headers = super::headers::program(&mut result, module, left)?;
    stored::ordinary_inventory(module, limits, left)?;
    super::nominal_catalog::program(&result, module, limits, left)?;
    super::creator_headers::populate(&mut result, module, ledger, limits, left)?;
    super::materialize::program(&mut result, module, ledger, limits, left)?;
    #[cfg(test)]
    super::source_probe::record(module, &result, limits, left)?;
    super::proof::program(&result, module, ledger, &headers, limits, left)?;
    context.verify_definitions(&result, limits, left)?;
    Ok(Some(result))
}
