//! The actual compiler-owned prelude is an original template inventory too.
//! IDs reserve a disjoint request-local range; canonical nominal identity comes
//! from the real role/AST producer, never a same-spelled user declaration.
use super::*;
use crate::generics::options;
use builtins::CompilerNominal;

fn pay_identity_copy(
    identity: &LifeDeclarationIdentity,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<()> {
    budget.text(&identity.package, span)?;
    budget.lineage_bytes(identity.package.len(), span)?;
    for values in [&identity.module_path, &identity.declaration_path] {
        budget.lineage_bytes(
            values
                .len()
                .checked_mul(std::mem::size_of::<String>())
                .ok_or_else(|| error(span, "compiler original identity rows overflow"))?,
            span,
        )?;
        for value in values {
            budget.text(value, span)?;
            budget.lineage_bytes(value.len(), span)?;
        }
    }
    budget.identity_work(identity.generation_path.len(), span)?;
    budget.lineage_bytes(
        identity
            .generation_path
            .len()
            .checked_mul(std::mem::size_of::<u32>())
            .ok_or_else(|| error(span, "compiler original generation rows overflow"))?,
        span,
    )
}
fn failure_product(
    value: &s::Aggregate,
    span: Span,
    module: ModuleId,
    declaration: LifeDeclarationId,
    identity: LifeDeclarationIdentity,
    names: &mut Vec<CompilerNominal>,
    definitions: &mut Vec<SemanticDefinition>,
    declarations: &mut Vec<LifeNominalDeclaration>,
    budget: &mut Budget<'_>,
) -> R<()> {
    let kind = match (declaration, value.kind) {
        (zil_ir::builtin_life::TASK_CREATE_FAILURE_RECORD, s::AggregateKind::Struct) => {
            LifeNominalKind::Record
        }
        (zil_ir::builtin_life::TASK_CREATE_FAILURE_PACKAGE, s::AggregateKind::Package) => {
            LifeNominalKind::Package
        }
        _ => {
            return Err(error(
                span,
                "compiler product family and its actual kind differ",
            ));
        }
    };
    let id = SemanticDefinitionId {
        module: declaration.module,
        index: declaration.declaration.0,
    };
    let checkpoint = budget.lineage_checkpoint();
    let mut parameters = Vec::new();
    let mut kinds = Vec::new();
    if value.generics.len() != 2 {
        return Err(error(
            span,
            "compiler task failure lost its complete mixed header",
        ));
    }
    for formal in &value.generics {
        budget.node(formal.span, 0)?;
        if !matches!(&formal.kind,s::GenericParamKind::Type(bounds) if bounds.is_empty()) {
            return Err(error(
                formal.span,
                "compiler task failure parameter kind differs",
            ));
        }
        budget.lineage_push(
            &mut parameters,
            SemanticFormal::Type {
                requirements: Vec::new(),
            },
            formal.span,
        )?;
        budget.lineage_push(&mut kinds, LifeParameterKind::Type, formal.span)?;
    }
    let mut fields = Vec::new();
    for (index, field) in value.fields.iter().enumerate() {
        budget.node(field.span, 0)?;
        let ty = match (index, &field.ty.kind) {
            (0, s::TypeKind::Named { path, args })
                if field.name == "reason"
                    && path.as_slice() == [options::TASK_CREATE_ERROR]
                    && args.is_empty() =>
            {
                SemanticTypeRecipe::Nominal {
                    definition: SemanticDefinitionId {
                        module: zil_ir::builtin_life::TASK_CREATE_ERROR.module,
                        index: zil_ir::builtin_life::TASK_CREATE_ERROR.declaration.0,
                    },
                    arguments: Vec::new(),
                }
            }
            (1, s::TypeKind::Named { path, args })
                if field.name == "inputs"
                    && path.as_slice() == [value.generics[0].name.as_str()]
                    && args.is_empty() =>
            {
                SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: 0,
                })
            }
            _ => {
                return Err(error(
                    field.span,
                    "compiler task failure field differs from its original binder",
                ));
            }
        };
        if field.manages || !field.public || !field.attributes.is_empty() || field.default.is_some()
        {
            return Err(error(
                field.span,
                "compiler task failure changed its actual value field",
            ));
        }
        budget.text(&field.name, field.span)?;
        budget.lineage_bytes(field.name.len(), field.span)?;
        budget.lineage_push(
            &mut fields,
            SemanticFieldRecipe {
                name: field.name.clone(),
                ty,
            },
            field.span,
        )?;
    }
    if fields.len() != 2 {
        return Err(error(
            span,
            "compiler task failure does not have both actual fields",
        ));
    }
    pay_identity_copy(&identity, budget, span)?;
    let nominal = LifeNominalDeclaration {
        id: declaration,
        identity: identity.clone(),
        kind,
        parameters: kinds,
    };
    budget.identity_work(declarations.len(), span)?;
    if let Some(old) = declarations.iter().find(|old| old.id == declaration) {
        if old != &nominal {
            return Err(error(
                span,
                "compiler task failure original catalogue differs across modules",
            ));
        }
    } else {
        budget.lineage_push(declarations, nominal, span)?;
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
            kind,
            body: SemanticNominalBody::Product { fields },
        },
    };
    budget.identity_work(definitions.len(), span)?;
    if let Some(old) = definitions.iter().find(|old| old.id == id) {
        prepay_recipe_work(old, budget, span, 0)?;
        prepay_recipe_work(&definition, budget, span, 0)?;
        if old != &definition {
            return Err(error(
                span,
                "compiler task failure original body differs across insertions",
            ));
        }
        budget.discard_lineage(checkpoint, definition, span)?;
    } else {
        budget.lineage_push(definitions, definition, span)?;
    }
    budget.text(&value.name, span)?;
    budget.lineage_bytes(
        value
            .name
            .len()
            .checked_mul(2)
            .ok_or_else(|| error(span, "compiler task failure binding overflow"))?,
        span,
    )?;
    budget.lineage_push(
        names,
        CompilerNominal {
            module,
            name: value.name.clone(),
            emitted_name: value.name.clone(),
            definition: id,
            declaration,
            parameters: value.generics.len(),
        },
        span,
    )
}

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
                if let s::ItemKind::Aggregate(value) = &row.item.kind {
                    failure_product(
                        value,
                        row.item.span,
                        module,
                        declaration,
                        identity,
                        &mut names,
                        &mut definitions,
                        &mut declarations,
                        &mut budget,
                    )?;
                    continue;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_context::{self, DeclarationPlacement, SourceIdentityContext};
    use std::collections::BTreeMap;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn task_failure_originals_keep_both_kinds_and_the_unused_frame_formal() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(7),
            "failure-originals.zil",
            "fn worker(){task_yield();}",
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let limits = Limits::default();
        let (origins, _) = source_context::capture_original_required(
            std::slice::from_ref(&source),
            &[&syntax],
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
            &limits,
        )
        .unwrap();
        let mut remaining = limits.clone();
        let inventory =
            collect_original_semantics(&[&syntax], &origins, &[Vec::new()], &mut remaining)
                .unwrap();
        for (id, kind, label) in [
            (
                builtin_life::TASK_CREATE_FAILURE_RECORD,
                LifeNominalKind::Record,
                "record",
            ),
            (
                builtin_life::TASK_CREATE_FAILURE_PACKAGE,
                LifeNominalKind::Package,
                "package",
            ),
        ] {
            let definition = inventory.modules[0]
                .1
                .definitions
                .iter()
                .find(|definition| {
                    definition.id
                        == SemanticDefinitionId {
                            module: id.module,
                            index: id.declaration.0,
                        }
                })
                .unwrap();
            assert_eq!(definition.parameters.len(), 2);
            assert!(definition.parameters.iter().all(|formal|matches!(formal,SemanticFormal::Type {requirements} if requirements.is_empty())));
            assert!(
                definition.environment.is_empty()
                    && definition.requirements.is_empty()
                    && definition.implicit_lifetimes.is_empty()
            );
            assert_eq!(definition.identity.package, "@zil.builtin");
            assert_eq!(definition.identity.module_path, ["task"]);
            assert_eq!(
                definition.identity.declaration_path,
                [label, "TaskCreateFailure"]
            );
            let SemanticDefinitionBody::Nominal {
                declaration,
                kind: actual,
                body: SemanticNominalBody::Product { fields },
            } = &definition.body
            else {
                panic!()
            };
            assert_eq!(*declaration, id);
            assert_eq!(*actual, kind);
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "reason");
            assert_eq!(fields[1].name, "inputs");
            assert_eq!(
                fields[0].ty,
                SemanticTypeRecipe::Nominal {
                    definition: SemanticDefinitionId {
                        module: builtin_life::TASK_CREATE_ERROR.module,
                        index: builtin_life::TASK_CREATE_ERROR.declaration.0
                    },
                    arguments: Vec::new()
                }
            );
            assert_eq!(
                fields[1].ty,
                SemanticTypeRecipe::Formal(SemanticFormalRef {
                    scope: SemanticScopeRef::Ancestor(0),
                    ordinal: 0
                })
            );
            let declaration = inventory
                .compiler_declarations
                .iter()
                .find(|declaration| declaration.id == id)
                .unwrap();
            assert_eq!(declaration.kind, kind);
            assert_eq!(
                declaration.parameters,
                [LifeParameterKind::Type, LifeParameterKind::Type]
            );
        }
        let (rows, _) = options::compiler_semantic_declarations(&syntax, &limits).unwrap();
        let row = rows
            .into_iter()
            .find(|row| row.life_declaration == Some(builtin_life::TASK_CREATE_FAILURE_PACKAGE))
            .unwrap();
        let mut forged = row.item.clone();
        let s::ItemKind::Aggregate(product) = &mut forged.kind else {
            panic!()
        };
        product.fields[1].manages = true;
        assert!(options::storage_identity(&forged, row.origin, &mut Budget::new(&limits)).is_err());
        let mut forged = row.item;
        let s::ItemKind::Aggregate(product) = &mut forged.kind else {
            panic!()
        };
        product.generics.pop();
        assert!(options::storage_identity(&forged, row.origin, &mut Budget::new(&limits)).is_err());
    }
}
