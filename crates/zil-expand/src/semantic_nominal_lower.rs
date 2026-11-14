//! Original nominal declarations and actual Core mint events. This catalogue
//! proposes complete types; it never proves a zero domain or an owning head.
use super::*;
use generics::{Budget, origins::Origin};
#[path = "semantic_nominal_copy.rs"]
mod copy;

/// Reuse the complete nominal Type clone layout; metadata interpretation
/// remains outside this allocation preflight.
pub(crate) fn pay_metadata_type(
    value: &Type,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
    retained: bool,
) -> Result<(), Diagnostic> {
    if retained {
        copy::ty(value, budget, span, depth)
    } else {
        crate::stored_types::pay_type(value, budget, span, depth)
    }
}
#[derive(Clone, Debug)]
pub(super) struct SemanticNominalEmission {
    pub item: usize,
    pub node: NodeId,
    pub enumeration: bool,
}
pub(crate) struct Application {
    pub origin: Origin,
    pub generated: String,
    pub owner: Option<s::SemanticContextMarker>,
    pub arguments: ApplicationArguments,
}
// A pending symbolic application is not a complete nominal instance. Its
// physical Core declaration and original semantic recipes remain intact;
// the actual Types helper scope must materialize its complete arguments.
pub(crate) enum ApplicationArguments {
    Complete(Vec<zil_ir::LifeNominalActual>),
    Deferred(Vec<Dependency>),
}
pub(crate) struct Dependency {
    pub argument: u32,
    pub namespace: zil_ir::SemanticOriginalNamespace,
    pub formal: zil_ir::SemanticFormalRef,
}
impl Expander<'_> {
    pub(super) fn emit_semantic_nominal(&mut self, node: NodeId, enumeration: bool, span: Span) {
        if self.member_owner_work >= self.member_owner_limit || self.member_owner_nodes == 0 {
            self.diagnostics.push(Diagnostic::limit(
                Phase::Expand,
                "Source nominal emission budget exceeded",
                Some(span),
            ));
            return;
        }
        self.member_owner_work += 1;
        self.member_owner_nodes -= 1;
        self.semantic_nominal_emissions
            .push(SemanticNominalEmission {
                item: self.source_item_ordinal,
                node,
                enumeration,
            });
    }
}
pub(crate) fn originals(
    inventory: &crate::source_declarations::SourceDeclarationInventory<'_>,
    budget: &mut Budget<'_>,
) -> Result<Vec<zil_ir::LifeNominalDeclaration>, Diagnostic> {
    use zil_ir::{LifeNominalDeclaration, LifeNominalKind, LifeParameterKind};
    let mut output = Vec::new();
    for row in &inventory.declarations {
        budget.node(row.item.span, 0)?;
        let kind = match &row.item.kind {
            s::ItemKind::Aggregate(value) => match value.kind {
                s::AggregateKind::Struct => LifeNominalKind::Record,
                s::AggregateKind::Package => LifeNominalKind::Package,
                s::AggregateKind::Resource => LifeNominalKind::Resource,
            },
            s::ItemKind::Enum(_) => LifeNominalKind::Enum,
            _ => continue,
        };
        let mut parameters = Vec::new();
        for formal in row.parameters() {
            budget.node(formal.span, 0)?;
            let kind = match &formal.kind {
                s::GenericParamKind::Lifetime => LifeParameterKind::Lifetime,
                s::GenericParamKind::Type(_) => LifeParameterKind::Type,
                s::GenericParamKind::Const(carrier) => {
                    budget.ty(carrier)?;
                    LifeParameterKind::Const(type_ref(carrier))
                }
            };
            budget.lineage_push(&mut parameters, kind, formal.span)?;
        }
        let identity = &row.origin.identity;
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
        budget.lineage_push(
            &mut output,
            LifeNominalDeclaration {
                id: zil_ir::LifeDeclarationId {
                    module: row.module,
                    declaration: row.declaration,
                    compiler: false,
                },
                identity: identity.clone(),
                kind,
                parameters,
            },
            row.item.span,
        )?;
    }
    Ok(output)
}
pub(crate) fn pay_declaration(
    value: &zil_ir::LifeNominalDeclaration,
    budget: &mut Budget<'_>,
    span: Span,
) -> Result<(), Diagnostic> {
    budget.node(span, 0)?;
    budget.text(&value.identity.package, span)?;
    budget.lineage_bytes(value.identity.package.len(), span)?;
    for text in value
        .identity
        .module_path
        .iter()
        .chain(&value.identity.declaration_path)
    {
        budget.text(text, span)?;
        budget.lineage_bytes(
            text.len().saturating_add(std::mem::size_of::<String>()),
            span,
        )?;
    }
    budget.identity_work(value.identity.generation_path.len(), span)?;
    budget.lineage_bytes(
        value
            .identity
            .generation_path
            .len()
            .saturating_mul(std::mem::size_of::<u32>()),
        span,
    )?;
    budget.lineage_bytes(
        value
            .parameters
            .len()
            .saturating_mul(std::mem::size_of::<zil_ir::LifeParameterKind>()),
        span,
    )?;
    for parameter in &value.parameters {
        budget.node(span, 0)?;
        if let zil_ir::LifeParameterKind::Const(ty) = parameter {
            copy::ty(ty, budget, span, 0)?;
        }
    }
    Ok(())
}
impl generics::products::Products {
    pub(crate) fn bind_semantic_nominals(
        &mut self,
        module: &s::Module,
        expanded: &mut ExpandedModule,
        limits: &Limits,
    ) -> CompileResult<()> {
        let Some(program) = self.semantic.as_ref() else {
            expanded.semantic_nominal_emissions.clear();
            return Ok(());
        };
        let mut budget = Budget::new(self.allowance.as_ref().unwrap_or(limits));
        let result = (|| {
            let origins = self.origins.as_ref().ok_or_else(|| {
                Diagnostic::error(
                    Phase::Expand,
                    "E-SOURCE-SEMANTICS",
                    Some(module.span),
                    "nominal emission has no actual origin inventory",
                )
            })?;
            for emission in std::mem::take(&mut expanded.semantic_nominal_emissions) {
                budget.node(module.span, 0)?;
                let origin =
                    origins.in_module(module.id, emission.item, &mut budget, module.span)?;
                let (declaration_id, identity) = if origin.compiler {
                    // Generic Option/Poll storage has a separate exact qualified
                    // producer. Closed task enums have actual compiler ASTs too.
                    if origin.role.is_some() {
                        continue;
                    }
                    let item = module.items.get(emission.item).ok_or_else(|| {
                        Diagnostic::error(
                            Phase::Expand,
                            "E-SOURCE-SEMANTICS",
                            Some(module.span),
                            "compiler nominal actual output position is absent",
                        )
                    })?;
                    if !matches!(&item.kind,s::ItemKind::Enum(value) if matches!(value.name.as_str(),generics::options::TASK_CREATE_ERROR|generics::options::TASK_POLL_STATE))
                    {
                        continue;
                    }
                    generics::options::storage_identity(item, origin, &mut budget)?
                } else {
                    let original =
                        origins.identity(origin.declaration, &mut budget, module.span)?;
                    let id = zil_ir::LifeDeclarationId {
                        module: origin.declaration.module,
                        declaration: original.declaration,
                        compiler: false,
                    };
                    budget.text(&original.identity.package, module.span)?;
                    budget.lineage_bytes(original.identity.package.len(), module.span)?;
                    for part in original
                        .identity
                        .module_path
                        .iter()
                        .chain(&original.identity.declaration_path)
                    {
                        budget.text(part, module.span)?;
                        budget.lineage_bytes(
                            part.len().saturating_add(std::mem::size_of::<String>()),
                            module.span,
                        )?;
                    }
                    budget.lineage_bytes(
                        original
                            .identity
                            .generation_path
                            .len()
                            .saturating_mul(std::mem::size_of::<u32>()),
                        module.span,
                    )?;
                    (id, original.identity.clone())
                };
                budget.identity_work(program.nominal_declarations.len(), module.span)?;
                let declaration = program
                    .nominal_declarations
                    .iter()
                    .find(|row| row.id == declaration_id)
                    .ok_or_else(|| {
                        Diagnostic::error(
                            Phase::Expand,
                            "E-SOURCE-SEMANTICS",
                            Some(module.span),
                            "actual nominal mint has no original declaration",
                        )
                    })?;
                if declaration.identity != identity {
                    return Err(Diagnostic::error(
                        Phase::Expand,
                        "E-SOURCE-SEMANTICS",
                        Some(module.span),
                        "actual nominal mint original identity differs",
                    ));
                }
                // Locate the exact emitted declaration before retaining a
                // second copy of its physical fields. Symbolic arguments do
                // not become complete merely because they have a layout name.
                let (actual_name, actual_span) = if emission.enumeration {
                    budget.identity_work(expanded.core.enums.len(), module.span)?;
                    let value = expanded
                        .core
                        .enums
                        .iter()
                        .find(|row| row.id == emission.node)
                        .ok_or_else(|| {
                            Diagnostic::error(
                                Phase::Expand,
                                "E-SOURCE-SEMANTICS",
                                Some(module.span),
                                "actual enum mint is absent",
                            )
                        })?;
                    (&value.name, value.span)
                } else {
                    budget.identity_work(expanded.core.aggregates.len(), module.span)?;
                    let value = expanded
                        .core
                        .aggregates
                        .iter()
                        .find(|row| row.id == emission.node)
                        .ok_or_else(|| {
                            Diagnostic::error(
                                Phase::Expand,
                                "E-SOURCE-SEMANTICS",
                                Some(module.span),
                                "actual aggregate mint is absent",
                            )
                        })?;
                    (&value.name, value.span)
                };
                budget.identity_work(self.storage.instances.len(), actual_span)?;
                if self.storage.instances.iter().any(|row| row.declaration.id == declaration.id && matches!(&row.instance.ty, Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Record{name: n,..}|Type::Package{name:n,..}|Type::Enum{name:n,..} if n == actual_name))) { continue; }
                let mut selected = None;
                if !declaration.parameters.is_empty() {
                    for application in &self.nominal_applications {
                        budget.node(actual_span, 0)?;
                        budget.text(&application.generated, actual_span)?;
                        budget.text(actual_name, actual_span)?;
                        if application.origin == origin && &application.generated == actual_name {
                            if selected.replace(application).is_some() {
                                return Err(Diagnostic::error(
                                    Phase::Expand,
                                    "E-SOURCE-SEMANTICS",
                                    Some(actual_span),
                                    "nominal mint has multiple actual application ledgers",
                                ));
                            }
                        }
                    }
                    let application = selected.ok_or_else(|| {
                        Diagnostic::unsupported(
                            Phase::Expand,
                            "actual nominal application lost its complete original arguments",
                            Some(actual_span),
                        )
                    })?;
                    if let ApplicationArguments::Deferred(dependencies) = &application.arguments {
                        if dependencies.is_empty() || application.owner.is_none() {
                            return Err(Diagnostic::error(
                                Phase::Expand,
                                "E-SOURCE-SEMANTICS",
                                Some(actual_span),
                                "deferred nominal application lost its original lexical owner",
                            ));
                        }
                        budget.identity_work(dependencies.len(), actual_span)?;
                        for dependency in dependencies {
                            if dependency.argument as usize >= declaration.parameters.len() {
                                return Err(Diagnostic::error(
                                    Phase::Expand,
                                    "E-SOURCE-SEMANTICS",
                                    Some(actual_span),
                                    "deferred nominal argument exceeds the original mixed formals",
                                ));
                            }
                        }
                        // Original nominal/body/site recipes and the actual
                        // Core aggregate remain available to the independently
                        // checked helper context. No ownerless Named(F) row is
                        // advertised as a complete instance or LayoutReady.
                        continue;
                    }
                }
                let (name, ty, span) = if emission.enumeration {
                    budget.identity_work(expanded.core.enums.len(), module.span)?;
                    let value = expanded
                        .core
                        .enums
                        .iter()
                        .find(|row| row.id == emission.node)
                        .ok_or_else(|| {
                            Diagnostic::error(
                                Phase::Expand,
                                "E-SOURCE-SEMANTICS",
                                Some(module.span),
                                "actual enum mint is absent",
                            )
                        })?;
                    budget.text(&value.name, value.span)?;
                    budget.lineage_bytes(value.name.len(), value.span)?;
                    for (name, fields) in &value.variants {
                        budget.text(name, value.span)?;
                        budget.lineage_bytes(
                            name.len()
                                .saturating_add(std::mem::size_of::<(String, Vec<Type>)>()),
                            value.span,
                        )?;
                        budget.lineage_bytes(
                            fields
                                .len()
                                .checked_mul(std::mem::size_of::<Type>())
                                .ok_or_else(|| {
                                    Diagnostic::limit(
                                        Phase::Expand,
                                        "nominal enum clone allocation overflow",
                                        Some(value.span),
                                    )
                                })?,
                            value.span,
                        )?;
                        for field in fields {
                            copy::ty(field, &mut budget, value.span, 0)?;
                        }
                    }
                    let ty = Type::Enum {
                        name: value.name.clone(),
                        variants: value.variants.clone(),
                    };
                    (&value.name, ty, value.span)
                } else {
                    budget.identity_work(expanded.core.aggregates.len(), module.span)?;
                    let value = expanded
                        .core
                        .aggregates
                        .iter()
                        .find(|row| row.id == emission.node)
                        .ok_or_else(|| {
                            Diagnostic::error(
                                Phase::Expand,
                                "E-SOURCE-SEMANTICS",
                                Some(module.span),
                                "actual aggregate mint is absent",
                            )
                        })?;
                    let mut fields = Vec::new();
                    for field in &value.fields {
                        budget.text(&field.name, field.span)?;
                        copy::ty(&field.ty, &mut budget, field.span, 0)?;
                        budget.lineage_bytes(field.name.len(), field.span)?;
                        let ty = if field.manages {
                            // The field's owning annotation selects the real
                            // resource payload. Sealed Store/ByteBuffer names
                            // enter Core through their default description;
                            // that outer observation mode is not a second
                            // nested payload of the owning field.
                            match &field.ty {
                                Type::Managed(_) => field.ty.clone(),
                                Type::Desc(payload) => Type::Managed(payload.clone()),
                                _ => {
                                    budget
                                        .lineage_bytes(std::mem::size_of::<Type>(), field.span)?;
                                    Type::Managed(Box::new(field.ty.clone()))
                                }
                            }
                        } else {
                            field.ty.clone()
                        };
                        budget.lineage_push(&mut fields, (field.name.clone(), ty), field.span)?;
                    }
                    budget.text(&value.name, value.span)?;
                    budget.lineage_bytes(value.name.len(), value.span)?;
                    let ty = if value.kind == AggregateKind::Package {
                        Type::Package {
                            name: value.name.clone(),
                            fields,
                        }
                    } else {
                        Type::Record {
                            name: value.name.clone(),
                            fields,
                        }
                    };
                    (&value.name, ty, value.span)
                };
                let mut arguments = Vec::new();
                if let Some(application) = selected {
                    let Some(actuals) = application.arguments.complete() else {
                        unreachable!("deferred applications do not publish complete instances")
                    };
                    for actual in actuals {
                        match actual {
                            zil_ir::LifeNominalActual::Type(ty) => {
                                copy::ty(ty, &mut budget, span, 0)?
                            }
                            _ => budget.node(span, 0)?,
                        }
                        budget.lineage_push(&mut arguments, actual.clone(), span)?;
                    }
                    if arguments.len() != declaration.parameters.len() {
                        return Err(Diagnostic::error(
                            Phase::Expand,
                            "E-SOURCE-SEMANTICS",
                            Some(span),
                            "nominal application omits original mixed arguments",
                        ));
                    }
                }
                pay_declaration(declaration, &mut budget, span)?;
                let candidate = crate::stored_types::Instance {
                    declaration: declaration.clone(),
                    instance: zil_ir::LifeNominalInstance {
                        declaration: declaration.id,
                        ty,
                        arguments,
                    },
                    classes: Vec::new(),
                };
                crate::stored_types::pay_instance(&candidate, &mut budget, span)?;
                if let Some(prior) = expanded
                    .core
                    .life_declarations
                    .iter()
                    .find(|row| row.id == declaration.id)
                {
                    if prior != declaration {
                        return Err(Diagnostic::error(
                            Phase::Expand,
                            "E-SOURCE-SEMANTICS",
                            Some(span),
                            "nominal declaration inventory conflicts",
                        ));
                    }
                } else if !declaration.id.compiler {
                    budget.lineage_push(
                        &mut expanded.core.life_declarations,
                        candidate.declaration,
                        span,
                    )?;
                }
                budget.lineage_push(&mut expanded.core.life_instances, candidate.instance, span)?;
            }
            Ok(())
        })();
        self.allowance = Some(budget.remaining_limits());
        result.map_err(|e| vec![e])
    }
}
