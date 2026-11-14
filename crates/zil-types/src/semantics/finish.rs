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
        stored::ordinary_inventory(module, limits, left)?;
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
    if let Some(source) = ledger.frozen.as_deref() {
        crate::dyn_metadata::semantic_snapshot(&source.semantic, limits, left, 1)?;
        for definition in &source.semantic.definitions {
            *left = left
                .checked_sub(result.definitions.len().saturating_add(1))
                .ok_or_else(|| {
                    Diagnostic::limit(
                        Phase::Types,
                        "frozen original definition merge exceeded",
                        None,
                    )
                })?;
            if let Some(actual) = result
                .definitions
                .iter()
                .find(|row| row.id == definition.id)
            {
                crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
                if actual != definition {
                    return Err(error(
                        "E-CT-FROZEN-TYPES",
                        Span::default(),
                        "E changes an immutable S original definition",
                    ));
                }
            } else {
                result.definitions.push(definition.clone());
            }
        }
    }
    let original = if let Some(source) = ledger.frozen.as_deref() {
        super::original::check_with_frozen(&result, source, limits, left)?
    } else {
        super::original::check(&result, limits, left)?
    };
    *ledger.original.borrow_mut() = Some(original);
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
    Ok(Some(result))
}
