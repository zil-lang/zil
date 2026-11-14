//! Relocate actual registry records through real emitter identities. This is
//! correspondence data, never a provider/where/cleanup permission theorem.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn relocate(
    program: &SemanticModule,
    module: &mut TypedModule,
    ledger: &Ledger,
    limits: &Limits,
    left: &mut usize,
) -> R<()> {
    provider_fee(
        left,
        module.functions.len().saturating_add(ledger.frames.len()),
    )?;
    let mut functions = BTreeMap::new();
    for function in &module.functions {
        provider_fee(left, functions.len().saturating_add(1).ilog2() as usize + 1)?;
        if functions.insert(function.id, function).is_some() {
            return Err(bad(
                "Dyn relocation has duplicate actual function identities",
            ));
        }
    }
    let mut origins: BTreeMap<(zil_base::ModuleId, NodeId), Vec<FunctionId>> = BTreeMap::new();
    for (function, frame) in &ledger.frames {
        if frame.origin.closure.is_some() || frame.parent.is_some() {
            continue;
        }
        provider_fee(left, origins.len().saturating_add(1).ilog2() as usize + 1)?;
        origins
            .entry((frame.origin.module, frame.origin.creator))
            .or_default()
            .push(*function);
    }
    let mut changes = Vec::new();
    let mut seen = BTreeSet::new();
    let mut records = 0usize;
    for (declaration_index, declaration) in module.dyn_declarations.iter().enumerate() {
        provider_fee(left, 1)?;
        let Some(definitions) = &declaration.method_definitions else {
            continue;
        };
        provider_fee(left, definitions.len())?;
        for (definition_index, definition) in definitions.iter().enumerate() {
            if definition.instances.is_empty() {
                continue;
            }
            let original = exact_scope(
                program,
                SemanticDynOwner::MethodDefinition(definition.id),
                left,
            )?;
            if original.kind != SemanticDefinitionKind::Method {
                return Err(bad(
                    "Dyn registry target is not an original method definition",
                ));
            }
            crate::dyn_metadata::semantic_operand_snapshot(&definition.identity, limits, left, 1)?;
            crate::dyn_metadata::semantic_operand_snapshot(&original.identity, limits, left, 1)?;
            if definition.identity != original.identity {
                return Err(bad(
                    "Dyn registry method changes its complete original identity",
                ));
            }
            exact_signature_recipe(&definition.signature, original, left)?;
            provider_fee(left, definition.instances.len())?;
            for (specialization_index, specialization) in definition.instances.iter().enumerate() {
                records = records.saturating_add(1);
                if records > limits.max_nodes {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "Dyn registry relocation exceeds node limit",
                        None,
                    ));
                }
                crate::dyn_metadata::semantic_operand_snapshot(specialization, limits, left, 1)?;
                let candidates: &[FunctionId] = match &specialization.function {
                    DynFunctionTarget::Core { module, source } => {
                        provider_fee(left, origins.len().saturating_add(1).ilog2() as usize + 1)?;
                        origins
                            .get(&(*module, *source))
                            .map(Vec::as_slice)
                            .unwrap_or(&[])
                    }
                    DynFunctionTarget::Typed(function) => std::slice::from_ref(function),
                };
                let mut selected = None;
                provider_fee(left, candidates.len())?;
                for id in candidates {
                    provider_fee(left, functions.len().saturating_add(1).ilog2() as usize + 1)?;
                    let Some(function) = functions.get(id).copied() else {
                        continue;
                    };
                    let Some(actual) = &function.dyn_instance else {
                        continue;
                    };
                    crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
                    if actual.definition != definition.id
                        || actual.receiver != specialization.receiver
                        || actual.declaration_actuals != specialization.declaration_actuals
                        || actual.method_actuals != specialization.method_actuals
                    {
                        continue;
                    }
                    let frame = ledger
                        .frames
                        .get(id)
                        .ok_or_else(|| bad("Dyn specialization lacks its actual emitter frame"))?;
                    if frame.parent.is_some() || frame.origin.closure.is_some() {
                        continue;
                    }
                    provider_fee(left, program.instances.len())?;
                    let mut found = false;
                    for instance in &program.instances {
                        if !matches!(instance.target, SemanticInstanceTarget::Function(SemanticFunctionTarget::Typed(target)) if target == *id)
                        {
                            continue;
                        }
                        if instance.definition != original.id || instance.parent.is_none() {
                            return Err(bad(
                                "Dyn actual target changes its original method or lexical scope",
                            ));
                        }
                        found = true;
                    }
                    if !found {
                        continue;
                    }
                    if selected.replace(*id).is_some() {
                        return Err(bad(
                            "Dyn registry specialization has multiple exact actual emitted targets",
                        ));
                    }
                }
                let target = selected.ok_or_else(|| {
                    let mut diagnostic = bad("Dyn registry specialization has no exact actual emitted target");
                    diagnostic.notes.push(format!("Dyn declaration {:?}; definition {:?}; original method {:?}; source target {:?}; candidate Typed functions {:?}", declaration.id, definition.id, original.id, specialization.function, candidates));
                    diagnostic
                })?;
                provider_fee(left, seen.len().saturating_add(1).ilog2() as usize + 2)?;
                if !seen.insert((definition.id, target)) {
                    return Err(bad("Dyn registry repeats an actual method specialization"));
                }
                changes.push((
                    declaration_index,
                    definition_index,
                    specialization_index,
                    target,
                ));
            }
        }
    }
    for (declaration, definition, specialization, target) in changes {
        provider_fee(left, 1)?;
        let record = module
            .dyn_declarations
            .get_mut(declaration)
            .and_then(|row| row.method_definitions.as_mut())
            .and_then(|rows| rows.get_mut(definition))
            .and_then(|row| row.instances.get_mut(specialization))
            .ok_or_else(|| bad("Dyn registry record disappeared during relocation"))?;
        record.function = DynFunctionTarget::Typed(target);
    }
    Ok(())
}
