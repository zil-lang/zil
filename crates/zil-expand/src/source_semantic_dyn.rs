//! Original Dyn recipe proposals and exact actual-emission links. This bridge
//! does not select a provider, prove a premise, infer a stored domain, or grant
//! a receiver loan. Consumers recheck the original scopes and actual bodies.
use super::*;

pub(super) struct Template {
    declaration: LifeDeclarationId,
    scope: SemanticDefinitionId,
    identity: LifeDeclarationIdentity,
    self_type: Option<DynTypePattern>,
    trait_application: Option<DynSelfPattern>,
    supers: Option<Vec<DynSelfPattern>>,
    requirements: Option<Vec<DynObligation>>,
    associated: Option<Vec<DynAssociatedDefinition>>,
    signatures: Option<Vec<DynMethodRecipe>>,
    definitions: Option<Vec<DynDefinitionDeclaration>>,
    method_scopes: Vec<(DynDefinitionId, SemanticDefinitionId)>,
    signature_scopes: Vec<(u32, SemanticDefinitionId)>,
}
fn find<'a>(
    modules: &'a [(ModuleId, SemanticModule)],
    id: SemanticDefinitionId,
    budget: &mut Budget<'_>,
) -> R<&'a SemanticDefinition> {
    let mut found = None;
    for (_, module) in modules {
        budget.identity_work(module.definitions.len(), Span::default())?;
        for definition in &module.definitions {
            if definition.id == id && found.replace(definition).is_some() {
                return Err(error(
                    Span::default(),
                    "original Dyn scope identity is duplicated",
                ));
            }
        }
    }
    found.ok_or_else(|| error(Span::default(), "original Dyn scope is absent"))
}
fn copy<T: DynRecipeMetadata + Clone>(value: &T, budget: &mut Budget<'_>, span: Span) -> R<T> {
    prepay_recipe(value, budget, span, 0)?;
    Ok(value.clone())
}
struct Identity<'a>(&'a LifeDeclarationIdentity);
impl DynRecipeMetadata for Identity<'_> {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.identity(self.0, depth)
    }
}
fn identity(
    value: &LifeDeclarationIdentity,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<LifeDeclarationIdentity> {
    prepay_recipe(&Identity(value), budget, span, 0)?;
    Ok(value.clone())
}
fn rows<T: DynRecipeMetadata + Clone>(
    values: &[T],
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Vec<T>> {
    let mut output = Vec::new();
    for value in values {
        let value = copy(value, budget, span)?;
        budget.lineage_push(&mut output, value, span)?;
    }
    Ok(output)
}
fn pattern(
    scope: SemanticDefinitionId,
    ty: &SemanticTypeRecipe,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<DynTypePattern> {
    let ty = copy(ty, budget, span)?;
    budget.lineage_bytes(std::mem::size_of::<SemanticTypeRecipe>(), span)?;
    Ok(DynTypePattern::Semantic {
        scope,
        ty: Box::new(ty),
    })
}
fn life(value: &SemanticLifeRecipe, method: bool) -> Option<DynLifePattern> {
    Some(match value {
        SemanticLifeRecipe::Formal(reference) => match reference.scope {
            SemanticScopeRef::Ancestor(0) if method => {
                DynLifePattern::MethodParameter(reference.ordinal)
            }
            SemanticScopeRef::Ancestor(0) => {
                DynLifePattern::DeclarationParameter(reference.ordinal)
            }
            SemanticScopeRef::Ancestor(1) if method => {
                DynLifePattern::DeclarationParameter(reference.ordinal)
            }
            _ => return None,
        },
        SemanticLifeRecipe::Bound { depth, index } => DynLifePattern::Bound {
            depth: *depth,
            index: *index,
        },
        SemanticLifeRecipe::Invocation(index) => DynLifePattern::Invocation(*index),
        // Semantic implicit occurrences remain in their exact Semantic scope.
        // They are not renamed into Dyn's unrelated anonymous namespace.
        _ => return None,
    })
}
fn constant(
    scope: SemanticDefinitionId,
    value: &SemanticConstRecipe,
    method: bool,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> R<Option<DynConstPattern>> {
    budget.node(span, depth)?;
    Ok(Some(match value {
        SemanticConstRecipe::Value(constant) => {
            prepay_recipe(value, budget, span, depth)?;
            DynConstPattern::Value(constant.clone())
        }
        SemanticConstRecipe::IntegerLiteral(text) => {
            budget.text(text, span)?;
            budget.lineage_bytes(text.len(), span)?;
            DynConstPattern::IntegerLiteral(text.clone())
        }
        SemanticConstRecipe::FloatLiteral(text) => {
            budget.text(text, span)?;
            budget.lineage_bytes(text.len(), span)?;
            DynConstPattern::FloatLiteral(text.clone())
        }
        SemanticConstRecipe::Formal(reference) => match reference.scope {
            SemanticScopeRef::Ancestor(0) if method => {
                DynConstPattern::MethodParameter(reference.ordinal)
            }
            SemanticScopeRef::Ancestor(0) => DynConstPattern::Parameter(reference.ordinal),
            SemanticScopeRef::Ancestor(1) if method => {
                DynConstPattern::Parameter(reference.ordinal)
            }
            _ => return Ok(None),
        },
        SemanticConstRecipe::Unary {
            op,
            operand_type,
            value,
        } => {
            let Some(value) = constant(scope, value, method, budget, span, depth + 1)? else {
                return Ok(None);
            };
            budget.lineage_bytes(
                std::mem::size_of::<DynConstPattern>() + std::mem::size_of::<DynTypePattern>(),
                span,
            )?;
            DynConstPattern::Unary {
                op: *op,
                value: Box::new(value),
                ty: Box::new(pattern(scope, operand_type, budget, span)?),
            }
        }
        SemanticConstRecipe::Binary {
            op,
            operand_type,
            left,
            right,
        } => {
            let Some(left) = constant(scope, left, method, budget, span, depth + 1)? else {
                return Ok(None);
            };
            let Some(right) = constant(scope, right, method, budget, span, depth + 1)? else {
                return Ok(None);
            };
            budget.lineage_bytes(
                2 * std::mem::size_of::<DynConstPattern>() + std::mem::size_of::<DynTypePattern>(),
                span,
            )?;
            DynConstPattern::Binary {
                op: *op,
                left: Box::new(left),
                right: Box::new(right),
                ty: Box::new(pattern(scope, operand_type, budget, span)?),
            }
        }
        SemanticConstRecipe::Convert {
            target_type, value, ..
        } => {
            let Some(value) = constant(scope, value, method, budget, span, depth + 1)? else {
                return Ok(None);
            };
            budget.lineage_bytes(
                std::mem::size_of::<DynConstPattern>() + std::mem::size_of::<DynTypePattern>(),
                span,
            )?;
            DynConstPattern::Convert {
                value: Box::new(value),
                ty: Box::new(pattern(scope, target_type, budget, span)?),
            }
        }
        SemanticConstRecipe::AssociatedConst {
            receiver,
            application,
            member,
        } => {
            let Some(application) =
                application_pattern(scope, application, method, budget, span, depth + 1)?
            else {
                return Ok(None);
            };
            budget.lineage_bytes(
                std::mem::size_of::<DynSelfPattern>() + std::mem::size_of::<DynTypePattern>(),
                span,
            )?;
            DynConstPattern::AssociatedProjection {
                receiver: Box::new(pattern(scope, receiver, budget, span)?),
                application: Box::new(application),
                member: *member,
            }
        }
        SemanticConstRecipe::Environment { .. } => return Ok(None),
    }))
}
fn application_pattern(
    scope: SemanticDefinitionId,
    value: &SemanticTraitRecipe,
    method: bool,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> R<Option<DynSelfPattern>> {
    budget.node(span, depth)?;
    let mut arguments = Vec::new();
    for argument in &value.arguments {
        let value = match argument {
            SemanticActualRecipe::Type(ty) => {
                DynPatternActual::Type(pattern(scope, ty, budget, span)?)
            }
            SemanticActualRecipe::Const { value, .. } => {
                let Some(value) = constant(scope, value, method, budget, span, depth + 1)? else {
                    return Ok(None);
                };
                DynPatternActual::Const(value)
            }
            SemanticActualRecipe::Life(value) => {
                let Some(value) = life(value, method) else {
                    return Ok(None);
                };
                DynPatternActual::Life(value)
            }
            SemanticActualRecipe::Infer => return Ok(None),
        };
        budget.lineage_push(&mut arguments, value, span)?;
    }
    Ok(Some(DynSelfPattern {
        declaration: value.declaration,
        arguments,
    }))
}
fn requirements(
    scope: SemanticDefinitionId,
    values: &[SemanticRequirementRecipe],
    method: bool,
    budget: &mut Budget<'_>,
    span: Span,
    depth: usize,
) -> R<Option<Vec<DynObligation>>> {
    let mut output = Vec::new();
    for value in values {
        budget.node(span, depth)?;
        let value = match value {
            SemanticRequirementRecipe::Structural { ty, bound } => DynObligation::Structural {
                receiver: pattern(scope, ty, budget, span)?,
                bound: *bound,
            },
            SemanticRequirementRecipe::Builtin { ty, trait_kind } => {
                let bound = match trait_kind {
                    SemanticBuiltinTrait::Copy => DynStructuralBound::Copy,
                    SemanticBuiltinTrait::Value => DynStructuralBound::Value,
                    SemanticBuiltinTrait::SyncDrop => DynStructuralBound::SyncDrop,
                    SemanticBuiltinTrait::StoreElement => DynStructuralBound::StoreElement,
                    _ => return Ok(None),
                };
                DynObligation::Structural {
                    receiver: pattern(scope, ty, budget, span)?,
                    bound,
                }
            }
            SemanticRequirementRecipe::Trait { ty, application } => {
                let Some(application) =
                    application_pattern(scope, application, method, budget, span, depth + 1)?
                else {
                    return Ok(None);
                };
                DynObligation::Trait {
                    receiver: pattern(scope, ty, budget, span)?,
                    application,
                }
            }
            SemanticRequirementRecipe::Callable {
                ty,
                kind,
                signature,
            } => {
                let quantified_lifetimes = signature.bound_lifetimes;
                let signature = signature_recipe(scope, signature, None, budget, span)?;
                budget.lineage_bytes(std::mem::size_of::<DynSignatureRecipe>(), span)?;
                DynObligation::Callable {
                    receiver: pattern(scope, ty, budget, span)?,
                    kind: *kind,
                    quantified_lifetimes,
                    signature: Box::new(signature),
                }
            }
            SemanticRequirementRecipe::TypeEqual { left, right } => DynObligation::TypeEqual {
                left: pattern(scope, left, budget, span)?,
                right: pattern(scope, right, budget, span)?,
            },
            SemanticRequirementRecipe::ConstEqual {
                carrier,
                left,
                right,
            } => {
                let Some(left) = constant(scope, left, method, budget, span, depth + 1)? else {
                    return Ok(None);
                };
                let Some(right) = constant(scope, right, method, budget, span, depth + 1)? else {
                    return Ok(None);
                };
                DynObligation::ConstEqual {
                    ty: pattern(scope, carrier, budget, span)?,
                    left,
                    right,
                }
            }
            // These complete original premises remain in the Semantic scope.
            // Dyn's narrower obligation language has no equivalent row, so a
            // full Dyn recipe must remain Unknown rather than dropping them.
            SemanticRequirementRecipe::LifeEqual { .. }
            | SemanticRequirementRecipe::Outlives { .. }
            | SemanticRequirementRecipe::ForAll { .. } => return Ok(None),
        };
        budget.lineage_push(&mut output, value, span)?;
    }
    Ok(Some(output))
}
fn complete_requirements(
    definition: &SemanticDefinition,
    method: bool,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Option<Vec<DynObligation>>> {
    let Some(mut output) = requirements(
        definition.id,
        &definition.requirements,
        method,
        budget,
        span,
        0,
    )?
    else {
        return Ok(None);
    };
    for formal in &definition.parameters {
        if let SemanticFormal::Type {
            requirements: values,
        } = formal
        {
            let Some(values) = requirements(definition.id, values, method, budget, span, 0)? else {
                return Ok(None);
            };
            for value in values {
                budget.lineage_push(&mut output, value, span)?
            }
        }
    }
    Ok(Some(output))
}
fn position(index: u32) -> StoragePosition {
    StoragePosition {
        base: StorageBase::Parameter(index),
        path: Vec::new(),
    }
}
fn signature_recipe(
    scope: SemanticDefinitionId,
    signature: &SemanticSignatureRecipe,
    source: Option<&s::Function>,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<DynSignatureRecipe> {
    let mut parameters = Vec::new();
    let mut requires = Vec::new();
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    for (index, parameter) in signature.parameters.iter().enumerate() {
        let index = u32::try_from(index)
            .map_err(|_| error(span, "Dyn original parameter ordinal overflow"))?;
        let is_address = matches!(parameter.ty, SemanticTypeRecipe::Address { .. });
        let mutable = matches!(
            parameter.ty,
            SemanticTypeRecipe::Address { mutable: true, .. }
        );
        if is_address {
            budget.lineage_push(
                &mut requires,
                DynRequirementRecipe::Read(position(index)),
                span,
            )?;
            budget.lineage_push(&mut reads, position(index), span)?;
        }
        if mutable {
            budget.lineage_push(
                &mut requires,
                DynRequirementRecipe::Write(position(index)),
                span,
            )?;
            budget.lineage_push(&mut writes, position(index), span)?;
        }
        let ty = pattern(scope, &parameter.ty, budget, span)?;
        budget.lineage_push(
            &mut parameters,
            DynParameterRecipe {
                ty,
                mode: parameter.mode,
            },
            span,
        )?;
    }
    let mut complete = true;
    let mut effects = Vec::new();
    let mut explicit_effects = false;
    let mut resume = DynResumeBound::Local;
    if let Some(function) = source {
        for clause in &function.contract {
            budget.node(clause.span, 0)?;
            match clause.name.as_str() {
                "effects" => {
                    explicit_effects = true;
                    for term in &clause.terms {
                        budget.node(term.span, 0)?;
                        let effect = match term.path.as_slice() {
                            [name]
                                if term.arguments.is_empty()
                                    && term.type_args.is_empty()
                                    && term.index.is_none() =>
                            {
                                match name.as_str() {
                                    "alloc" => Some(DynEffectRecipe::Alloc),
                                    "io" => Some(DynEffectRecipe::Io),
                                    "block" => Some(DynEffectRecipe::Block),
                                    "suspend" => Some(DynEffectRecipe::Suspend),
                                    "spawn" => Some(DynEffectRecipe::Spawn),
                                    "ffi" => Some(DynEffectRecipe::Ffi),
                                    "reenter" => Some(DynEffectRecipe::Reenter),
                                    "panic" => Some(DynEffectRecipe::Panic),
                                    _ => None,
                                }
                            }
                            _ => None,
                        };
                        if let Some(effect) = effect {
                            budget.lineage_push(&mut effects, effect, term.span)?
                        } else {
                            complete = false;
                        }
                    }
                }
                "resume" => {
                    if matches!(clause.terms.as_slice(),[term]if matches!(term.path.as_slice(),[name]if name=="transfer"))
                    {
                        resume = DynResumeBound::Transfer;
                    }
                }
                "result"
                    if matches!(
                        signature.result_mode,
                        PassingMode::Create | PassingMode::Give
                    ) => {}
                _ => complete = false,
            }
        }
    }
    if !explicit_effects {
        for effect in [
            DynEffectRecipe::Alloc,
            DynEffectRecipe::Io,
            DynEffectRecipe::Block,
            DynEffectRecipe::Ffi,
            DynEffectRecipe::Reenter,
            DynEffectRecipe::Panic,
        ] {
            budget.lineage_push(&mut effects, effect, span)?;
        }
        if signature.call_kind == CallKind::Suspend {
            budget.lineage_push(&mut effects, DynEffectRecipe::Suspend, span)?;
        }
    }
    let result = match &signature.result {
        SemanticTypeRecipe::Address {
            mutable, lifetime, ..
        } => {
            let mode = if *mutable {
                StoredBorrowMode::ExclusiveAddress
            } else {
                StoredBorrowMode::SharedAddress
            };
            let mut sources = Vec::new();
            let leaf = pattern(scope, &signature.result, budget, span)?;
            budget.lineage_push(
                &mut sources,
                DynStorageSourceRecipe::Static { leaf, mode },
                span,
            )?;
            for (index, parameter) in signature.parameters.iter().enumerate() {
                if let SemanticTypeRecipe::Address {
                    lifetime: input, ..
                } = &parameter.ty
                {
                    prepay_recipe_work(input, budget, span, 0)?;
                    prepay_recipe_work(lifetime, budget, span, 0)?;
                    if input == lifetime {
                        let index = u32::try_from(index)
                            .map_err(|_| error(span, "Dyn source ordinal overflow"))?;
                        if parameter.ty == signature.result {
                            budget.lineage_push(
                                &mut sources,
                                DynStorageSourceRecipe::Exact(position(index)),
                                span,
                            )?;
                        }
                        if index == 0 {
                            let leaf = pattern(scope, &signature.result, budget, span)?;
                            budget.lineage_push(
                                &mut sources,
                                DynStorageSourceRecipe::OwnedView {
                                    head: position(0),
                                    leaf,
                                    mode,
                                },
                                span,
                            )?;
                        }
                    }
                }
            }
            DynResultSourceRecipe::Borrow {
                sources,
                mutable: *mutable,
            }
        }
        _ => match signature.result_mode {
            PassingMode::Create => DynResultSourceRecipe::Create,
            PassingMode::Give => DynResultSourceRecipe::Give,
            _ => DynResultSourceRecipe::Value,
        },
    };
    Ok(DynSignatureRecipe {
        parameters,
        result: pattern(scope, &signature.result, budget, span)?,
        result_mode: signature.result_mode,
        call_kind: signature.call_kind,
        invocation_binder_count: signature.invocation_lifetimes,
        contract: DynContractRecipe {
            requires: complete.then_some(requires),
            guarantees: complete.then(Vec::new),
            hold: complete.then(Vec::new),
            call: complete.then_some(DynCallAccess { reads, writes }),
            result: complete.then_some(result),
            cleanup: complete.then_some(DynCleanupRecipe {
                environment: false,
                positions: Vec::new(),
                access: DynCallAccess {
                    reads: Vec::new(),
                    writes: Vec::new(),
                },
                effects: Vec::new(),
            }),
            effects: complete.then_some(effects),
            resume: complete.then_some(resume),
            threads: complete.then_some(DynThreadRequirements {
                transfer: false,
                share: false,
            }),
        },
    })
}

fn self_application(
    scope: &SemanticDefinition,
    budget: &mut Budget<'_>,
    span: Span,
) -> R<Option<SemanticTraitRecipe>> {
    if let SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
        trait_application, ..
    }) = &scope.body
    {
        return Ok(Some(copy(trait_application, budget, span)?));
    }
    let SemanticDefinitionBody::Scope(SemanticScopeRecipe::Trait { declaration }) = &scope.body
    else {
        return Ok(None);
    };
    let mut arguments = Vec::new();
    for (ordinal, formal) in scope.parameters.iter().enumerate() {
        let ordinal = u32::try_from(ordinal)
            .map_err(|_| error(span, "Dyn associated formal ordinal overflow"))?;
        let reference = SemanticFormalRef {
            scope: SemanticScopeRef::Ancestor(0),
            ordinal,
        };
        let argument = match formal {
            SemanticFormal::Lifetime => {
                SemanticActualRecipe::Life(SemanticLifeRecipe::Formal(reference))
            }
            SemanticFormal::Type { .. } => {
                SemanticActualRecipe::Type(SemanticTypeRecipe::Formal(reference))
            }
            SemanticFormal::Const { carrier } => SemanticActualRecipe::Const {
                carrier: copy(carrier, budget, span)?,
                value: SemanticConstRecipe::Formal(reference),
            },
        };
        budget.lineage_push(&mut arguments, argument, span)?;
    }
    Ok(Some(SemanticTraitRecipe {
        declaration: *declaration,
        arguments,
    }))
}
fn associated(
    builder: &mut Builder<'_, '_, '_, '_>,
    scope: &SemanticDefinition,
    members: &[s::Item],
) -> R<Option<Vec<DynAssociatedDefinition>>> {
    let mut output = Vec::new();
    for (index, item) in members.iter().enumerate() {
        let member = u32::try_from(index)
            .map_err(|_| error(item.span, "Dyn associated member ordinal overflow"))?;
        let before = builder.implicit.len();
        let value = match &item.kind {
            s::ItemKind::TypeAlias(alias) => {
                if !alias.generics.is_empty() {
                    return Ok(None);
                }
                let Some(application) = self_application(scope, builder.budget, item.span)? else {
                    return Ok(None);
                };
                let receiver = SemanticTypeRecipe::SelfType {
                    scope: SemanticScopeRef::Ancestor(0),
                };
                let mut constraints = Vec::new();
                for (ordinal, bound) in alias.bounds.iter().enumerate() {
                    let subject = SemanticTypeRecipe::AssociatedType {
                        receiver: Box::new(copy(&receiver, builder.budget, item.span)?),
                        application: Box::new(copy(&application, builder.budget, item.span)?),
                        member,
                    };
                    builder.budget.lineage_bytes(
                        std::mem::size_of::<SemanticTypeRecipe>()
                            + std::mem::size_of::<SemanticTraitRecipe>(),
                        item.span,
                    )?;
                    let value = builder.requirement(
                        subject,
                        bound,
                        &mut vec![37, member, 1, ordinal as u32],
                        0,
                    )?;
                    builder.push(&mut constraints, value, item.span)?;
                }
                let Some(requirements) =
                    requirements(scope.id, &constraints, false, builder.budget, item.span, 0)?
                else {
                    return Ok(None);
                };
                let value = if let Some(ty) = &alias.value {
                    let ty = builder.ty(ty, &mut vec![37, member, 0], 0, false)?;
                    Some(DynAssociatedValueRecipe::Type(pattern(
                        scope.id,
                        &ty,
                        builder.budget,
                        item.span,
                    )?))
                } else {
                    None
                };
                DynAssociatedDefinition {
                    carrier: None,
                    member,
                    value,
                    requirements,
                }
            }
            s::ItemKind::Const(constant_decl) => {
                let Some(ty) = &constant_decl.ty else {
                    return Ok(None);
                };
                let ty = builder.ty(ty, &mut vec![37, member, 0], 0, false)?;
                let value = if let Some(value) = &constant_decl.value {
                    let value = builder.constant(value, &ty, &mut vec![37, member, 1], 0)?;
                    let Some(value) =
                        constant(scope.id, &value, false, builder.budget, item.span, 0)?
                    else {
                        return Ok(None);
                    };
                    Some(DynAssociatedValueRecipe::Constant {
                        ty: pattern(scope.id, &ty, builder.budget, item.span)?,
                        value,
                    })
                } else {
                    None
                };
                DynAssociatedDefinition {
                    carrier: Some(pattern(scope.id, &ty, builder.budget, item.span)?),
                    member,
                    value,
                    requirements: Vec::new(),
                }
            }
            _ => continue,
        };
        if builder.implicit.len() != before {
            // Publishing a new anonymous binder would also need its exact
            // original Scope occurrence table. Never invent a Dyn ordinal.
            return Ok(None);
        }
        builder.push(&mut output, value, item.span)?;
    }
    Ok(Some(output))
}

fn has_original_dyn(
    inventory: &SourceDeclarationInventory<'_>,
    budget: &mut Budget<'_>,
) -> R<bool> {
    struct Scan<'a, 'l> {
        budget: &'a mut Budget<'l>,
        found: bool,
    }
    impl crate::ct_selection::Visitor for Scan<'_, '_> {
        fn node(&mut self, span: Span, depth: usize) -> CompileResult<()> {
            self.budget.node(span, depth).map_err(|error| vec![error])
        }
        fn text(&mut self, text: &str, span: Span) -> CompileResult<()> {
            self.budget.text(text, span).map_err(|error| vec![error])
        }
        fn ty(&mut self, ty: &s::TypeRef, depth: usize) -> CompileResult<()> {
            if matches!(ty.kind, s::TypeKind::Dyn { .. }) {
                self.found = true;
            }
            crate::ct_selection::walk_type(self, ty, depth)
        }
    }
    let mut scan = Scan {
        budget,
        found: false,
    };
    for row in &inventory.declarations {
        if row.parent.is_none() {
            crate::ct_selection::walk_item(&mut scan, &row.item, 0)
                .map_err(|mut errors| errors.remove(0))?;
        }
    }
    Ok(scan.found)
}

pub(super) fn collect(
    builder: &mut Builder<'_, '_, '_, '_>,
    modules: &[(ModuleId, SemanticModule)],
) -> R<Vec<Template>> {
    let inventory = builder.inventory;
    let mut output = Vec::new();
    if !has_original_dyn(inventory, builder.budget)? {
        return Ok(output);
    }
    for (owner, row) in inventory.declarations.iter().enumerate() {
        let members = match &row.item.kind {
            s::ItemKind::Trait(value) => &value.members,
            s::ItemKind::Impl(value) if value.trait_type.is_some() => {
                builder.owner = owner;
                if builder
                    .builtin_trait(value.trait_type.as_ref().expect("guarded trait"))?
                    .is_some()
                {
                    continue;
                }
                &value.members
            }
            _ => continue,
        };
        builder.owner = owner;
        builder.lexical_offset = 0;
        let scope_id = builder.id(owner);
        let scope = find(modules, scope_id, builder.budget)?;
        let declaration = match &scope.body {
            SemanticDefinitionBody::Scope(
                SemanticScopeRecipe::Trait { declaration }
                | SemanticScopeRecipe::TraitImpl { declaration, .. },
            ) => *declaration,
            _ => {
                return Err(error(
                    row.item.span,
                    "Dyn original owner is not its actual trait/impl scope",
                ));
            }
        };
        let (self_type, trait_application) = match &scope.body {
            SemanticDefinitionBody::Scope(SemanticScopeRecipe::TraitImpl {
                self_type,
                trait_application,
                ..
            }) => (
                Some(pattern(scope_id, self_type, builder.budget, row.item.span)?),
                application_pattern(
                    scope_id,
                    trait_application,
                    false,
                    builder.budget,
                    row.item.span,
                    0,
                )?,
            ),
            _ => (None, None),
        };
        let mut supers = Some(Vec::new());
        if matches!(row.item.kind, s::ItemKind::Trait(_)) {
            for requirement in &scope.requirements {
                if let SemanticRequirementRecipe::Trait {
                    ty:
                        SemanticTypeRecipe::SelfType {
                            scope: SemanticScopeRef::Ancestor(0),
                        },
                    application,
                } = requirement
                {
                    if let Some(value) = application_pattern(
                        scope_id,
                        application,
                        false,
                        builder.budget,
                        row.item.span,
                        0,
                    )? {
                        if let Some(supers) = &mut supers {
                            builder.push(supers, value, row.item.span)?
                        }
                    } else {
                        supers = None;
                    }
                }
            }
        }
        let scope_requirements =
            complete_requirements(scope, false, builder.budget, row.item.span)?;
        let mut method_signatures = Some(Vec::new());
        let mut method_definitions = Some(Vec::new());
        let mut method_scopes = Vec::new();
        let mut signature_scopes = Vec::new();
        builder.implicit.clear();
        let associated = associated(builder, scope, members)?;
        for (ordinal, item) in members.iter().enumerate() {
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| error(item.span, "Dyn original member ordinal overflow"))?;
            match &item.kind {
                s::ItemKind::Function(function) => {
                    builder
                        .budget
                        .identity_work(inventory.declarations.len(), item.span)?;
                    let member = inventory
                        .declarations
                        .iter()
                        .enumerate()
                        .find(|(_, child)| {
                            child.parent == Some(owner) && child.path.last() == Some(&ordinal)
                        })
                        .map(|(index, _)| index)
                        .ok_or_else(|| {
                            error(
                                item.span,
                                "Dyn member has no actual SourceIndex declaration",
                            )
                        })?;
                    let method = find(modules, builder.id(member), builder.budget)?;
                    let SemanticDefinitionBody::FunctionWithBody {
                        signature, body, ..
                    } = &method.body
                    else {
                        return Err(error(
                            item.span,
                            "Dyn original method has no complete authored body inventory",
                        ));
                    };
                    let requirements =
                        complete_requirements(method, true, builder.budget, item.span)?;
                    let id = DynDefinitionId {
                        module: method.id.module,
                        source: zil_base::NodeId(method.id.index),
                    };
                    let defined = !matches!(body, SemanticOriginalFunctionBody::Declaration);
                    if matches!(row.item.kind, s::ItemKind::Trait(_)) {
                        builder.push(&mut signature_scopes, (ordinal, method.id), item.span)?;
                        if let Some(requirements) = &requirements {
                            let requirements = rows(requirements, builder.budget, item.span)?;
                            let signature = signature_recipe(
                                method.id,
                                signature,
                                Some(function),
                                builder.budget,
                                item.span,
                            )?;
                            if let Some(rows) = &mut method_signatures {
                                builder.push(
                                    rows,
                                    DynMethodRecipe {
                                        requirements,
                                        member: ordinal,
                                        signature,
                                        default_definition: defined.then_some(id),
                                    },
                                    item.span,
                                )?;
                            }
                        } else {
                            method_signatures = None;
                        }
                    }
                    if defined {
                        if let Some(requirements) = requirements {
                            let identity = identity(&method.identity, builder.budget, item.span)?;
                            let signature = signature_recipe(
                                method.id,
                                signature,
                                Some(function),
                                builder.budget,
                                item.span,
                            )?;
                            if let Some(rows) = &mut method_definitions {
                                builder.push(
                                    rows,
                                    DynDefinitionDeclaration {
                                        requirements,
                                        instances: Vec::new(),
                                        id,
                                        identity,
                                        member: ordinal,
                                        scope: if matches!(row.item.kind, s::ItemKind::Trait(_)) {
                                            DynDefinitionScope::TraitDefault
                                        } else {
                                            DynDefinitionScope::ImplOverride
                                        },
                                        signature,
                                    },
                                    item.span,
                                )?;
                            }
                            builder.push(&mut method_scopes, (id, method.id), item.span)?;
                        } else {
                            method_definitions = None;
                        }
                    } else if !matches!(row.item.kind, s::ItemKind::Trait(_)) {
                        method_definitions = None;
                    }
                }
                s::ItemKind::TypeAlias(_) | s::ItemKind::Const(_) => {}
                _ => {
                    method_signatures = None;
                    method_definitions = None;
                }
            }
        }
        let identity = identity(&scope.identity, builder.budget, row.item.span)?;
        builder.push(
            &mut output,
            Template {
                declaration,
                scope: scope_id,
                identity,
                self_type,
                trait_application,
                supers,
                requirements: scope_requirements,
                associated,
                signatures: method_signatures,
                definitions: method_definitions,
                method_scopes,
                signature_scopes,
            },
            row.item.span,
        )?;
    }
    Ok(output)
}

struct RawType<'a>(&'a Type);
impl DynRecipeMetadata for RawType<'_> {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.ty(self.0, depth)
    }
}
fn type_copy(ty: &Type, budget: &mut Budget<'_>, span: Span) -> R<Type> {
    prepay_recipe(&RawType(ty), budget, span, 0)?;
    Ok(ty.clone())
}
/// Complete borrowed comparison prepayment for the nominal catalogue record.
/// The public recipe grammar has Type/Life/Const hooks but no nominal-instance
/// root implementation; keep this adapter private rather than omit actuals.
struct NominalInstance<'a>(&'a LifeNominalInstance);
impl DynRecipeMetadata for NominalInstance<'_> {
    fn visit_dyn_metadata<V: DynRecipeVisitor>(
        &self,
        visitor: &mut V,
        depth: usize,
    ) -> Result<(), V::Error> {
        visitor.node(depth)?;
        // declaration.module, declaration.node and declaration.compiler
        for _ in 0..3 {
            visitor.node(depth + 1)?;
        }
        visitor.ty(&self.0.ty, depth + 1)?;
        visitor.rows(self.0.arguments.len(), depth + 1)?;
        for actual in &self.0.arguments {
            visitor.node(depth + 2)?;
            match actual {
                LifeNominalActual::Type(ty) => visitor.ty(ty, depth + 3)?,
                LifeNominalActual::QualifiedType { ty, life } => {
                    visitor.ty(ty, depth + 3)?;
                    visitor.life(life, depth + 3)?;
                }
                LifeNominalActual::Const(value) => visitor.constant(value, depth + 3)?,
                LifeNominalActual::Lifetime(_) => visitor.node(depth + 3)?,
            }
        }
        Ok(())
    }
}
fn zero_shape() -> StorageLifeShape {
    StorageLifeShape {
        binder_count: 0,
        slots: Vec::new(),
        retained: Vec::new(),
        nominal_arguments: Vec::new(),
    }
}
fn empty_shape(shape: &StorageLifeShape) -> bool {
    shape.binder_count == 0
        && shape.slots.is_empty()
        && shape.retained.is_empty()
        && shape.nominal_arguments.is_empty()
}
fn nominal_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::BorrowedStorage { storage, .. } => nominal_name(storage),
        Type::Record { name, .. } | Type::Package { name, .. } | Type::Enum { name, .. } => {
            Some(name)
        }
        _ => None,
    }
}
// This is a producer's closed-domain proposal from complete actual nominal
// emissions and their full original actual vectors. It grants no B0, Copy,
// cleanup or permission certificate. Unknown/cyclic/borrowed edges stop it.
fn closed_type(
    ty: &Type,
    cores: &[(&mut CoreModule, &mut zil_base::SourceMap)],
    active: &mut Vec<(ModuleId, usize)>,
    budget: &mut Budget<'_>,
    depth: usize,
) -> R<bool> {
    budget.node(Span::default(), depth)?;
    if ty.is_b0_scalar() {
        return Ok(true);
    }
    Ok(match ty {
        Type::ByteBuffer | Type::ForeignHandle | Type::CWideString | Type::SyncCell(_) => true,
        Type::Managed(inner)
        | Type::OwnedStore { element: inner }
        | Type::Mutex { payload: inner }
        | Type::Array { element: inner, .. } => {
            closed_type(inner, cores, active, budget, depth + 1)?
        }
        Type::Tuple(fields) => {
            for field in fields {
                if !closed_type(field, cores, active, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        Type::Record { fields, .. } | Type::Package { fields, .. } => {
            for (_, field) in fields {
                if !closed_type(field, cores, active, budget, depth + 1)? {
                    return Ok(false);
                }
            }
            true
        }
        Type::Enum { variants, .. } => {
            for (_, fields) in variants {
                for field in fields {
                    if !closed_type(field, cores, active, budget, depth + 1)? {
                        return Ok(false);
                    }
                }
            }
            true
        }
        Type::ErrorUnion { error, result } => {
            closed_type(error, cores, active, budget, depth + 1)?
                && closed_type(result, cores, active, budget, depth + 1)?
        }
        Type::BorrowedStorage { storage, life } => {
            empty_shape(life) && closed_type(storage, cores, active, budget, depth + 1)?
        }
        Type::Named(name) => {
            let mut selected: Option<(ModuleId, usize, &LifeNominalInstance)> = None;
            for (core, _) in cores {
                for (index, instance) in core.life_instances.iter().enumerate() {
                    budget.node(Span::default(), depth)?;
                    if nominal_name(&instance.ty) != Some(name.as_str()) {
                        continue;
                    }
                    if let Some((_, _, old)) = selected {
                        prepay_recipe_work(&NominalInstance(old), budget, Span::default(), depth)?;
                        prepay_recipe_work(
                            &NominalInstance(instance),
                            budget,
                            Span::default(),
                            depth,
                        )?;
                        if old != instance {
                            return Ok(false);
                        }
                    } else {
                        selected = Some((core.module_id, index, instance));
                    }
                }
            }
            let Some((module, index, instance)) = selected else {
                return Ok(false);
            };
            budget.identity_work(active.len(), Span::default())?;
            if active.contains(&(module, index)) {
                return Ok(false);
            }
            let mut declaration: Option<&LifeNominalDeclaration> = None;
            for (core, _) in cores {
                for candidate in &core.life_declarations {
                    budget.node(Span::default(), depth)?;
                    if candidate.id == instance.declaration {
                        if let Some(old) = declaration {
                            if old != candidate {
                                return Ok(false);
                            }
                        } else {
                            declaration = Some(candidate)
                        }
                    }
                }
            }
            let Some(declaration) = declaration else {
                return Ok(false);
            };
            if declaration.parameters.len() != instance.arguments.len() {
                return Ok(false);
            }
            budget.lineage_push(active, (module, index), Span::default())?;
            for (formal, actual) in declaration.parameters.iter().zip(&instance.arguments) {
                match (formal, actual) {
                    (LifeParameterKind::Type, LifeNominalActual::Type(ty)) => {
                        if !closed_type(ty, cores, active, budget, depth + 1)? {
                            active.pop();
                            return Ok(false);
                        }
                    }
                    (LifeParameterKind::Type, LifeNominalActual::QualifiedType { ty, life }) => {
                        if !empty_shape(life) || !closed_type(ty, cores, active, budget, depth + 1)?
                        {
                            active.pop();
                            return Ok(false);
                        }
                    }
                    (LifeParameterKind::Const(_), LifeNominalActual::Const(_)) => {}
                    _ => {
                        active.pop();
                        return Ok(false);
                    }
                }
            }
            let result = closed_type(&instance.ty, cores, active, budget, depth + 1)?;
            active.pop();
            result
        }
        // Payload borrowing and hidden carrier domains are never inferred
        // empty from an absent outer class table or an erased raw pointer.
        _ => false,
    })
}
fn closed_value(
    ty: &Type,
    cores: &[(&mut CoreModule, &mut zil_base::SourceMap)],
    budget: &mut Budget<'_>,
) -> R<Option<DynTypeValue>> {
    let mut active = Vec::new();
    let closed = closed_type(ty, cores, &mut active, budget, 0)?;
    budget.drop_lineage_vec(active, Span::default())?;
    Ok(if closed {
        Some(DynTypeValue {
            ty: type_copy(ty, budget, Span::default())?,
            life: zero_shape(),
            classes: Vec::new(),
        })
    } else {
        None
    })
}
fn binding(
    core: &mut CoreModule,
    owner: SemanticDynOwner,
    scope: SemanticDefinitionId,
    budget: &mut Budget<'_>,
) -> R<()> {
    let semantic = core.semantic.as_mut().ok_or_else(|| {
        error(
            Span::default(),
            "original Dyn output has no complete semantic program",
        )
    })?;
    budget.identity_work(semantic.dyn_scopes.len(), Span::default())?;
    if let Some(old) = semantic.dyn_scopes.iter().find(|row| row.owner == owner) {
        if old.scope != scope {
            return Err(error(
                Span::default(),
                "original Dyn owner has conflicting scopes",
            ));
        }
        return Ok(());
    }
    budget.lineage_push(
        &mut semantic.dyn_scopes,
        SemanticDynScopeBinding { owner, scope },
        Span::default(),
    )
}
fn core_position(
    cores: &[(&mut CoreModule, &mut zil_base::SourceMap)],
    module: ModuleId,
    function: zil_base::NodeId,
    budget: &mut Budget<'_>,
) -> R<Option<(usize, usize)>> {
    let mut found = None;
    for (mi, (core, _)) in cores.iter().enumerate() {
        if core.module_id != module {
            continue;
        }
        for (fi, candidate) in core.functions.iter().enumerate() {
            budget.node(candidate.span, 0)?;
            if candidate.id == function && found.replace((mi, fi)).is_some() {
                return Err(error(
                    candidate.span,
                    "Dyn method actual Core target is duplicated",
                ));
            }
        }
    }
    Ok(found)
}
fn semantic_definition<'a>(
    cores: &'a [(&mut CoreModule, &mut zil_base::SourceMap)],
    id: SemanticDefinitionId,
    budget: &mut Budget<'_>,
) -> R<&'a SemanticDefinition> {
    let mut found = None;
    for (core, _) in cores {
        if let Some(semantic) = &core.semantic {
            for definition in &semantic.definitions {
                budget.node(Span::default(), 0)?;
                if definition.id == id && found.replace(definition).is_some() {
                    return Err(error(
                        Span::default(),
                        "Dyn method original definition is duplicated",
                    ));
                }
            }
        }
    }
    found.ok_or_else(|| {
        error(
            Span::default(),
            "Dyn method original scope disappeared before actual emission",
        )
    })
}
fn method_instances(
    definition: &mut DynDefinitionDeclaration,
    scope: SemanticDefinitionId,
    cores: &mut [(&mut CoreModule, &mut zil_base::SourceMap)],
    budget: &mut Budget<'_>,
) -> R<()> {
    let original = semantic_definition(cores, scope, budget)?;
    let SemanticDefinitionBody::FunctionWithBody {
        receiver_parameter, ..
    } = &original.body
    else {
        return Err(error(
            Span::default(),
            "Dyn actual method has no authored function",
        ));
    };
    let Some(receiver_parameter) = *receiver_parameter else {
        return Ok(());
    };
    // This closed instantiation path has no unresolved lexical actuals. More
    // general actuals remain in the real Semantic instance, never guessed here.
    if !original.parameters.is_empty() {
        return Ok(());
    }
    let Some(parent_scope) = original.parent else {
        return Ok(());
    };
    if !semantic_definition(cores, parent_scope, budget)?
        .parameters
        .is_empty()
    {
        return Ok(());
    }
    let mut requests = Vec::new();
    for (core, _) in cores.iter() {
        let Some(semantic) = &core.semantic else {
            continue;
        };
        for instance in &semantic.instances {
            budget.node(Span::default(), 0)?;
            if instance.definition != scope || !instance.actuals.is_empty() {
                continue;
            }
            let SemanticInstanceTarget::Function(SemanticFunctionTarget::Core { module, function }) =
                instance.target
            else {
                continue;
            };
            let Some(parent) = instance.parent else {
                continue;
            };
            let mut parent_found = None;
            for (owner, _) in cores.iter() {
                if let Some(semantic) = &owner.semantic {
                    for candidate in &semantic.instances {
                        budget.node(Span::default(), 0)?;
                        if candidate.id == parent {
                            if parent_found.replace(candidate).is_some() {
                                return Err(error(
                                    Span::default(),
                                    "Dyn lexical instance identity is duplicated",
                                ));
                            }
                        }
                    }
                }
            }
            let Some(parent) = parent_found else { continue };
            if parent.definition != parent_scope
                || !parent.actuals.is_empty()
                || !matches!(parent.target, SemanticInstanceTarget::Scope)
            {
                continue;
            }
            budget.lineage_push(&mut requests, (module, function), Span::default())?;
        }
    }
    for (module, function) in &requests {
        let Some((mi, fi)) = core_position(cores, *module, *function, budget)? else {
            return Err(error(
                Span::default(),
                "Dyn exact original emission has no actual Core function",
            ));
        };
        let actual = &cores[mi].0.functions[fi];
        let Some(receiver) = actual.parameters.get(receiver_parameter as usize) else {
            return Err(error(
                actual.span,
                "Dyn receiver parameter is absent from actual emitted header",
            ));
        };
        let Type::Address { inner, .. } = &receiver.ty else {
            continue;
        };
        let Some(receiver) = closed_value(inner, cores, budget)? else {
            continue;
        };
        let Some(header) = &actual.storage_life else {
            continue;
        };
        // A nonempty Fixed header needs its original lexical parameter map;
        // this path does not coalesce it with same-numbered Invocation slots.
        if header.fixed_binder_count != 0 || !header.classes.is_empty() {
            continue;
        }
        let mut embedding = Vec::new();
        for index in 0..header.invocation_binder_count {
            budget.lineage_push(
                &mut embedding,
                DynLifeEmbedding {
                    from: StorageBinder::Invocation(index),
                    to: StorageBinder::Invocation(index),
                },
                actual.span,
            )?;
        }
        let row = DynFunctionInstance {
            scope: DynInstantiationScope {
                fixed_binder_count: 0,
                invocation_binder_count: header.invocation_binder_count,
            },
            header_embedding: embedding,
            definition: definition.id,
            receiver: copy(&receiver, budget, actual.span)?,
            declaration_actuals: Vec::new(),
            method_actuals: Vec::new(),
        };
        let specialization = DynDefinitionSpecialization {
            function: DynFunctionTarget::Core {
                module: *module,
                source: *function,
            },
            receiver,
            declaration_actuals: Vec::new(),
            method_actuals: Vec::new(),
        };
        let actual = &mut cores[mi].0.functions[fi];
        if let Some(old) = &actual.dyn_instance {
            prepay_recipe_work(old, budget, actual.span, 0)?;
            prepay_recipe_work(&row, budget, actual.span, 0)?;
            if old != &row {
                return Err(error(
                    actual.span,
                    "one emitted Dyn method has incompatible original instances",
                ));
            }
        } else {
            actual.dyn_instance = Some(row)
        }
        budget.identity_work(definition.instances.len(), actual.span)?;
        if !definition
            .instances
            .iter()
            .any(|old| old.function == specialization.function)
        {
            budget.lineage_push(&mut definition.instances, specialization, actual.span)?;
        }
    }
    budget.drop_lineage_vec(requests, Span::default())
}
fn no_dependencies(declaration: &DynDeclaration) -> bool {
    declaration
        .direct_supertraits
        .as_ref()
        .is_some_and(Vec::is_empty)
        && declaration.requirements.as_ref().is_some_and(Vec::is_empty)
        && declaration
            .associated_definitions
            .as_ref()
            .is_some_and(Vec::is_empty)
        && declaration.parameters.is_empty()
}
fn witnesses(
    cores: &mut [(&mut CoreModule, &mut zil_base::SourceMap)],
    budget: &mut Budget<'_>,
) -> R<()> {
    for mi in 0..cores.len() {
        for ti in 0..cores[mi].0.dyn_vtables.len() {
            let table = &cores[mi].0.dyn_vtables[ti];
            if table.implementation_witnesses.is_some()
                || !table.actuals.is_empty()
                || !table.self_classes.is_empty()
            {
                continue;
            }
            let mut interface = None;
            let mut implementation = None;
            for (core, _) in cores.iter() {
                for candidate in &core.dyn_interfaces {
                    budget.node(Span::default(), 0)?;
                    if candidate.id == table.interface {
                        if interface.replace(candidate).is_some() {
                            return Err(error(
                                Span::default(),
                                "Dyn interface identity is duplicated",
                            ));
                        }
                    }
                }
                for candidate in &core.dyn_declarations {
                    budget.node(Span::default(), 0)?;
                    if candidate.id == table.implementation {
                        if implementation.replace(candidate).is_some() {
                            return Err(error(
                                Span::default(),
                                "Dyn implementation declaration identity is duplicated",
                            ));
                        }
                    }
                }
            }
            let (Some(interface), Some(implementation)) = (interface, implementation) else {
                continue;
            };
            if !no_dependencies(implementation)
                || !interface.application.actuals.is_empty()
                || !interface.inherited.is_empty()
                || !interface.associated_types.is_empty()
                || !interface.associated_constants.is_empty()
                || interface.retained_life.binder_count != 0
                || !interface.retained_life.ports.is_empty()
                || !interface.retained_life.retained.is_empty()
                || !interface.retained_life.nominal_arguments.is_empty()
            {
                continue;
            }
            let mut trait_declaration = None;
            for (core, _) in cores.iter() {
                for declaration in &core.dyn_declarations {
                    budget.node(Span::default(), 0)?;
                    if declaration.id == interface.application.declaration {
                        if trait_declaration.replace(declaration).is_some() {
                            return Err(error(
                                Span::default(),
                                "Dyn original trait identity is duplicated",
                            ));
                        }
                    }
                }
            }
            let Some(trait_declaration) = trait_declaration else {
                continue;
            };
            if !no_dependencies(trait_declaration) {
                continue;
            }
            let Some(receiver) = closed_value(&table.self_type, cores, budget)? else {
                continue;
            };
            let witness = DynImplementationWitness {
                scope: DynInstantiationScope {
                    fixed_binder_count: 0,
                    invocation_binder_count: 0,
                },
                to_context: Vec::new(),
                receiver,
                application: copy(&interface.application, budget, Span::default())?,
                implementation: table.implementation,
                actuals: Vec::new(),
                associated: Vec::new(),
            };
            let mut graph = Vec::new();
            budget.lineage_push(&mut graph, witness, Span::default())?;
            // The empty local Self shape is justified as a complete proposal
            // above, not recovered from an absent table.self_life.
            cores[mi].0.dyn_vtables[ti].self_life = Some(zero_shape());
            cores[mi].0.dyn_vtables[ti].implementation_witnesses = Some(graph);
        }
    }
    Ok(())
}

pub(super) fn publish(
    templates: &mut [Template],
    cores: &mut [(&mut CoreModule, &mut zil_base::SourceMap)],
    budget: &mut Budget<'_>,
) -> R<()> {
    for template in templates {
        let mut selected = None;
        for (mi, (core, _)) in cores.iter().enumerate() {
            for (di, declaration) in core.dyn_declarations.iter().enumerate() {
                budget.node(Span::default(), 0)?;
                if declaration.id == template.declaration && selected.replace((mi, di)).is_some() {
                    return Err(error(
                        Span::default(),
                        "original Dyn declaration was emitted twice",
                    ));
                }
            }
        }
        let Some((mi, di)) = selected else { continue };
        prepay_recipe_work(&Identity(&template.identity), budget, Span::default(), 0)?;
        prepay_recipe_work(
            &Identity(&cores[mi].0.dyn_declarations[di].identity),
            budget,
            Span::default(),
            0,
        )?;
        if cores[mi].0.dyn_declarations[di].identity != template.identity {
            return Err(error(
                Span::default(),
                "Dyn actual declaration has a different original identity",
            ));
        }
        if let Some(definitions) = &mut template.definitions {
            for definition in definitions {
                budget.identity_work(template.method_scopes.len(), Span::default())?;
                let scope = template
                    .method_scopes
                    .iter()
                    .find(|(id, _)| *id == definition.id)
                    .map(|(_, scope)| *scope)
                    .ok_or_else(|| {
                        error(
                            Span::default(),
                            "Dyn method definition has no original scope mapping",
                        )
                    })?;
                method_instances(definition, scope, cores, budget)?;
            }
        }
        let declaration = &mut cores[mi].0.dyn_declarations[di];
        declaration.self_pattern = template.self_type.take();
        declaration.implemented_trait = template.trait_application.take();
        declaration.implicit_lifetimes = Some(Vec::new());
        declaration.direct_supertraits = template.supers.take();
        declaration.requirements = template.requirements.take();
        declaration.associated_definitions = template.associated.take();
        declaration.method_signatures = template.signatures.take();
        declaration.method_definitions = template.definitions.take();
        binding(
            cores[mi].0,
            SemanticDynOwner::Declaration(template.declaration),
            template.scope,
            budget,
        )?;
        for (member, scope) in &template.signature_scopes {
            binding(
                cores[mi].0,
                SemanticDynOwner::MethodSignature {
                    declaration: template.declaration,
                    member: *member,
                },
                *scope,
                budget,
            )?;
        }
        for (id, scope) in &template.method_scopes {
            binding(
                cores[mi].0,
                SemanticDynOwner::MethodDefinition(*id),
                *scope,
                budget,
            )?;
        }
    }
    witnesses(cores, budget)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_context::{DeclarationPlacement, SourceIdentityContext};
    use std::collections::BTreeMap;
    use zil_base::{FileId, SourceUnit};

    #[test]
    fn real_owned_dyn_methods_keep_original_scopes_and_actual_emission_links() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "dyn-original.zil",
            include_str!(
                "../../../tests/language-readiness/dynamic-dispatch/dyn-owned-user-drop-store.zil"
            ),
        );
        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let output = crate::expand_with_map_semantics(
            &source,
            &syntax,
            &Limits::default(),
            &zil_base::TargetProfile::default(),
            SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &BTreeMap::new(),
                declarations: DeclarationPlacement::Original,
            },
        )
        .unwrap();
        let core = output.core;
        let semantic = core.semantic.as_ref().unwrap();
        assert_eq!(core.dyn_vtables.len(), 2);
        assert_eq!(core.dyn_declarations.len(), 3);
        for declaration in &core.dyn_declarations {
            assert_eq!(
                declaration.direct_supertraits.as_deref(),
                Some([].as_slice())
            );
            assert_eq!(declaration.requirements.as_deref(), Some([].as_slice()));
            assert_eq!(
                declaration.associated_definitions.as_deref(),
                Some([].as_slice())
            );
            let scope = semantic
                .dyn_scopes
                .iter()
                .find(|binding| binding.owner == SemanticDynOwner::Declaration(declaration.id))
                .unwrap()
                .scope;
            let original = semantic
                .definitions
                .iter()
                .find(|definition| definition.id == scope)
                .unwrap();
            assert_eq!(declaration.identity, original.identity);
            if declaration.kind == DynDeclarationKind::Trait {
                let method = &declaration.method_signatures.as_ref().unwrap()[0];
                assert_eq!(method.member, 0);
                let method_scope = semantic
                    .dyn_scopes
                    .iter()
                    .find(|binding| {
                        binding.owner
                            == SemanticDynOwner::MethodSignature {
                                declaration: declaration.id,
                                member: 0,
                            }
                    })
                    .unwrap()
                    .scope;
                assert!(
                    matches!(&method.signature.parameters[0].ty,DynTypePattern::Semantic {scope,ty} if *scope==method_scope&&matches!(ty.as_ref(),SemanticTypeRecipe::Address {..}))
                );
                assert_eq!(
                    method.signature.contract.requires.as_ref().unwrap(),
                    &[DynRequirementRecipe::Read(position(0))]
                );
                assert_eq!(
                    method.signature.contract.result,
                    Some(DynResultSourceRecipe::Value)
                );
            } else {
                let method = &declaration.method_definitions.as_ref().unwrap()[0];
                let scope = semantic
                    .dyn_scopes
                    .iter()
                    .find(|binding| binding.owner == SemanticDynOwner::MethodDefinition(method.id))
                    .unwrap()
                    .scope;
                assert_eq!(
                    method.id,
                    DynDefinitionId {
                        module: scope.module,
                        source: zil_base::NodeId(scope.index)
                    }
                );
                assert!(!method.instances.is_empty());
                for instance in &method.instances {
                    let DynFunctionTarget::Core { module, source } = instance.function else {
                        panic!()
                    };
                    assert_eq!(module, core.module_id);
                    let function = core
                        .functions
                        .iter()
                        .find(|function| function.id == source)
                        .unwrap();
                    assert_eq!(
                        function.dyn_instance.as_ref().unwrap().definition,
                        method.id
                    );
                    assert!(
                        semantic
                            .instances
                            .iter()
                            .any(|actual| actual.definition == scope
                                && actual.target
                                    == SemanticInstanceTarget::Function(
                                        SemanticFunctionTarget::Core {
                                            module,
                                            function: source
                                        }
                                    ))
                    );
                }
            }
        }
        for table in &core.dyn_vtables {
            let graph = table.implementation_witnesses.as_ref().unwrap();
            assert_eq!(graph.len(), 1);
            assert_eq!(graph[0].implementation, table.implementation);
            assert!(empty_shape(&graph[0].receiver.life));
            assert_eq!(graph[0].receiver.ty, table.self_type);
        }
    }
}
