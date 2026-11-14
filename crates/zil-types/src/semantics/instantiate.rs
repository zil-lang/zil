//! Full original-domain substitution at a real nominal use. Domain identities
//! are mandatory: a source ancestor never becomes a same-numbered use ancestor.
use super::original::{Proof, substitute};
use crate::*;
use std::collections::BTreeMap;

pub(super) struct DomainMapping<'a> {
    pub source: SemanticDefinitionId,
    pub use_definition: SemanticDefinitionId,
    pub arguments: &'a [SemanticActualRecipe],
    /// Optional original method boundary proving a nonlexical parent Scope.
    pub parent_call: Option<SemanticSiteId>,
}
fn bad(message: &str) -> Diagnostic {
    error("E-SEMANTIC-ORIGINAL-DOMAIN", Span::default(), message)
}
fn fee(left: &mut usize, n: usize) -> R<()> {
    *left = left.checked_sub(n).ok_or_else(|| {
        Diagnostic::limit(
            Phase::Types,
            "original domain substitution work exceeded",
            None,
        )
    })?;
    Ok(())
}
fn definition<'a>(
    program: &'a SemanticModule,
    id: SemanticDefinitionId,
    left: &mut usize,
) -> R<&'a SemanticDefinition> {
    fee(left, program.definitions.len())?;
    let mut rows = program.definitions.iter().filter(|row| row.id == id);
    let row = rows
        .next()
        .ok_or_else(|| bad("original substitution domain definition is absent"))?;
    if rows.next().is_some() {
        return Err(bad("original substitution domain definition is duplicated"));
    }
    Ok(row)
}
fn ancestry(
    program: &SemanticModule,
    id: SemanticDefinitionId,
    limits: &Limits,
    left: &mut usize,
) -> R<Vec<SemanticDefinitionId>> {
    let mut result = Vec::new();
    let mut next = Some(id);
    while let Some(id) = next {
        fee(left, result.len().saturating_add(1))?;
        if result.len() > limits.max_call_depth {
            return Err(Diagnostic::limit(
                Phase::Types,
                "original domain ancestry depth exceeded",
                None,
            ));
        }
        if result.contains(&id) {
            return Err(bad("original substitution domain ancestry is cyclic"));
        }
        next = definition(program, id, left)?.parent;
        result.push(id);
    }
    Ok(result)
}
fn kinds(
    original: &SemanticDefinition,
    actuals: &[SemanticActualRecipe],
    left: &mut usize,
) -> R<()> {
    fee(
        left,
        original.parameters.len().saturating_add(actuals.len()),
    )?;
    if original.parameters.len() != actuals.len() {
        return Err(bad(
            "original domain substitution omits full mixed actual positions",
        ));
    }
    for (formal, actual) in original.parameters.iter().zip(actuals) {
        if !matches!(
            (formal, actual),
            (SemanticFormal::Type { .. }, SemanticActualRecipe::Type(_))
                | (SemanticFormal::Lifetime, SemanticActualRecipe::Life(_))
                | (
                    SemanticFormal::Const { .. },
                    SemanticActualRecipe::Const { .. }
                )
        ) {
            return Err(bad(
                "original domain substitution changes a mixed formal kind",
            ));
        }
    }
    Ok(())
}
pub(super) fn ty(
    program: &SemanticModule,
    proof: &Proof,
    value: &SemanticTypeRecipe,
    mapping: DomainMapping<'_>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    ty_using_self(program, proof, value, mapping, None, true, limits, left)
}
/// Evaluate a real declaration Scope's recipe after the original trait
/// selection has proved its receiver and all mixed arguments. This operation
/// supplies data substitution only; it grants no trait or where obligation.
pub(super) fn ty_with_self(
    program: &SemanticModule,
    proof: &Proof,
    value: &SemanticTypeRecipe,
    mapping: DomainMapping<'_>,
    receiver: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    ty_using_self(
        program,
        proof,
        value,
        mapping,
        Some(receiver),
        true,
        limits,
        left,
    )
}
/// Derive a Const actual's carrier before installing the complete carrier
/// vector. This only evaluates the real source formal's Type expression; the
/// caller must retain every full mixed actual and then check the value in it.
pub(super) fn carrier(
    program: &SemanticModule,
    proof: &Proof,
    mapping: DomainMapping<'_>,
    ordinal: u32,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    let source = definition(program, mapping.source, left)?;
    let Some(SemanticFormal::Const { carrier }) = source.parameters.get(ordinal as usize) else {
        return Err(bad(
            "original carrier request does not select a Const formal",
        ));
    };
    ty_using_self(program, proof, carrier, mapping, None, false, limits, left)
}
fn ty_using_self(
    program: &SemanticModule,
    proof: &Proof,
    value: &SemanticTypeRecipe,
    mapping: DomainMapping<'_>,
    receiver: Option<&SemanticTypeRecipe>,
    check_carriers: bool,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticTypeRecipe> {
    in_mapping(
        program,
        proof,
        mapping,
        receiver,
        check_carriers,
        limits,
        left,
        |substitution, scopes, allow, left| {
            substitute::ty_in_domains(value, substitution, scopes, allow, limits, left)
        },
    )
}
/// Substitute a nominal declaration's obligations without inventing a Self
/// binder. Only the separate Scope entry below accepts a proved receiver.
pub(super) fn requirement(
    program: &SemanticModule,
    proof: &Proof,
    value: &SemanticRequirementRecipe,
    mapping: DomainMapping<'_>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticRequirementRecipe> {
    in_mapping(
        program,
        proof,
        mapping,
        None,
        true,
        limits,
        left,
        |substitution, scopes, allow, left| {
            substitute::requirement_in_domains(value, substitution, scopes, allow, limits, left)
        },
    )
}
pub(super) fn requirement_with_self(
    program: &SemanticModule,
    proof: &Proof,
    value: &SemanticRequirementRecipe,
    mapping: DomainMapping<'_>,
    receiver: &SemanticTypeRecipe,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticRequirementRecipe> {
    in_mapping(
        program,
        proof,
        mapping,
        Some(receiver),
        true,
        limits,
        left,
        |substitution, scopes, allow, left| {
            substitute::requirement_in_domains(value, substitution, scopes, allow, limits, left)
        },
    )
}
fn in_mapping<T>(
    program: &SemanticModule,
    proof: &Proof,
    mapping: DomainMapping<'_>,
    receiver: Option<&SemanticTypeRecipe>,
    check_carriers: bool,
    limits: &Limits,
    left: &mut usize,
    run: impl FnOnce(&substitute::Substitution<'_>, &BTreeMap<u32, u32>, bool, &mut usize) -> R<T>,
) -> R<T> {
    let source = definition(program, mapping.source, left)?;
    let source_chain = ancestry(program, mapping.source, limits, left)?;
    let use_chain = ancestry(program, mapping.use_definition, limits, left)?;
    kinds(source, mapping.arguments, left)?;
    let mut scopes = BTreeMap::new();
    fee(left, source_chain.len().saturating_mul(use_chain.len()))?;
    for (source_hop, id) in source_chain.iter().enumerate() {
        if let Some(use_hop) = use_chain.iter().position(|candidate| candidate == id) {
            fee(left, 1)?;
            scopes.insert(
                u32::try_from(source_hop).map_err(|_| bad("original source depth overflows"))?,
                u32::try_from(use_hop).map_err(|_| bad("original use depth overflows"))?,
            );
        }
    }
    let mut frames = Vec::new();
    let mut selves = Vec::new();
    fee(left, source_chain.len().saturating_add(1))?;
    frames.push((0, mapping.arguments));
    if let Some(receiver) = receiver {
        if !matches!(source.body, SemanticDefinitionBody::Scope(_)) {
            return Err(bad(
                "original Self substitution does not name a real declaration Scope",
            ));
        }
        crate::dyn_metadata::semantic_operand_snapshot(receiver, limits, left, 1)?;
        selves.push((0, receiver));
    }
    if let Some(call) = mapping.parent_call {
        fee(left, source_chain.len().saturating_add(2))?;
        let method = proof
            .method_calls
            .get(&(mapping.use_definition, call))
            .ok_or_else(|| bad("original parent domain has no proved method call boundary"))?;
        let hop = source_chain
            .iter()
            .position(|id| *id == method.parent)
            .ok_or_else(|| {
                bad("original method parent does not own the substituted declaration")
            })?;
        if hop == 0 {
            return Err(bad(
                "original nominal own arguments cannot be replaced by a parent Scope vector",
            ));
        }
        let original = definition(program, method.parent, left)?;
        if !matches!(original.body, SemanticDefinitionBody::Scope(_)) {
            return Err(bad(
                "original method parent mapping names a non-Scope declaration",
            ));
        }
        let actuals = proof
            .scope_calls
            .get(&(mapping.use_definition, call, method.parent))
            .ok_or_else(|| bad("original method parent has no full mixed Scope solution"))?;
        kinds(original, actuals, left)?;
        let hop = u32::try_from(hop).map_err(|_| bad("original parent Scope depth overflows"))?;
        frames.push((hop, actuals.as_slice()));
        selves.push((hop, &method.self_type));
    }
    let empty = BTreeMap::new();
    let substitution = substitute::Substitution {
        frames: &frames,
        selves: &selves,
        environments: &[],
        implicit: &empty,
        invocation: &empty,
        shift: 0,
    };
    // Const carriers are checked in the same source-to-use mapping as fields.
    // A concrete actual image can never discharge a rigid carrier difference.
    for (hop, actuals) in &frames {
        if !check_carriers {
            continue;
        }
        let original = definition(program, source_chain[*hop as usize], left)?;
        for (formal, actual) in original.parameters.iter().zip(*actuals) {
            if let (
                SemanticFormal::Const { carrier },
                SemanticActualRecipe::Const {
                    carrier: actual, ..
                },
            ) = (formal, actual)
            {
                if *hop != 0 {
                    // The already proved parent solution is in the use domain;
                    // its carrier is independently rechecked at that call.
                    crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
                    continue;
                }
                let carrier = substitute::ty_in_domains(
                    carrier,
                    &substitution,
                    &scopes,
                    mapping.source == mapping.use_definition,
                    limits,
                    left,
                )?;
                crate::dyn_metadata::semantic_operand_snapshot(actual, limits, left, 1)?;
                if &carrier != actual {
                    return Err(bad(
                        "original substitution changes a full rigid Const carrier",
                    ));
                }
            }
        }
    }
    run(
        &substitution,
        &scopes,
        mapping.source == mapping.use_definition,
        left,
    )
}
