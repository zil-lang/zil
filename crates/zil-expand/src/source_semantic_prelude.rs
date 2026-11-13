//! The actual compiler-owned prelude is an original template inventory too.
//! IDs reserve a disjoint request-local range; canonical nominal identity comes
//! from the real role/AST producer, never a same-spelled user declaration.
use super::*;
use crate::generics::options;
use builtins::CompilerNominal;

pub(super) fn collect(
    modules: &[&s::Module],
    remaining: &mut Limits,
) -> CompileResult<(Vec<CompilerNominal>, Vec<SemanticDefinition>)> {
    if modules.iter().any(|module| module.id == ModuleId(u32::MAX)) {
        return Err(vec![error(
            Span::default(),
            "source module collides with the reserved compiler definition namespace",
        )]);
    }
    let mut originals = Vec::new();
    for module in modules {
        let (rows, left) = options::compiler_semantic_declarations(module, remaining)?;
        *remaining = left;
        originals.push((module.id, rows));
    }
    let limits = remaining.clone();
    let mut budget = Budget::new(&limits);
    budget.limit_temporary_bytes(limits.max_source_bytes);
    let result = (|| -> R<_> {
        let mut names = Vec::new();
        let mut definitions = Vec::new();
        for (module, rows) in originals {
            for row in rows {
                budget.node(row.item.span, 0)?;
                let Some(declaration) = row.life_declaration else {
                    // Closed enums and aliases remain in the complete compiler
                    // AST inventory. They carry no generic nominal Life family.
                    // builtin_type normalizes their actual complete carrier.
                    continue;
                };
                let identity = row.identity.ok_or_else(|| {
                    error(
                        row.item.span,
                        "compiler nominal role lost its canonical original identity",
                    )
                })?;
                let role = row.origin.role.ok_or_else(|| {
                    error(
                        row.item.span,
                        "compiler nominal lacks its actual insertion role",
                    )
                })?;
                let s::ItemKind::Enum(value) = row.item.kind else {
                    return Err(error(
                        row.item.span,
                        "compiler nominal template is not its actual enum",
                    ));
                };
                // The actual canonical compiler directory is a single
                // reserved namespace, shared by all source modules. It is not
                // chosen from a same-named user declaration or output row.
                let id = SemanticDefinitionId {
                    module: declaration.module,
                    index: declaration.declaration.0,
                };
                let public_name = match role {
                    crate::generics::origins::CompilerRole::Option => "Option",
                    crate::generics::origins::CompilerRole::Poll => "Poll",
                };
                let mut parameters = Vec::new();
                for formal in &value.generics {
                    budget.node(formal.span, 0)?;
                    let s::GenericParamKind::Type(bounds) = &formal.kind else {
                        return Err(error(
                            formal.span,
                            "actual compiler nominal formal is not a Type",
                        ));
                    };
                    if !bounds.is_empty() {
                        return Err(error(
                            formal.span,
                            "compiler nominal formal changed its original constraints",
                        ));
                    }
                    budget.lineage_bytes(std::mem::size_of::<SemanticFormal>(), formal.span)?;
                    parameters.push(SemanticFormal::Type {
                        requirements: Vec::new(),
                    });
                }
                let mut variants = Vec::new();
                for variant in &value.variants {
                    budget.node(variant.span, 0)?;
                    budget.text(&variant.name, variant.span)?;
                    budget.lineage_bytes(
                        variant.name.len() + std::mem::size_of::<SemanticVariantRecipe>(),
                        variant.span,
                    )?;
                    let mut fields = Vec::new();
                    match &variant.fields {
                        s::VariantFields::Unit => {}
                        s::VariantFields::Tuple(payloads) => {
                            for payload in payloads {
                                budget.node(payload.span, 0)?;
                                let s::TypeKind::Named { path, args } = &payload.ty.kind else {
                                    return Err(error(
                                        payload.span,
                                        "compiler nominal payload is not its original formal",
                                    ));
                                };
                                let [name] = path.as_slice() else {
                                    return Err(error(
                                        payload.span,
                                        "compiler nominal payload has another owner",
                                    ));
                                };
                                if !args.is_empty() {
                                    return Err(error(
                                        payload.span,
                                        "compiler nominal payload changed its formal application",
                                    ));
                                }
                                let ordinal = value
                                    .generics
                                    .iter()
                                    .position(|formal| formal.name == *name)
                                    .ok_or_else(|| {
                                        error(payload.span, "compiler payload formal is absent")
                                    })?;
                                budget.lineage_bytes(
                                    std::mem::size_of::<SemanticTypeRecipe>(),
                                    payload.span,
                                )?;
                                fields.push(SemanticTypeRecipe::Formal(SemanticFormalRef {
                                    scope: SemanticScopeRef::Ancestor(0),
                                    ordinal: u32::try_from(ordinal).map_err(|_| {
                                        error(
                                            payload.span,
                                            "compiler formal ordinal exceeds its representation",
                                        )
                                    })?,
                                }));
                            }
                        }
                        s::VariantFields::Record(_) => {
                            return Err(error(
                                variant.span,
                                "compiler nominal changed its original payload shape",
                            ));
                        }
                    }
                    variants.push(SemanticVariantRecipe {
                        name: variant.name.clone(),
                        fields,
                    });
                }
                budget.lineage_bytes(
                    public_name.len()
                        + std::mem::size_of::<CompilerNominal>()
                        + std::mem::size_of::<SemanticDefinition>(),
                    row.item.span,
                )?;
                names.push(CompilerNominal {
                    module,
                    name: public_name.into(),
                    definition: id,
                    declaration,
                });
                let definition = SemanticDefinition {
                    id,
                    identity,
                    parent: None,
                    kind: SemanticDefinitionKind::Nominal,
                    parameters,
                    implicit_lifetimes: Vec::new(),
                    requirements: Vec::new(),
                    environment: Vec::new(),
                    body: SemanticDefinitionBody::Nominal {
                        declaration,
                        kind: LifeNominalKind::Enum,
                        body: SemanticNominalBody::Enum { variants },
                    },
                };
                if let Some(previous) = definitions
                    .iter()
                    .find(|previous: &&SemanticDefinition| previous.id == id)
                {
                    prepay_recipe(previous, &mut budget, row.item.span, 0)?;
                    prepay_recipe(&definition, &mut budget, row.item.span, 0)?;
                    if previous != &definition {
                        return Err(error(
                            row.item.span,
                            "shared compiler declaration differs across actual prelude insertions",
                        ));
                    }
                } else {
                    definitions.push(definition);
                }
            }
        }
        Ok((names, definitions))
    })();
    *remaining = budget.remaining_limits();
    result.map_err(|error| vec![error])
}
