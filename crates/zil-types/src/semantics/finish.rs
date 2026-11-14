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
        stored::OrdinaryStats::step(
            &ledger.ordinary_stats,
            "finish: ordinary inventory",
            left,
            |left| stored::ordinary_inventory(module, limits, left),
        )?;
        return Ok(None);
    }
    let context = ledger.original.context().ok_or_else(|| {
        error(
            "E-SEMANTIC-CONTEXT",
            Span::default(),
            "semantic publication has no checked original context",
        )
    })?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: verify original inputs",
        left,
        |left| context.verify_inputs(cores, ledger.frozen.as_ref(), limits, left),
    )?;
    let mut result = stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: original publication snapshot",
        left,
        |left| context.publication_copy(limits, left),
    )?;
    stored::OrdinaryStats::step(&ledger.ordinary_stats, "finish: relocation", left, |left| {
        super::lower::program(&mut result, module, ledger, limits, left)
    })?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: actual Dyn registry relocation",
        left,
        |left| super::materialize::dynamic::relocate(&result, module, ledger, limits, left),
    )?;
    #[cfg(test)]
    {
        super::source_probe::record_stage(module, "after-lower")?;
        super::source_probe::record_semantic_stage(&result, "after-lower")?;
    }
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: ghost capture",
        left,
        |left| super::ghost_capture::populate(&mut result, module, ledger, limits, left),
    )?;
    #[cfg(test)]
    {
        super::source_probe::record_stage(module, "after-ghost")?;
        super::source_probe::record_semantic_stage(&result, "after-ghost")?;
    }
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: deferred nominals",
        left,
        |left| {
            super::materialize::populate_deferred_nominals(&result, module, ledger, limits, left)
        },
    )?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: complete headers",
        left,
        |left| super::headers::complete_emitted_headers(&mut result, module, ledger, limits, left),
    )?;
    // Captured-head embeddings are derived from the actual entry's reads;
    // materializing a whole closure must not invent those persistent images.
    let mut headers = stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: header bindings",
        left,
        |left| super::headers::program(&mut result, module, left),
    )?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: capture domains",
        left,
        |left| {
            super::headers::capture_domains(&mut result, module, ledger, limits, left, &mut headers)
        },
    )?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: ordinary inventory",
        left,
        |left| stored::ordinary_inventory(module, limits, left),
    )?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: original nominal catalog",
        left,
        |left| super::nominal_catalog::program(&result, module, ledger, limits, left),
    )?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: creator headers",
        left,
        |left| super::creator_headers::populate(&mut result, module, ledger, limits, left),
    )?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: full materialization",
        left,
        |left| super::materialize::program(&mut result, module, ledger, limits, left),
    )?;
    #[cfg(test)]
    super::source_probe::record(module, &result, limits, left)?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: actual worlds",
        left,
        |left| super::proof::program(&result, module, ledger, &headers, limits, left),
    )?;
    stored::OrdinaryStats::step(
        &ledger.ordinary_stats,
        "finish: verify original definitions",
        left,
        |left| context.verify_definitions(&result, limits, left),
    )?;
    Ok(Some(result))
}
