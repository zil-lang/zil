//! The actual compiler-owned prelude is an original template inventory too.
//! IDs reserve a disjoint request-local range; canonical nominal identity comes
//! from the real role/AST producer, never a same-spelled user declaration.
use super::*;
use crate::generics::options;
use builtins::CompilerNominal;

pub(super) fn collect(
    modules: &[&s::Module],
    remaining: &mut Limits,
) -> CompileResult<(
    Vec<CompilerNominal>,
    Vec<SemanticDefinition>,
    Vec<LifeNominalDeclaration>,
)> {
    if modules.iter().any(|module| module.id == ModuleId(u32::MAX)) {
        return Err(vec![error(
            Span::default(),
            "source module collides with the reserved compiler definition namespace",
        )]);
    }
    let limits = remaining.clone();
    let mut budget = Budget::new(&limits);
    let result = (|| -> R<_> {
        let mut names = Vec::new();
        let mut definitions = Vec::new();
        let mut declarations = Vec::new();
        // Normalize one actual compiler prelude at a time. Holding every
        // module's temporary AST prefix here duplicates live input storage
        // without contributing any new canonical declaration identity.
        for source in modules {
            let allowance = budget.remaining_limits();
            let (rows, left) = options::compiler_semantic_declarations(source, &allowance)
                .map_err(|errors| {
                    errors.into_iter().next().unwrap_or_else(|| {
                        error(
                            source.span,
                            "compiler declaration collection failed without a diagnostic",
                        )
                    })
                })?;
            budget.adopt_remaining(&left);
            let module = source.id;
            let mut aliases = Vec::new();
            for row in rows {
                budget.node(row.item.span, 0)?;
                let Some(declaration) = row.life_declaration else {
                    if let s::ItemKind::TypeAlias(alias) = row.item.kind {
                        budget.lineage_push(&mut aliases, alias, row.item.span)?;
                    }
                    continue;
                };
                let identity = row.identity.ok_or_else(|| {
                    error(
                        row.item.span,
                        "compiler nominal role lost its canonical original identity",
                    )
                })?;
                let role = row.origin.role;
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
                let candidate = budget.lineage_checkpoint();
                let public_name = match role {
                    Some(crate::generics::origins::CompilerRole::Option) => "Option",
                    Some(crate::generics::origins::CompilerRole::Poll) => "Poll",
                    None if declaration == zil_ir::builtin_life::TASK_CREATE_ERROR => {
                        "TaskCreateError"
                    }
                    None if declaration == zil_ir::builtin_life::TASK_POLL_STATE => "TaskPollState",
                    None => {
                        return Err(error(
                            row.item.span,
                            "compiler nominal lacks its authenticated original family",
                        ));
                    }
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
                    budget.lineage_push(
                        &mut parameters,
                        SemanticFormal::Type {
                            requirements: Vec::new(),
                        },
                        formal.span,
                    )?;
                }
                let mut variants = Vec::new();
                for variant in &value.variants {
                    budget.node(variant.span, 0)?;
                    budget.text(&variant.name, variant.span)?;
                    budget.lineage_bytes(variant.name.len(), variant.span)?;
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
                                budget.lineage_push(&mut fields, SemanticTypeRecipe::Formal(SemanticFormalRef {
                                    scope: SemanticScopeRef::Ancestor(0),
                                    ordinal: u32::try_from(ordinal).map_err(|_| {
                                        error(payload.span, "compiler formal ordinal exceeds its representation")
                                    })?,
                                }), payload.span)?;
                            }
                        }
                        s::VariantFields::Record(_) => {
                            return Err(error(
                                variant.span,
                                "compiler nominal changed its original payload shape",
                            ));
                        }
                    }
                    budget.lineage_push(
                        &mut variants,
                        SemanticVariantRecipe {
                            name: variant.name.clone(),
                            fields,
                        },
                        variant.span,
                    )?;
                }
                let mut kinds = Vec::new();
                for formal in &value.generics {
                    budget.lineage_push(&mut kinds, LifeParameterKind::Type, formal.span)?;
                }
                budget
                    .lineage_bytes(std::mem::size_of::<LifeNominalDeclaration>(), row.item.span)?;
                budget.text(&identity.package, row.item.span)?;
                budget.lineage_bytes(identity.package.len(), row.item.span)?;
                for part in identity
                    .module_path
                    .iter()
                    .chain(&identity.declaration_path)
                {
                    budget.text(part, row.item.span)?;
                    budget.lineage_bytes(
                        part.len().saturating_add(std::mem::size_of::<String>()),
                        row.item.span,
                    )?;
                }
                budget.lineage_bytes(
                    identity
                        .generation_path
                        .len()
                        .saturating_mul(std::mem::size_of::<u32>()),
                    row.item.span,
                )?;
                let nominal = LifeNominalDeclaration {
                    id: declaration,
                    identity: identity.clone(),
                    kind: LifeNominalKind::Enum,
                    parameters: kinds,
                };
                crate::semantic_nominal_lower::pay_declaration(
                    &nominal,
                    &mut budget,
                    row.item.span,
                )?;
                if let Some(prior) = declarations
                    .iter()
                    .find(|prior: &&LifeNominalDeclaration| prior.id == declaration)
                {
                    if prior != &nominal {
                        return Err(error(
                            row.item.span,
                            "compiler original nominal inventory differs across insertions",
                        ));
                    }
                } else {
                    budget.lineage_push(&mut declarations, nominal, row.item.span)?;
                }
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
                    prepay_recipe_work(previous, &mut budget, row.item.span, 0)?;
                    prepay_recipe_work(&definition, &mut budget, row.item.span, 0)?;
                    if previous != &definition {
                        return Err(error(
                            row.item.span,
                            "shared compiler declaration differs across actual prelude insertions",
                        ));
                    }
                    // The duplicate's concrete fields/strings have now been
                    // compared. Destroy it before releasing only allocations
                    // made for this candidate, never a shared retained row.
                    budget.discard_lineage(candidate, definition, row.item.span)?;
                } else {
                    budget.lineage_push(&mut definitions, definition, row.item.span)?;
                }
                let binding_name = if role == Some(crate::generics::origins::CompilerRole::Option) {
                    public_name
                } else {
                    value.name.as_str()
                };
                budget.lineage_bytes(
                    binding_name.len().saturating_add(value.name.len()),
                    row.item.span,
                )?;
                budget.lineage_push(
                    &mut names,
                    CompilerNominal {
                        module,
                        name: binding_name.into(),
                        emitted_name: value.name.clone(),
                        definition: id,
                        declaration,
                        parameters: value.generics.len(),
                    },
                    row.item.span,
                )?;
            }
            for alias in aliases {
                let Some(target) = &alias.value else {
                    return Err(error(
                        source.span,
                        "compiler alias lost its original target",
                    ));
                };
                let s::TypeKind::Named { path, args } = &target.kind else {
                    return Err(error(
                        target.span,
                        "compiler alias target is not its original nominal application",
                    ));
                };
                let [name] = path.as_slice() else {
                    return Err(error(
                        target.span,
                        "compiler alias target has an unexpected namespace",
                    ));
                };
                let Some(original) = names
                    .iter()
                    .find(|row| row.module == module && row.emitted_name == *name)
                else {
                    // An unimplemented compiler family is retained by the
                    // original AST and remains explicitly unavailable here.
                    continue;
                };
                if alias.generics.len() != original.parameters || args.len() != original.parameters
                {
                    return Err(error(
                        target.span,
                        "compiler alias changed its original mixed arguments",
                    ));
                }
                for (formal, actual) in alias.generics.iter().zip(args) {
                    if !matches!((&formal.kind,actual), (s::GenericParamKind::Type(bounds),s::GenericArg::Type(actual)) if bounds.is_empty() && matches!(&actual.kind,s::TypeKind::Named{path,args} if path.as_slice()==[formal.name.as_str()] && args.is_empty()))
                    {
                        return Err(error(
                            target.span,
                            "compiler alias does not preserve its original formal",
                        ));
                    }
                }
                if names
                    .iter()
                    .any(|row| row.module == module && row.name == alias.name)
                {
                    continue;
                }
                let definition = original.definition;
                let declaration = original.declaration;
                let parameters = original.parameters;
                budget.text(&original.emitted_name, target.span)?;
                budget.lineage_bytes(original.emitted_name.len(), target.span)?;
                let emitted_name = original.emitted_name.clone();
                budget.lineage_push(
                    &mut names,
                    CompilerNominal {
                        module,
                        name: alias.name,
                        emitted_name,
                        definition,
                        declaration,
                        parameters,
                    },
                    target.span,
                )?;
            }
        }
        Ok((names, definitions, declarations))
    })();
    *remaining = budget.remaining_limits();
    result.map_err(|error| vec![error])
}
