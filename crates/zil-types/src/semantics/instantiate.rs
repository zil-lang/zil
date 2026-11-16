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
    proof: &Proof,
    id: SemanticDefinitionId,
    left: &mut usize,
) -> R<&'a SemanticDefinition> {
    // This directory belongs to the same immutable original program. Its
    // construction rejects duplicate identities before any original checking;
    // each lookup still checks the actual row, never a concrete type image.
    fee(
        left,
        proof.definition_positions.len().saturating_add(1).ilog2() as usize + 2,
    )?;
    proof
        .definition_positions
        .get(&id)
        .and_then(|position| program.definitions.get(*position))
        .filter(|row| row.id == id)
        .ok_or_else(|| bad("original substitution domain definition is absent"))
}
fn ancestry(
    program: &SemanticModule,
    proof: &Proof,
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
        next = definition(program, proof, id, left)?.parent;
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
    let source = definition(program, proof, mapping.source, left)?;
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

/// Extract a checked callable signature into its real use domain. Captured
/// actuals are already expressed there and are substituted atomically, while
/// remaining lexical references must name the same original ancestor.
pub(super) fn callable_signature(
    program: &SemanticModule,
    proof: &Proof,
    value: &SemanticSignatureRecipe,
    mapping: DomainMapping<'_>,
    environment: Option<&[SemanticActualRecipe]>,
    limits: &Limits,
    left: &mut usize,
) -> R<SemanticSignatureRecipe> {
    if let Some(actuals) = environment {
        let source = definition(program, proof, mapping.source, left)?;
        fee(left, source.environment.len().saturating_add(actuals.len()))?;
        if source.kind != SemanticDefinitionKind::Closure
            || source.environment.len() != actuals.len()
        {
            return Err(bad(
                "callable extraction omits its complete closure environment",
            ));
        }
        for (formal, actual) in source.environment.iter().zip(actuals) {
            if !matches!(
                (&formal.kind, actual),
                (SemanticFormal::Type { .. }, SemanticActualRecipe::Type(_))
                    | (SemanticFormal::Lifetime, SemanticActualRecipe::Life(_))
                    | (
                        SemanticFormal::Const { .. },
                        SemanticActualRecipe::Const { .. }
                    )
            ) {
                return Err(bad("callable extraction changes a sealed environment kind"));
            }
        }
    }
    in_mapping(
        program,
        proof,
        mapping,
        None,
        true,
        limits,
        left,
        |base, scopes, _, left| {
            let environments = environment.map(|actuals| [(0, actuals)]);
            let substitution = substitute::Substitution {
                frames: base.frames,
                selves: base.selves,
                environments: environments.as_ref().map_or(&[], |rows| rows.as_slice()),
                implicit: base.implicit,
                invocation: base.invocation,
                shift: base.shift,
            };
            substitute::signature_in_domains(value, &substitution, scopes, limits, left)
        },
    )
}
/// Map one declaration's complete obligation sequence under a single checked
/// lexical/actual correspondence. No obligation is assumed or discharged here.
/// The caller must prove every returned requirement in its own original Gamma.
pub(super) fn requirements<'a>(
    program: &SemanticModule,
    proof: &Proof,
    values: impl IntoIterator<Item = &'a SemanticRequirementRecipe>,
    mapping: DomainMapping<'_>,
    limits: &Limits,
    left: &mut usize,
) -> R<Vec<SemanticRequirementRecipe>> {
    let mut values = values.into_iter().peekable();
    fee(left, 1)?;
    if values.peek().is_none() {
        return Ok(Vec::new());
    }
    in_mapping(
        program,
        proof,
        mapping,
        None,
        true,
        limits,
        left,
        |substitution, scopes, allow, left| {
            let mut result = Vec::new();
            for value in values {
                if result.len() >= limits.max_nodes {
                    return Err(Diagnostic::limit(
                        Phase::Types,
                        "original obligation sequence node limit exceeded",
                        None,
                    ));
                }
                // Covers the amortized vector allocation/movement in addition
                // to each complete recipe's own copy and substitution charge.
                fee(
                    left,
                    2 * std::mem::size_of::<SemanticRequirementRecipe>() + 1,
                )?;
                result.push(substitute::requirement_in_domains(
                    value,
                    substitution,
                    scopes,
                    allow,
                    limits,
                    left,
                )?);
            }
            Ok(result)
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
    let source = definition(program, proof, mapping.source, left)?;
    let source_chain = ancestry(program, proof, mapping.source, limits, left)?;
    let use_chain = ancestry(program, proof, mapping.use_definition, limits, left)?;
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
        let original = definition(program, proof, method.parent, left)?;
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
        let original = definition(program, proof, source_chain[*hop as usize], left)?;
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

#[cfg(test)]
mod mapping_tests {
    use super::*;
    fn id(index: u32) -> SemanticDefinitionId {
        SemanticDefinitionId {
            module: zil_base::ModuleId(0),
            index,
        }
    }
    fn formal(hop: u32, ordinal: u32) -> SemanticFormalRef {
        SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(hop),
            ordinal,
        }
    }
    fn fixture() -> (SemanticModule, Proof, Vec<SemanticActualRecipe>) {
        let definitions = (0..3)
            .map(|index| SemanticDefinition {
                id: id(index),
                identity: LifeDeclarationIdentity {
                    package: "mapping-test".into(),
                    module_path: vec![],
                    declaration_path: vec![index.to_string()],
                    generation_path: vec![],
                },
                parent: (index != 0).then_some(id(0)),
                kind: SemanticDefinitionKind::Function,
                parameters: if index == 1 {
                    vec![
                        SemanticFormal::Type {
                            requirements: vec![],
                        },
                        SemanticFormal::Lifetime,
                        SemanticFormal::Const {
                            carrier: SemanticTypeRecipe::Atom(Type::I32),
                        },
                    ]
                } else {
                    vec![SemanticFormal::Lifetime]
                },
                implicit_lifetimes: vec![],
                requirements: vec![],
                environment: vec![],
                body: SemanticDefinitionBody::Function {
                    signature: SemanticSignatureRecipe {
                        bound_lifetimes: 0,
                        invocation_lifetimes: 0,
                        parameters: vec![],
                        result: SemanticTypeRecipe::Atom(Type::Unit),
                        result_mode: PassingMode::Copy,
                        call_kind: CallKind::Direct,
                    },
                    sites: vec![],
                },
            })
            .collect();
        let program = SemanticModule {
            definitions,
            instances: vec![],
            sites: vec![],
            dyn_scopes: vec![],
        };
        let mut proof = Proof::default();
        proof.definition_positions = std::rc::Rc::new(
            program
                .definitions
                .iter()
                .enumerate()
                .map(|(position, row)| (row.id, position))
                .collect(),
        );
        let actuals = vec![
            SemanticActualRecipe::Type(SemanticTypeRecipe::Atom(Type::I64)),
            SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(formal(0, 0))),
            SemanticActualRecipe::Const {
                carrier: SemanticTypeRecipe::Atom(Type::I32),
                value: SemanticConstRecipe::Value(Constant::I32(7)),
            },
        ];
        (program, proof, actuals)
    }
    #[test]
    fn batched_obligations_keep_full_mixed_and_protected_domains() {
        let (program, proof, actuals) = fixture();
        let values = vec![
            SemanticRequirementRecipe::TypeEqual {
                left: SemanticTypeRecipe::Formal(formal(0, 0)),
                right: SemanticTypeRecipe::Atom(Type::I64),
            },
            SemanticRequirementRecipe::ConstEqual {
                carrier: SemanticTypeRecipe::Atom(Type::I32),
                left: SemanticConstRecipe::Formal(formal(0, 2)),
                right: SemanticConstRecipe::Value(Constant::I32(7)),
            },
            SemanticRequirementRecipe::ForAll {
                lifetimes: 1,
                requirements: vec![
                    SemanticRequirementRecipe::Outlives {
                        longer: SemanticLifeRecipe::Formal(formal(0, 1)),
                        shorter: SemanticLifeRecipe::Bound { depth: 0, index: 0 },
                    },
                    SemanticRequirementRecipe::LifeEqual {
                        left: SemanticLifeRecipe::Formal(formal(1, 0)),
                        right: SemanticLifeRecipe::Formal(formal(1, 0)),
                    },
                ],
            },
        ];
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        let actual = requirements(
            &program,
            &proof,
            &values,
            DomainMapping {
                source: id(1),
                use_definition: id(2),
                arguments: &actuals,
                parent_call: None,
            },
            &limits,
            &mut left,
        )
        .unwrap();
        assert_eq!(
            actual[0],
            SemanticRequirementRecipe::TypeEqual {
                left: SemanticTypeRecipe::Atom(Type::I64),
                right: SemanticTypeRecipe::Atom(Type::I64),
            }
        );
        assert_eq!(
            actual[1],
            SemanticRequirementRecipe::ConstEqual {
                carrier: SemanticTypeRecipe::Atom(Type::I32),
                left: SemanticConstRecipe::Value(Constant::I32(7)),
                right: SemanticConstRecipe::Value(Constant::I32(7)),
            }
        );
        let SemanticRequirementRecipe::ForAll {
            requirements: nested,
            ..
        } = &actual[2]
        else {
            panic!()
        };
        assert_eq!(
            nested[0],
            SemanticRequirementRecipe::Outlives {
                longer: SemanticLifeRecipe::Formal(formal(0, 0)),
                shorter: SemanticLifeRecipe::Bound { depth: 0, index: 0 },
            }
        );
        for (value, expected) in values.iter().zip(&actual) {
            assert_eq!(
                &requirement(
                    &program,
                    &proof,
                    value,
                    DomainMapping {
                        source: id(1),
                        use_definition: id(2),
                        arguments: &actuals,
                        parent_call: None,
                    },
                    &limits,
                    &mut left
                )
                .unwrap(),
                expected
            );
        }
        let mut wrong = actuals.clone();
        wrong.swap(0, 1);
        assert!(
            requirements(
                &program,
                &proof,
                &values,
                DomainMapping {
                    source: id(1),
                    use_definition: id(2),
                    arguments: &wrong,
                    parent_call: None,
                },
                &limits,
                &mut left
            )
            .is_err()
        );
        wrong = actuals.clone();
        let SemanticActualRecipe::Const { carrier, .. } = &mut wrong[2] else {
            panic!()
        };
        *carrier = SemanticTypeRecipe::Atom(Type::I64);
        assert!(
            requirements(
                &program,
                &proof,
                &values,
                DomainMapping {
                    source: id(1),
                    use_definition: id(2),
                    arguments: &wrong,
                    parent_call: None,
                },
                &limits,
                &mut left
            )
            .is_err()
        );
        let mut unrelated = program.clone();
        unrelated.definitions[2].parent = None;
        assert!(
            requirements(
                &unrelated,
                &proof,
                &values,
                DomainMapping {
                    source: id(1),
                    use_definition: id(2),
                    arguments: &actuals,
                    parent_call: None,
                },
                &limits,
                &mut left
            )
            .is_err()
        );
    }
    #[test]
    fn declaration_index_rejects_wrong_row_and_ancestry_cycle() {
        let (mut program, mut proof, _) = fixture();
        let limits = Limits::default();
        let mut left = limits.max_analysis_steps;
        std::rc::Rc::make_mut(&mut proof.definition_positions).insert(id(1), 2);
        assert!(definition(&program, &proof, id(1), &mut left).is_err());
        std::rc::Rc::make_mut(&mut proof.definition_positions).insert(id(1), 1);
        program.definitions[0].parent = Some(id(1));
        assert!(ancestry(&program, &proof, id(1), &limits, &mut left).is_err());
    }
}
