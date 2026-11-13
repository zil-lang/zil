//! Complete raw recipe identity with an explicit traversal stack.
use super::*;
enum Work<'a> {
    Byte(u8),
    Word(u32),
    Length(usize),
    DynTypeValue(&'a DynTypeValue),
    DynInstantiationScope(&'a DynInstantiationScope),
    DynLifeEmbedding(&'a DynLifeEmbedding),
    DynImplicitLifeKind(&'a DynImplicitLifeKind),
    DynImplicitLifeRoot(&'a DynImplicitLifeRoot),
    DynImplicitLifeOccurrence(&'a DynImplicitLifeOccurrence),
    DynImplicitLifeDeclaration(&'a DynImplicitLifeDeclaration),
    DynLifePattern(&'a DynLifePattern),
    DynObligation(&'a DynObligation),
    DynParameterRecipe(&'a DynParameterRecipe),
    DynSignatureRecipe(&'a DynSignatureRecipe),
    DynRequirementRecipe(&'a DynRequirementRecipe),
    DynGuaranteeRecipe(&'a DynGuaranteeRecipe),
    DynEffectRecipe(&'a DynEffectRecipe),
    DynCleanupRecipe(&'a DynCleanupRecipe),
    DynContractRecipe(&'a DynContractRecipe),
    DynStorageSourceRecipe(&'a DynStorageSourceRecipe),
    DynStorageSourceBindingRecipe(&'a DynStorageSourceBindingRecipe),
    DynResultSourceRecipe(&'a DynResultSourceRecipe),
    DynDefinitionId(&'a DynDefinitionId),
    DynDefinitionScope(&'a DynDefinitionScope),
    DynDefinitionDeclaration(&'a DynDefinitionDeclaration),
    DynMethodRecipe(&'a DynMethodRecipe),
    DynFunctionInstance(&'a DynFunctionInstance),
    DynAssociatedValueRecipe(&'a DynAssociatedValueRecipe),
    DynAssociatedDefinition(&'a DynAssociatedDefinition),
    DynAssociatedSource(&'a DynAssociatedSource),
    DynAssociatedValue(&'a DynAssociatedValue),
    DynAssociatedResolution(&'a DynAssociatedResolution),
    DynImplementationWitness(&'a DynImplementationWitness),
    DynFunctionTarget(&'a DynFunctionTarget),
    DynDefinitionSpecialization(&'a DynDefinitionSpecialization),
    DynStructuralBound(&'a DynStructuralBound),
    SemanticDefinitionId(&'a SemanticDefinitionId),
    SemanticInstanceId(&'a SemanticInstanceId),
    SemanticModule(&'a SemanticModule),
    SemanticDynScopeBinding(&'a SemanticDynScopeBinding),
    SemanticDynOwner(&'a SemanticDynOwner),
    SemanticDefinition(&'a SemanticDefinition),
    SemanticDefinitionKind(&'a SemanticDefinitionKind),
    SemanticDefinitionBody(&'a SemanticDefinitionBody),
    SemanticScopeRecipe(&'a SemanticScopeRecipe),
    SemanticNominalBody(&'a SemanticNominalBody),
    SemanticFieldRecipe(&'a SemanticFieldRecipe),
    SemanticVariantRecipe(&'a SemanticVariantRecipe),
    SemanticFormal(&'a SemanticFormal),
    SemanticImplicitLifetime(&'a SemanticImplicitLifetime),
    SemanticEnvironmentFormal(&'a SemanticEnvironmentFormal),
    SemanticInstance(&'a SemanticInstance),
    SemanticLifetimeInstantiation(&'a SemanticLifetimeInstantiation),
    SemanticFunctionTarget(&'a SemanticFunctionTarget),
    SemanticInstanceTarget(&'a SemanticInstanceTarget),
    SemanticClosureTarget(&'a SemanticClosureTarget),
    SemanticHeaderBinding(&'a SemanticHeaderBinding),
    SemanticEnvironmentInstance(&'a SemanticEnvironmentInstance),
    SemanticActual(&'a SemanticActual),
    SemanticCoreTypeSource(&'a SemanticCoreTypeSource),
    SemanticTypeValue(&'a SemanticTypeValue),
    SemanticScopeRef(&'a SemanticScopeRef),
    SemanticClassExpr(&'a SemanticClassExpr),
    SemanticOperand(&'a SemanticOperand),
    SemanticSiteRecipe(&'a SemanticSiteRecipe),
    SemanticSiteRecipeKind(&'a SemanticSiteRecipeKind),
    SemanticSite(&'a SemanticSite),
    SemanticSiteAddress(&'a SemanticSiteAddress),
    SemanticEmissionRole(&'a SemanticEmissionRole),
    SemanticSiteEdge(&'a SemanticSiteEdge),
    SemanticSiteAction(&'a SemanticSiteAction),
    SemanticLocalTarget(&'a SemanticLocalTarget),
    SemanticRecipeOperand(&'a SemanticRecipeOperand),
    SemanticFormalRef(&'a SemanticFormalRef),
    SemanticLifeRecipe(&'a SemanticLifeRecipe),
    SemanticOuterLifeRecipe(&'a SemanticOuterLifeRecipe),
    SemanticTypeRecipe(&'a SemanticTypeRecipe),
    SemanticExactTypeRecipe(&'a SemanticExactTypeRecipe),
    SemanticCodeKind(&'a SemanticCodeKind),
    SemanticActualRecipe(&'a SemanticActualRecipe),
    SemanticClassSequenceRecipe(&'a SemanticClassSequenceRecipe),
    SemanticClassSequencePart(&'a SemanticClassSequencePart),
    SemanticStorageShapeRecipe(&'a SemanticStorageShapeRecipe),
    SemanticSlotRecipe(&'a SemanticSlotRecipe),
    SemanticRetainedPart(&'a SemanticRetainedPart),
    SemanticNominalPart(&'a SemanticNominalPart),
    SemanticRetainedShapeRecipe(&'a SemanticRetainedShapeRecipe),
    SemanticPortPart(&'a SemanticPortPart),
    SemanticConstRecipe(&'a SemanticConstRecipe),
    SemanticTraitRecipe(&'a SemanticTraitRecipe),
    SemanticRequirementRecipe(&'a SemanticRequirementRecipe),
    SemanticSignatureRecipe(&'a SemanticSignatureRecipe),
    SemanticParameterRecipe(&'a SemanticParameterRecipe),
    SemanticDynContextRecipe(&'a SemanticDynContextRecipe),
    SemanticDynInterfaceRecipe(&'a SemanticDynInterfaceRecipe),
    SemanticAssociatedBindingRecipe(&'a SemanticAssociatedBindingRecipe),
    SemanticAssociatedRecipe(&'a SemanticAssociatedRecipe),
    DynSelfPattern(&'a DynSelfPattern),
    DynPatternActual(&'a DynPatternActual),
    DynConstPattern(&'a DynConstPattern),
    DynTypePattern(&'a DynTypePattern),
    StoredBorrowMode(&'a StoredBorrowMode),
    LifeNominalKind(&'a LifeNominalKind),
    LifeTypeProjection(&'a LifeTypeProjection),
    PlaceProjection(&'a PlaceProjection),
    VariantProjection(&'a VariantProjection),
    String(&'a String),
    Type(&'a Type),
    StorageLifeShape(&'a StorageLifeShape),
    StorageBinder(&'a StorageBinder),
    DynGenericActual(&'a DynGenericActual),
    StoragePosition(&'a StoragePosition),
    DynCallAccess(&'a DynCallAccess),
    DynResumeBound(&'a DynResumeBound),
    DynThreadRequirements(&'a DynThreadRequirements),
    PassingMode(&'a PassingMode),
    CallKind(&'a CallKind),
    LifeDeclarationIdentity(&'a LifeDeclarationIdentity),
    LifeDeclarationId(&'a LifeDeclarationId),
    DynTraitApplication(&'a DynTraitApplication),
    DynMember(&'a DynMember),
    Constant(&'a Constant),
}
fn encode_recipe(b: &mut impl FingerprintSink, first: Work<'_>) {
    let mut pending = vec![first];
    while let Some(next) = pending.pop() {
        match next {
            Work::Byte(value) => b.push(value),
            Work::Word(value) => encode_u32(b, value),
            Work::Length(value) => encode_len(b, value),
            Work::String(value) => encode_text(b, value),
            Work::Type(value) => encode_type(b, value),
            Work::StorageLifeShape(value) => encode_storage_shape(b, value),
            Work::StorageBinder(value) => encode_storage_binder(b, value),
            Work::DynGenericActual(value) => encode_dyn_generic_actual(b, value),
            Work::StoragePosition(value) => encode_storage_position(b, value),
            Work::DynCallAccess(value) => encode_dyn_call_access(b, value),
            Work::DynResumeBound(value) => encode_dyn_resume_bound(b, value),
            Work::DynThreadRequirements(value) => encode_dyn_thread_requirements(b, value),
            Work::PassingMode(value) => encode_dyn_passing_mode(b, value),
            Work::CallKind(value) => encode_dyn_call_kind(b, value),
            Work::LifeDeclarationIdentity(value) => encode_declaration_identity(b, value),
            Work::LifeDeclarationId(value) => encode_declaration_id(b, value),
            Work::DynTraitApplication(value) => encode_dyn_trait_application(b, value),
            Work::DynMember(value) => encode_dyn_member(b, value),
            Work::Constant(value) => encode_constant(b, value),
            Work::DynTypeValue(value) => {
                for item in value.classes.iter().rev() {
                    pending.push(Work::StorageBinder(item));
                }
                pending.push(Work::Length(value.classes.len()));
                pending.push(Work::StorageLifeShape(&value.life));
                pending.push(Work::Type(&value.ty));
            }
            Work::DynInstantiationScope(value) => {
                pending.push(Work::Word(value.invocation_binder_count));
                pending.push(Work::Word(value.fixed_binder_count));
            }
            Work::DynLifeEmbedding(value) => {
                pending.push(Work::StorageBinder(&value.to));
                pending.push(Work::StorageBinder(&value.from));
            }
            Work::DynImplicitLifeKind(value) => match value {
                DynImplicitLifeKind::Address => {
                    b.push(0);
                }
                DynImplicitLifeKind::Description => {
                    b.push(1);
                }
                DynImplicitLifeKind::LifetimeArgument => {
                    b.push(2);
                }
            },
            Work::DynImplicitLifeRoot(value) => match value {
                DynImplicitLifeRoot::SelfPattern => {
                    b.push(0);
                }
                DynImplicitLifeRoot::ImplementedTrait => {
                    b.push(1);
                }
                DynImplicitLifeRoot::Supertrait(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(2);
                }
                DynImplicitLifeRoot::Requirement(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(3);
                }
                DynImplicitLifeRoot::AssociatedDefinition(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(4);
                }
                DynImplicitLifeRoot::MethodSignature(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(5);
                }
                DynImplicitLifeRoot::MethodDefinition(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(6);
                }
            },
            Work::DynImplicitLifeOccurrence(value) => {
                pending.push(Work::DynImplicitLifeKind(&value.kind));
                for item in value.path.iter().rev() {
                    pending.push(Work::Word(*item));
                }
                pending.push(Work::Length(value.path.len()));
                pending.push(Work::DynImplicitLifeRoot(&value.root));
            }
            Work::DynImplicitLifeDeclaration(value) => {
                for item in value.occurrences.iter().rev() {
                    pending.push(Work::DynImplicitLifeOccurrence(item));
                }
                pending.push(Work::Length(value.occurrences.len()));
            }
            Work::DynLifePattern(value) => match value {
                DynLifePattern::DeclarationParameter(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(0);
                }
                DynLifePattern::MethodParameter(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(1);
                }
                DynLifePattern::Bound { depth, index } => {
                    pending.push(Work::Word(*index));
                    pending.push(Work::Word(*depth));
                    b.push(2);
                }
                DynLifePattern::Invocation(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(3);
                }
                DynLifePattern::ImplicitDeclaration(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(4);
                }
            },
            Work::DynObligation(value) => match value {
                DynObligation::Structural { receiver, bound } => {
                    pending.push(Work::DynStructuralBound(bound));
                    pending.push(Work::DynTypePattern(receiver));
                    b.push(0);
                }
                DynObligation::Trait {
                    receiver,
                    application,
                } => {
                    pending.push(Work::DynSelfPattern(application));
                    pending.push(Work::DynTypePattern(receiver));
                    b.push(1);
                }
                DynObligation::Callable {
                    receiver,
                    kind,
                    quantified_lifetimes,
                    signature,
                } => {
                    pending.push(Work::DynSignatureRecipe(signature));
                    pending.push(Work::Word(*quantified_lifetimes));
                    pending.push(Work::Byte(match kind {
                        CallableKind::FunctionPointer => 0,
                        CallableKind::Fn => 1,
                        CallableKind::FnMut => 2,
                        CallableKind::FnOnce => 3,
                    }));
                    pending.push(Work::DynTypePattern(receiver));
                    b.push(2);
                }
                DynObligation::TypeEqual { left, right } => {
                    pending.push(Work::DynTypePattern(right));
                    pending.push(Work::DynTypePattern(left));
                    b.push(3);
                }
                DynObligation::ConstEqual { ty, left, right } => {
                    pending.push(Work::DynConstPattern(right));
                    pending.push(Work::DynConstPattern(left));
                    pending.push(Work::DynTypePattern(ty));
                    b.push(4);
                }
            },
            Work::DynParameterRecipe(value) => {
                pending.push(Work::PassingMode(&value.mode));
                pending.push(Work::DynTypePattern(&value.ty));
            }
            Work::DynSignatureRecipe(value) => {
                pending.push(Work::DynContractRecipe(&value.contract));
                pending.push(Work::Word(value.invocation_binder_count));
                pending.push(Work::CallKind(&value.call_kind));
                pending.push(Work::PassingMode(&value.result_mode));
                pending.push(Work::DynTypePattern(&value.result));
                for item in value.parameters.iter().rev() {
                    pending.push(Work::DynParameterRecipe(item));
                }
                pending.push(Work::Length(value.parameters.len()));
            }
            Work::DynRequirementRecipe(value) => match value {
                DynRequirementRecipe::Read(value_0) => {
                    pending.push(Work::StoragePosition(value_0));
                    b.push(0);
                }
                DynRequirementRecipe::Write(value_0) => {
                    pending.push(Work::StoragePosition(value_0));
                    b.push(1);
                }
                DynRequirementRecipe::Manage(value_0) => {
                    pending.push(Work::StoragePosition(value_0));
                    b.push(2);
                }
                DynRequirementRecipe::Apart(value_0, value_1) => {
                    pending.push(Work::StoragePosition(value_1));
                    pending.push(Work::StoragePosition(value_0));
                    b.push(3);
                }
                DynRequirementRecipe::State {
                    position,
                    ty,
                    variant,
                } => {
                    pending.push(Work::Word(*variant));
                    pending.push(Work::DynTypePattern(ty));
                    pending.push(Work::StoragePosition(position));
                    b.push(4);
                }
            },
            Work::DynGuaranteeRecipe(value) => match value {
                DynGuaranteeRecipe::StorageSources(value_0) => {
                    pending.push(Work::DynStorageSourceBindingRecipe(value_0));
                    b.push(0);
                }
                DynGuaranteeRecipe::ManageResult => {
                    b.push(1);
                }
                DynGuaranteeRecipe::State {
                    position,
                    ty,
                    variant,
                } => {
                    pending.push(Work::Word(*variant));
                    pending.push(Work::DynTypePattern(ty));
                    pending.push(Work::StoragePosition(position));
                    b.push(2);
                }
            },
            Work::DynEffectRecipe(value) => match value {
                DynEffectRecipe::Alloc => {
                    b.push(0);
                }
                DynEffectRecipe::Io => {
                    b.push(1);
                }
                DynEffectRecipe::Block => {
                    b.push(2);
                }
                DynEffectRecipe::Suspend => {
                    b.push(3);
                }
                DynEffectRecipe::Spawn => {
                    b.push(4);
                }
                DynEffectRecipe::Ffi => {
                    b.push(5);
                }
                DynEffectRecipe::Reenter => {
                    b.push(6);
                }
                DynEffectRecipe::Panic => {
                    b.push(7);
                }
                DynEffectRecipe::Call(value_0) => {
                    pending.push(Work::StoragePosition(value_0));
                    b.push(8);
                }
                DynEffectRecipe::Cleanup(value_0) => {
                    pending.push(Work::DynTypePattern(value_0));
                    b.push(9);
                }
            },
            Work::DynCleanupRecipe(value) => {
                for item in value.effects.iter().rev() {
                    pending.push(Work::DynEffectRecipe(item));
                }
                pending.push(Work::Length(value.effects.len()));
                pending.push(Work::DynCallAccess(&value.access));
                for item in value.positions.iter().rev() {
                    pending.push(Work::StoragePosition(item));
                }
                pending.push(Work::Length(value.positions.len()));
                pending.push(Work::Byte(u8::from(value.environment)));
            }
            Work::DynContractRecipe(value) => {
                if let Some(item) = &value.threads {
                    pending.push(Work::DynThreadRequirements(item));
                }
                pending.push(Work::Byte(u8::from(value.threads.is_some())));
                if let Some(item) = &value.resume {
                    pending.push(Work::DynResumeBound(item));
                }
                pending.push(Work::Byte(u8::from(value.resume.is_some())));
                if let Some(item) = &value.effects {
                    for item in (item).iter().rev() {
                        pending.push(Work::DynEffectRecipe(item));
                    }
                    pending.push(Work::Length((item).len()));
                }
                pending.push(Work::Byte(u8::from(value.effects.is_some())));
                if let Some(item) = &value.cleanup {
                    pending.push(Work::DynCleanupRecipe(item));
                }
                pending.push(Work::Byte(u8::from(value.cleanup.is_some())));
                if let Some(item) = &value.result {
                    pending.push(Work::DynResultSourceRecipe(item));
                }
                pending.push(Work::Byte(u8::from(value.result.is_some())));
                if let Some(item) = &value.call {
                    pending.push(Work::DynCallAccess(item));
                }
                pending.push(Work::Byte(u8::from(value.call.is_some())));
                if let Some(item) = &value.hold {
                    for item in (item).iter().rev() {
                        pending.push(Work::StoragePosition(item));
                    }
                    pending.push(Work::Length((item).len()));
                }
                pending.push(Work::Byte(u8::from(value.hold.is_some())));
                if let Some(item) = &value.guarantees {
                    for item in (item).iter().rev() {
                        pending.push(Work::DynGuaranteeRecipe(item));
                    }
                    pending.push(Work::Length((item).len()));
                }
                pending.push(Work::Byte(u8::from(value.guarantees.is_some())));
                if let Some(item) = &value.requires {
                    for item in (item).iter().rev() {
                        pending.push(Work::DynRequirementRecipe(item));
                    }
                    pending.push(Work::Length((item).len()));
                }
                pending.push(Work::Byte(u8::from(value.requires.is_some())));
            }
            Work::DynStorageSourceRecipe(value) => match value {
                DynStorageSourceRecipe::Exact(value_0) => {
                    pending.push(Work::StoragePosition(value_0));
                    b.push(0);
                }
                DynStorageSourceRecipe::PlaceAddress { place, mode } => {
                    pending.push(Work::StoredBorrowMode(mode));
                    pending.push(Work::StoragePosition(place));
                    b.push(1);
                }
                DynStorageSourceRecipe::OwnedView { head, leaf, mode } => {
                    pending.push(Work::StoredBorrowMode(mode));
                    pending.push(Work::DynTypePattern(leaf));
                    pending.push(Work::StoragePosition(head));
                    b.push(2);
                }
                DynStorageSourceRecipe::ReborrowedLeaf { source, head } => {
                    pending.push(Work::StoragePosition(head));
                    pending.push(Work::StoragePosition(source));
                    b.push(3);
                }
                DynStorageSourceRecipe::Static { leaf, mode } => {
                    pending.push(Work::StoredBorrowMode(mode));
                    pending.push(Work::DynTypePattern(leaf));
                    b.push(4);
                }
            },
            Work::DynStorageSourceBindingRecipe(value) => {
                for item in value.sources.iter().rev() {
                    pending.push(Work::DynStorageSourceRecipe(item));
                }
                pending.push(Work::Length(value.sources.len()));
                pending.push(Work::StoragePosition(&value.position));
            }
            Work::DynResultSourceRecipe(value) => match value {
                DynResultSourceRecipe::Value => {
                    b.push(0);
                }
                DynResultSourceRecipe::Create => {
                    b.push(1);
                }
                DynResultSourceRecipe::Give => {
                    b.push(2);
                }
                DynResultSourceRecipe::Borrow { sources, mutable } => {
                    pending.push(Work::Byte(u8::from(*mutable)));
                    for item in (sources).iter().rev() {
                        pending.push(Work::DynStorageSourceRecipe(item));
                    }
                    pending.push(Work::Length((sources).len()));
                    b.push(3);
                }
            },
            Work::DynDefinitionId(value) => {
                pending.push(Work::Word(value.source.0));
                pending.push(Work::Word(value.module.0));
            }
            Work::DynDefinitionScope(value) => match value {
                DynDefinitionScope::TraitDefault => {
                    b.push(0);
                }
                DynDefinitionScope::ImplOverride => {
                    b.push(1);
                }
            },
            Work::DynDefinitionDeclaration(value) => {
                pending.push(Work::DynSignatureRecipe(&value.signature));
                pending.push(Work::DynDefinitionScope(&value.scope));
                pending.push(Work::Word(value.member));
                pending.push(Work::LifeDeclarationIdentity(&value.identity));
                pending.push(Work::DynDefinitionId(&value.id));
                for item in value.instances.iter().rev() {
                    pending.push(Work::DynDefinitionSpecialization(item));
                }
                pending.push(Work::Length(value.instances.len()));
                for item in value.requirements.iter().rev() {
                    pending.push(Work::DynObligation(item));
                }
                pending.push(Work::Length(value.requirements.len()));
            }
            Work::DynMethodRecipe(value) => {
                if let Some(item) = &value.default_definition {
                    pending.push(Work::DynDefinitionId(item));
                }
                pending.push(Work::Byte(u8::from(value.default_definition.is_some())));
                pending.push(Work::DynSignatureRecipe(&value.signature));
                pending.push(Work::Word(value.member));
                for item in value.requirements.iter().rev() {
                    pending.push(Work::DynObligation(item));
                }
                pending.push(Work::Length(value.requirements.len()));
            }
            Work::DynFunctionInstance(value) => {
                for item in value.method_actuals.iter().rev() {
                    pending.push(Work::DynGenericActual(item));
                }
                pending.push(Work::Length(value.method_actuals.len()));
                for item in value.declaration_actuals.iter().rev() {
                    pending.push(Work::DynGenericActual(item));
                }
                pending.push(Work::Length(value.declaration_actuals.len()));
                pending.push(Work::DynTypeValue(&value.receiver));
                pending.push(Work::DynDefinitionId(&value.definition));
                for item in value.header_embedding.iter().rev() {
                    pending.push(Work::DynLifeEmbedding(item));
                }
                pending.push(Work::Length(value.header_embedding.len()));
                pending.push(Work::DynInstantiationScope(&value.scope));
            }
            Work::DynAssociatedValueRecipe(value) => match value {
                DynAssociatedValueRecipe::Type(value_0) => {
                    pending.push(Work::DynTypePattern(value_0));
                    b.push(0);
                }
                DynAssociatedValueRecipe::Constant { ty, value } => {
                    pending.push(Work::DynConstPattern(value));
                    pending.push(Work::DynTypePattern(ty));
                    b.push(1);
                }
            },
            Work::DynAssociatedDefinition(value) => {
                for item in value.requirements.iter().rev() {
                    pending.push(Work::DynObligation(item));
                }
                pending.push(Work::Length(value.requirements.len()));
                if let Some(item) = &value.value {
                    pending.push(Work::DynAssociatedValueRecipe(item));
                }
                pending.push(Work::Byte(u8::from(value.value.is_some())));
                pending.push(Work::Word(value.member));
                if let Some(item) = &value.carrier {
                    pending.push(Work::DynTypePattern(item));
                }
                pending.push(Work::Byte(u8::from(value.carrier.is_some())));
            }
            Work::DynAssociatedSource(value) => match value {
                DynAssociatedSource::TraitDefault {
                    declaration,
                    member,
                } => {
                    pending.push(Work::Word(*member));
                    pending.push(Work::LifeDeclarationId(declaration));
                    b.push(0);
                }
                DynAssociatedSource::ImplOverride {
                    declaration,
                    member,
                } => {
                    pending.push(Work::Word(*member));
                    pending.push(Work::LifeDeclarationId(declaration));
                    b.push(1);
                }
            },
            Work::DynAssociatedValue(value) => match value {
                DynAssociatedValue::Type(value_0) => {
                    pending.push(Work::DynTypeValue(value_0));
                    b.push(0);
                }
                DynAssociatedValue::Constant { ty, value } => {
                    pending.push(Work::Constant(value));
                    pending.push(Work::DynTypeValue(ty));
                    b.push(1);
                }
            },
            Work::DynAssociatedResolution(value) => {
                pending.push(Work::DynAssociatedValue(&value.value));
                pending.push(Work::DynAssociatedSource(&value.source));
                pending.push(Work::DynMember(&value.member));
            }
            Work::DynImplementationWitness(value) => {
                for item in value.associated.iter().rev() {
                    pending.push(Work::DynAssociatedResolution(item));
                }
                pending.push(Work::Length(value.associated.len()));
                for item in value.actuals.iter().rev() {
                    pending.push(Work::DynGenericActual(item));
                }
                pending.push(Work::Length(value.actuals.len()));
                pending.push(Work::LifeDeclarationId(&value.implementation));
                pending.push(Work::DynTraitApplication(&value.application));
                pending.push(Work::DynTypeValue(&value.receiver));
                for item in value.to_context.iter().rev() {
                    pending.push(Work::DynLifeEmbedding(item));
                }
                pending.push(Work::Length(value.to_context.len()));
                pending.push(Work::DynInstantiationScope(&value.scope));
            }
            Work::DynFunctionTarget(value) => match value {
                DynFunctionTarget::Core { module, source } => {
                    pending.push(Work::Word((source).0));
                    pending.push(Work::Word((module).0));
                    b.push(0);
                }
                DynFunctionTarget::Typed(value_0) => {
                    pending.push(Work::Word((value_0).0));
                    b.push(1);
                }
            },
            Work::DynDefinitionSpecialization(value) => {
                for item in value.method_actuals.iter().rev() {
                    pending.push(Work::DynGenericActual(item));
                }
                pending.push(Work::Length(value.method_actuals.len()));
                for item in value.declaration_actuals.iter().rev() {
                    pending.push(Work::DynGenericActual(item));
                }
                pending.push(Work::Length(value.declaration_actuals.len()));
                pending.push(Work::DynTypeValue(&value.receiver));
                pending.push(Work::DynFunctionTarget(&value.function));
            }
            Work::DynStructuralBound(value) => match value {
                DynStructuralBound::Copy => {
                    b.push(0);
                }
                DynStructuralBound::Value => {
                    b.push(1);
                }
                DynStructuralBound::SyncDrop => {
                    b.push(2);
                }
                DynStructuralBound::StoreElement => {
                    b.push(3);
                }
            },
            Work::SemanticDefinitionId(value) => {
                pending.push(Work::Word(value.index));
                pending.push(Work::Word(value.module.0));
            }
            Work::SemanticInstanceId(value) => {
                pending.push(Work::Word(value.index));
                pending.push(Work::Word(value.module.0));
            }
            Work::SemanticModule(value) => {
                for item in value.dyn_scopes.iter().rev() {
                    pending.push(Work::SemanticDynScopeBinding(item));
                }
                pending.push(Work::Length(value.dyn_scopes.len()));
                for item in value.sites.iter().rev() {
                    pending.push(Work::SemanticSite(item));
                }
                pending.push(Work::Length(value.sites.len()));
                for item in value.instances.iter().rev() {
                    pending.push(Work::SemanticInstance(item));
                }
                pending.push(Work::Length(value.instances.len()));
                for item in value.definitions.iter().rev() {
                    pending.push(Work::SemanticDefinition(item));
                }
                pending.push(Work::Length(value.definitions.len()));
            }
            Work::SemanticDynScopeBinding(value) => {
                pending.push(Work::SemanticDefinitionId(&value.scope));
                pending.push(Work::SemanticDynOwner(&value.owner));
            }
            Work::SemanticDynOwner(value) => match value {
                SemanticDynOwner::Declaration(value_0) => {
                    pending.push(Work::LifeDeclarationId(value_0));
                    b.push(0);
                }
                SemanticDynOwner::MethodSignature {
                    declaration,
                    member,
                } => {
                    pending.push(Work::Word(*member));
                    pending.push(Work::LifeDeclarationId(declaration));
                    b.push(1);
                }
                SemanticDynOwner::MethodDefinition(value_0) => {
                    pending.push(Work::DynDefinitionId(value_0));
                    b.push(2);
                }
            },
            Work::SemanticDefinition(value) => {
                pending.push(Work::SemanticDefinitionBody(&value.body));
                for item in value.environment.iter().rev() {
                    pending.push(Work::SemanticEnvironmentFormal(item));
                }
                pending.push(Work::Length(value.environment.len()));
                for item in value.requirements.iter().rev() {
                    pending.push(Work::SemanticRequirementRecipe(item));
                }
                pending.push(Work::Length(value.requirements.len()));
                for item in value.implicit_lifetimes.iter().rev() {
                    pending.push(Work::SemanticImplicitLifetime(item));
                }
                pending.push(Work::Length(value.implicit_lifetimes.len()));
                for item in value.parameters.iter().rev() {
                    pending.push(Work::SemanticFormal(item));
                }
                pending.push(Work::Length(value.parameters.len()));
                pending.push(Work::SemanticDefinitionKind(&value.kind));
                if let Some(item) = &value.parent {
                    pending.push(Work::SemanticDefinitionId(item));
                }
                pending.push(Work::Byte(u8::from(value.parent.is_some())));
                pending.push(Work::LifeDeclarationIdentity(&value.identity));
                pending.push(Work::SemanticDefinitionId(&value.id));
            }
            Work::SemanticDefinitionKind(value) => match value {
                SemanticDefinitionKind::Function => {
                    b.push(0);
                }
                SemanticDefinitionKind::Method => {
                    b.push(1);
                }
                SemanticDefinitionKind::Closure => {
                    b.push(2);
                }
                SemanticDefinitionKind::TaskEntry => {
                    b.push(3);
                }
                SemanticDefinitionKind::Nominal => {
                    b.push(4);
                }
                SemanticDefinitionKind::Scope => {
                    b.push(5);
                }
            },
            Work::SemanticDefinitionBody(value) => match value {
                SemanticDefinitionBody::Function { signature, sites } => {
                    for item in (sites).iter().rev() {
                        pending.push(Work::SemanticSiteRecipe(item));
                    }
                    pending.push(Work::Length((sites).len()));
                    pending.push(Work::SemanticSignatureRecipe(signature));
                    b.push(0);
                }
                SemanticDefinitionBody::Nominal {
                    declaration,
                    kind,
                    body,
                } => {
                    pending.push(Work::SemanticNominalBody(body));
                    pending.push(Work::LifeNominalKind(kind));
                    pending.push(Work::LifeDeclarationId(declaration));
                    b.push(1);
                }
                SemanticDefinitionBody::Scope(value_0) => {
                    pending.push(Work::SemanticScopeRecipe(value_0));
                    b.push(2);
                }
            },
            Work::SemanticScopeRecipe(value) => match value {
                SemanticScopeRecipe::Trait { declaration } => {
                    pending.push(Work::LifeDeclarationId(declaration));
                    b.push(0);
                }
                SemanticScopeRecipe::InherentImpl {
                    declaration,
                    self_type,
                } => {
                    pending.push(Work::SemanticTypeRecipe(self_type));
                    pending.push(Work::LifeDeclarationId(declaration));
                    b.push(1);
                }
                SemanticScopeRecipe::TraitImpl {
                    declaration,
                    self_type,
                    trait_application,
                } => {
                    pending.push(Work::SemanticTraitRecipe(trait_application));
                    pending.push(Work::SemanticTypeRecipe(self_type));
                    pending.push(Work::LifeDeclarationId(declaration));
                    b.push(2);
                }
            },
            Work::SemanticNominalBody(value) => match value {
                SemanticNominalBody::Product { fields } => {
                    for item in (fields).iter().rev() {
                        pending.push(Work::SemanticFieldRecipe(item));
                    }
                    pending.push(Work::Length((fields).len()));
                    b.push(0);
                }
                SemanticNominalBody::Enum { variants } => {
                    for item in (variants).iter().rev() {
                        pending.push(Work::SemanticVariantRecipe(item));
                    }
                    pending.push(Work::Length((variants).len()));
                    b.push(1);
                }
            },
            Work::SemanticFieldRecipe(value) => {
                pending.push(Work::SemanticTypeRecipe(&value.ty));
                pending.push(Work::String(&value.name));
            }
            Work::SemanticVariantRecipe(value) => {
                for item in value.fields.iter().rev() {
                    pending.push(Work::SemanticTypeRecipe(item));
                }
                pending.push(Work::Length(value.fields.len()));
                pending.push(Work::String(&value.name));
            }
            Work::SemanticFormal(value) => match value {
                SemanticFormal::Type { requirements } => {
                    for item in (requirements).iter().rev() {
                        pending.push(Work::SemanticRequirementRecipe(item));
                    }
                    pending.push(Work::Length((requirements).len()));
                    b.push(0);
                }
                SemanticFormal::Const { carrier } => {
                    pending.push(Work::SemanticTypeRecipe(carrier));
                    b.push(1);
                }
                SemanticFormal::Lifetime => {
                    b.push(2);
                }
            },
            Work::SemanticImplicitLifetime(value) => {
                for item in value.occurrence_path.iter().rev() {
                    pending.push(Work::Word(*item));
                }
                pending.push(Work::Length(value.occurrence_path.len()));
                pending.push(Work::Word(value.ordinal));
            }
            Work::SemanticEnvironmentFormal(value) => {
                pending.push(Work::SemanticActualRecipe(&value.source));
                pending.push(Work::SemanticFormal(&value.kind));
            }
            Work::SemanticInstance(value) => {
                if let Some(item) = &value.self_type {
                    pending.push(Work::SemanticTypeValue(item));
                }
                pending.push(Work::Byte(u8::from(value.self_type.is_some())));
                if let Some(item) = &value.environment {
                    pending.push(Work::SemanticEnvironmentInstance(item));
                }
                pending.push(Work::Byte(u8::from(value.environment.is_some())));
                for item in value.header_embedding.iter().rev() {
                    pending.push(Work::SemanticHeaderBinding(item));
                }
                pending.push(Work::Length(value.header_embedding.len()));
                for item in value.actuals.iter().rev() {
                    pending.push(Work::SemanticActual(item));
                }
                pending.push(Work::Length(value.actuals.len()));
                if let Some(item) = &value.parent {
                    pending.push(Work::SemanticInstanceId(item));
                }
                pending.push(Work::Byte(u8::from(value.parent.is_some())));
                pending.push(Work::SemanticLifetimeInstantiation(
                    &value.lifetime_instantiation,
                ));
                pending.push(Work::SemanticInstanceTarget(&value.target));
                pending.push(Work::SemanticDefinitionId(&value.definition));
                pending.push(Work::SemanticInstanceId(&value.id));
            }
            Work::SemanticLifetimeInstantiation(value) => match value {
                SemanticLifetimeInstantiation::Rigid => {
                    b.push(0);
                }
                SemanticLifetimeInstantiation::Partition { rigid } => {
                    pending.push(Work::SemanticInstanceId(rigid));
                    b.push(1);
                }
            },
            Work::SemanticFunctionTarget(value) => match value {
                SemanticFunctionTarget::Core { module, function } => {
                    pending.push(Work::Word((function).0));
                    pending.push(Work::Word((module).0));
                    b.push(0);
                }
                SemanticFunctionTarget::CoreClosure {
                    module,
                    creator,
                    expression,
                    occurrence,
                } => {
                    pending.push(Work::Word(*occurrence));
                    pending.push(Work::Word((expression).0));
                    pending.push(Work::Word((creator).0));
                    pending.push(Work::Word((module).0));
                    b.push(1);
                }
                SemanticFunctionTarget::Typed(value_0) => {
                    pending.push(Work::Word((value_0).0));
                    b.push(2);
                }
            },
            Work::SemanticInstanceTarget(value) => match value {
                SemanticInstanceTarget::Function(value_0) => {
                    pending.push(Work::SemanticFunctionTarget(value_0));
                    b.push(0);
                }
                SemanticInstanceTarget::Scope => {
                    b.push(1);
                }
            },
            Work::SemanticClosureTarget(value) => match value {
                SemanticClosureTarget::Core {
                    module,
                    creator,
                    expression,
                    occurrence,
                } => {
                    pending.push(Work::Word(*occurrence));
                    pending.push(Work::Word((expression).0));
                    pending.push(Work::Word((creator).0));
                    pending.push(Work::Word((module).0));
                    b.push(0);
                }
                SemanticClosureTarget::Typed(value_0) => {
                    pending.push(Work::Word((value_0).0));
                    b.push(1);
                }
            },
            Work::SemanticHeaderBinding(value) => {
                pending.push(Work::SemanticClassExpr(&value.actual));
                pending.push(Work::StorageBinder(&value.header));
            }
            Work::SemanticEnvironmentInstance(value) => {
                for item in value.actuals.iter().rev() {
                    pending.push(Work::SemanticActual(item));
                }
                pending.push(Work::Length(value.actuals.len()));
                pending.push(Work::SemanticClosureTarget(&value.closure));
            }
            Work::SemanticActual(value) => match value {
                SemanticActual::Type(value_0) => {
                    pending.push(Work::SemanticTypeValue(value_0));
                    b.push(0);
                }
                SemanticActual::Const { ty, value } => {
                    pending.push(Work::Constant(value));
                    pending.push(Work::SemanticTypeValue(ty));
                    b.push(1);
                }
                SemanticActual::Life(value_0) => {
                    pending.push(Work::SemanticClassExpr(value_0));
                    b.push(2);
                }
                SemanticActual::CoreRecipe { scope, recipe } => {
                    pending.push(Work::SemanticActualRecipe(recipe));
                    pending.push(Work::SemanticInstanceId(scope));
                    b.push(3);
                }
            },
            Work::SemanticCoreTypeSource(value) => match value {
                SemanticCoreTypeSource::OriginalRecipe(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(0);
                }
                SemanticCoreTypeSource::Expression {
                    module,
                    function,
                    expression,
                    occurrence,
                    recipe,
                } => {
                    pending.push(Work::SemanticTypeRecipe(recipe));
                    pending.push(Work::Word(*occurrence));
                    pending.push(Work::Word((expression).0));
                    pending.push(Work::Word((function).0));
                    pending.push(Work::Word((module).0));
                    b.push(1);
                }
            },
            Work::SemanticTypeValue(value) => match value {
                SemanticTypeValue::Resolved { ty, life, classes } => {
                    for item in (classes).iter().rev() {
                        pending.push(Work::SemanticClassExpr(item));
                    }
                    pending.push(Work::Length((classes).len()));
                    pending.push(Work::StorageLifeShape(life));
                    pending.push(Work::Type(ty));
                    b.push(0);
                }
                SemanticTypeValue::Core { scope, source } => {
                    pending.push(Work::SemanticCoreTypeSource(source));
                    pending.push(Work::SemanticInstanceId(scope));
                    b.push(1);
                }
            },
            Work::SemanticScopeRef(value) => match value {
                SemanticScopeRef::Ancestor(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(0);
                }
            },
            Work::SemanticClassExpr(value) => match value {
                SemanticClassExpr::Formal { scope, ordinal } => {
                    pending.push(Work::Word(*ordinal));
                    pending.push(Work::SemanticScopeRef(scope));
                    b.push(0);
                }
                SemanticClassExpr::Implicit { scope, ordinal } => {
                    pending.push(Work::Word(*ordinal));
                    pending.push(Work::SemanticScopeRef(scope));
                    b.push(1);
                }
                SemanticClassExpr::Bound { depth, index } => {
                    pending.push(Work::Word(*index));
                    pending.push(Work::Word(*depth));
                    b.push(2);
                }
                SemanticClassExpr::OuterOccurrence {
                    operand,
                    path,
                    mode,
                } => {
                    pending.push(Work::StoredBorrowMode(mode));
                    for item in (path).iter().rev() {
                        pending.push(Work::LifeTypeProjection(item));
                    }
                    pending.push(Work::Length((path).len()));
                    pending.push(Work::SemanticOperand(operand));
                    b.push(3);
                }
                SemanticClassExpr::IntrinsicClass {
                    operand,
                    path,
                    class,
                } => {
                    pending.push(Work::Word(*class));
                    for item in (path).iter().rev() {
                        pending.push(Work::LifeTypeProjection(item));
                    }
                    pending.push(Work::Length((path).len()));
                    pending.push(Work::SemanticOperand(operand));
                    b.push(4);
                }
            },
            Work::SemanticOperand(value) => match value {
                SemanticOperand::Parameter(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(0);
                }
                SemanticOperand::Capture(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(1);
                }
                SemanticOperand::Receiver => {
                    b.push(2);
                }
                SemanticOperand::Value(value_0) => {
                    pending.push(Work::Word((value_0).0));
                    b.push(3);
                }
                SemanticOperand::Local(value_0) => {
                    pending.push(Work::Word((value_0).0));
                    b.push(4);
                }
                SemanticOperand::TypeArgument { scope, ordinal } => {
                    pending.push(Work::Word(*ordinal));
                    pending.push(Work::SemanticScopeRef(scope));
                    b.push(5);
                }
                SemanticOperand::Environment(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(6);
                }
                SemanticOperand::CoreExpression {
                    expression,
                    occurrence,
                } => {
                    pending.push(Work::Word(*occurrence));
                    pending.push(Work::Word((expression).0));
                    b.push(7);
                }
            },
            Work::SemanticSiteRecipe(value) => {
                pending.push(Work::SemanticSiteRecipeKind(&value.kind));
                for item in value.occurrence_path.iter().rev() {
                    pending.push(Work::Word(*item));
                }
                pending.push(Work::Length(value.occurrence_path.len()));
                pending.push(Work::Word(value.id.0));
            }
            Work::SemanticSiteRecipeKind(value) => match value {
                SemanticSiteRecipeKind::Construct { result } => {
                    pending.push(Work::SemanticTypeRecipe(result));
                    b.push(0);
                }
                SemanticSiteRecipeKind::Call { target, actuals } => {
                    for item in (actuals).iter().rev() {
                        pending.push(Work::SemanticActualRecipe(item));
                    }
                    pending.push(Work::Length((actuals).len()));
                    pending.push(Work::SemanticDefinitionId(target));
                    b.push(1);
                }
                SemanticSiteRecipeKind::CallableCall { callee } => {
                    pending.push(Work::SemanticTypeRecipe(callee));
                    b.push(2);
                }
                SemanticSiteRecipeKind::TypeUse { expected } => {
                    pending.push(Work::SemanticTypeRecipe(expected));
                    b.push(3);
                }
                SemanticSiteRecipeKind::DeclareLocal { expected } => {
                    pending.push(Work::SemanticTypeRecipe(expected));
                    b.push(4);
                }
                SemanticSiteRecipeKind::Closure {
                    definition,
                    environment,
                } => {
                    for item in (environment).iter().rev() {
                        pending.push(Work::SemanticActualRecipe(item));
                    }
                    pending.push(Work::Length((environment).len()));
                    pending.push(Work::SemanticDefinitionId(definition));
                    b.push(5);
                }
            },
            Work::SemanticSite(value) => {
                pending.push(Work::SemanticSiteAction(&value.action));
                pending.push(Work::SemanticSiteAddress(&value.at));
                pending.push(Work::Word(value.recipe.0));
                pending.push(Work::SemanticInstanceId(&value.instance));
            }
            Work::SemanticSiteAddress(value) => match value {
                SemanticSiteAddress::Core {
                    expression,
                    occurrence,
                    role,
                } => {
                    pending.push(Work::SemanticEmissionRole(role));
                    pending.push(Work::Word(*occurrence));
                    pending.push(Work::Word((expression).0));
                    b.push(0);
                }
                SemanticSiteAddress::Typed {
                    block,
                    instruction,
                    edge,
                } => {
                    pending.push(Work::SemanticSiteEdge(edge));
                    pending.push(Work::Word(*instruction));
                    pending.push(Work::Word((block).0));
                    b.push(1);
                }
            },
            Work::SemanticEmissionRole(value) => match value {
                SemanticEmissionRole::Construct => {
                    b.push(0);
                }
                SemanticEmissionRole::Call => {
                    b.push(1);
                }
                SemanticEmissionRole::Ascription => {
                    b.push(2);
                }
                SemanticEmissionRole::Binding => {
                    b.push(3);
                }
                SemanticEmissionRole::Closure => {
                    b.push(4);
                }
            },
            Work::SemanticSiteEdge(value) => match value {
                SemanticSiteEdge::Before => {
                    b.push(0);
                }
                SemanticSiteEdge::After => {
                    b.push(1);
                }
            },
            Work::SemanticSiteAction(value) => match value {
                SemanticSiteAction::Construct { result, value } => {
                    pending.push(Work::SemanticTypeValue(value));
                    pending.push(Work::SemanticOperand(result));
                    b.push(0);
                }
                SemanticSiteAction::Call { target, actuals } => {
                    for item in (actuals).iter().rev() {
                        pending.push(Work::SemanticActual(item));
                    }
                    pending.push(Work::Length((actuals).len()));
                    pending.push(Work::SemanticFunctionTarget(target));
                    b.push(1);
                }
                SemanticSiteAction::CallableCall { callee } => {
                    pending.push(Work::SemanticOperand(callee));
                    b.push(2);
                }
                SemanticSiteAction::TypeUse { subject, expected } => {
                    pending.push(Work::SemanticTypeValue(expected));
                    pending.push(Work::SemanticOperand(subject));
                    b.push(3);
                }
                SemanticSiteAction::DeclareLocal { local, expected } => {
                    pending.push(Work::SemanticTypeValue(expected));
                    pending.push(Work::SemanticLocalTarget(local));
                    b.push(4);
                }
                SemanticSiteAction::Closure {
                    result,
                    closure,
                    environment,
                } => {
                    for item in (environment).iter().rev() {
                        pending.push(Work::SemanticActual(item));
                    }
                    pending.push(Work::Length((environment).len()));
                    pending.push(Work::SemanticClosureTarget(closure));
                    pending.push(Work::SemanticOperand(result));
                    b.push(5);
                }
            },
            Work::SemanticLocalTarget(value) => match value {
                SemanticLocalTarget::Core {
                    statement,
                    occurrence,
                } => {
                    pending.push(Work::Word(*occurrence));
                    pending.push(Work::Word((statement).0));
                    b.push(0);
                }
                SemanticLocalTarget::Typed(value_0) => {
                    pending.push(Work::Word((value_0).0));
                    b.push(1);
                }
            },
            Work::SemanticRecipeOperand(value) => match value {
                SemanticRecipeOperand::TypeFormal(value_0) => {
                    pending.push(Work::SemanticFormalRef(value_0));
                    b.push(0);
                }
                SemanticRecipeOperand::Environment { scope, ordinal } => {
                    pending.push(Work::Word(*ordinal));
                    pending.push(Work::SemanticScopeRef(scope));
                    b.push(1);
                }
                SemanticRecipeOperand::Parameter(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(2);
                }
                SemanticRecipeOperand::Receiver => {
                    b.push(3);
                }
            },
            Work::SemanticFormalRef(value) => {
                pending.push(Work::Word(value.ordinal));
                pending.push(Work::SemanticScopeRef(&value.scope));
            }
            Work::SemanticLifeRecipe(value) => match value {
                SemanticLifeRecipe::Formal(value_0) => {
                    pending.push(Work::SemanticFormalRef(value_0));
                    b.push(0);
                }
                SemanticLifeRecipe::Implicit { scope, ordinal } => {
                    pending.push(Work::Word(*ordinal));
                    pending.push(Work::SemanticScopeRef(scope));
                    b.push(1);
                }
                SemanticLifeRecipe::Environment { scope, ordinal } => {
                    pending.push(Work::Word(*ordinal));
                    pending.push(Work::SemanticScopeRef(scope));
                    b.push(2);
                }
                SemanticLifeRecipe::Bound { depth, index } => {
                    pending.push(Work::Word(*index));
                    pending.push(Work::Word(*depth));
                    b.push(3);
                }
                SemanticLifeRecipe::Invocation(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(4);
                }
                SemanticLifeRecipe::OuterOccurrence {
                    operand,
                    path,
                    mode,
                } => {
                    pending.push(Work::StoredBorrowMode(mode));
                    for item in (path).iter().rev() {
                        pending.push(Work::LifeTypeProjection(item));
                    }
                    pending.push(Work::Length((path).len()));
                    pending.push(Work::SemanticRecipeOperand(operand));
                    b.push(5);
                }
                SemanticLifeRecipe::IntrinsicClass {
                    operand,
                    path,
                    class,
                } => {
                    pending.push(Work::Word(*class));
                    for item in (path).iter().rev() {
                        pending.push(Work::LifeTypeProjection(item));
                    }
                    pending.push(Work::Length((path).len()));
                    pending.push(Work::SemanticRecipeOperand(operand));
                    b.push(6);
                }
            },
            Work::SemanticOuterLifeRecipe(value) => match value {
                SemanticOuterLifeRecipe::Class(value_0) => {
                    pending.push(Work::SemanticLifeRecipe(value_0));
                    b.push(0);
                }
                SemanticOuterLifeRecipe::Static => {
                    b.push(1);
                }
            },
            Work::SemanticTypeRecipe(value) => match value {
                SemanticTypeRecipe::Atom(value_0) => {
                    pending.push(Work::Type(value_0));
                    b.push(0);
                }
                SemanticTypeRecipe::Exact(value_0) => {
                    pending.push(Work::SemanticExactTypeRecipe(value_0));
                    b.push(1);
                }
                SemanticTypeRecipe::Formal(value_0) => {
                    pending.push(Work::SemanticFormalRef(value_0));
                    b.push(2);
                }
                SemanticTypeRecipe::Environment { scope, ordinal } => {
                    pending.push(Work::Word(*ordinal));
                    pending.push(Work::SemanticScopeRef(scope));
                    b.push(3);
                }
                SemanticTypeRecipe::SelfType { scope } => {
                    pending.push(Work::SemanticScopeRef(scope));
                    b.push(4);
                }
                SemanticTypeRecipe::Nominal {
                    definition,
                    arguments,
                } => {
                    for item in (arguments).iter().rev() {
                        pending.push(Work::SemanticActualRecipe(item));
                    }
                    pending.push(Work::Length((arguments).len()));
                    pending.push(Work::SemanticDefinitionId(definition));
                    b.push(5);
                }
                SemanticTypeRecipe::AssociatedType {
                    receiver,
                    application,
                    member,
                } => {
                    pending.push(Work::Word(*member));
                    pending.push(Work::SemanticTraitRecipe(application));
                    pending.push(Work::SemanticTypeRecipe(receiver));
                    b.push(6);
                }
                SemanticTypeRecipe::Tuple(value_0) => {
                    for item in (value_0).iter().rev() {
                        pending.push(Work::SemanticTypeRecipe(item));
                    }
                    pending.push(Work::Length((value_0).len()));
                    b.push(7);
                }
                SemanticTypeRecipe::Array { element, length } => {
                    pending.push(Work::SemanticConstRecipe(length));
                    pending.push(Work::SemanticTypeRecipe(element));
                    b.push(8);
                }
                SemanticTypeRecipe::Slice(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(9);
                }
                SemanticTypeRecipe::Address {
                    mutable,
                    lifetime,
                    inner,
                } => {
                    pending.push(Work::SemanticTypeRecipe(inner));
                    pending.push(Work::SemanticOuterLifeRecipe(lifetime));
                    pending.push(Work::Byte(u8::from(*mutable)));
                    b.push(10);
                }
                SemanticTypeRecipe::Desc { lifetime, inner } => {
                    pending.push(Work::SemanticTypeRecipe(inner));
                    pending.push(Work::SemanticOuterLifeRecipe(lifetime));
                    b.push(11);
                }
                SemanticTypeRecipe::Managed(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(12);
                }
                SemanticTypeRecipe::OwnedStore(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(13);
                }
                SemanticTypeRecipe::Mutex(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(14);
                }
                SemanticTypeRecipe::MutexGuard { payload, retained } => {
                    pending.push(Work::SemanticRetainedShapeRecipe(retained));
                    pending.push(Work::SemanticTypeRecipe(payload));
                    b.push(15);
                }
                SemanticTypeRecipe::Registration(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(16);
                }
                SemanticTypeRecipe::AsyncRegistration(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(17);
                }
                SemanticTypeRecipe::Task { result, retained } => {
                    pending.push(Work::SemanticRetainedShapeRecipe(retained));
                    pending.push(Work::SemanticTypeRecipe(result));
                    b.push(18);
                }
                SemanticTypeRecipe::ErrorUnion { error, result } => {
                    pending.push(Work::SemanticTypeRecipe(result));
                    pending.push(Work::SemanticTypeRecipe(error));
                    b.push(19);
                }
                SemanticTypeRecipe::Qualified { storage, life } => {
                    pending.push(Work::SemanticStorageShapeRecipe(life));
                    pending.push(Work::SemanticTypeRecipe(storage));
                    b.push(20);
                }
                SemanticTypeRecipe::Code { kind, signature } => {
                    pending.push(Work::SemanticSignatureRecipe(signature));
                    pending.push(Work::SemanticCodeKind(kind));
                    b.push(21);
                }
                SemanticTypeRecipe::Closure {
                    definition,
                    arguments,
                    environment,
                } => {
                    for item in (environment).iter().rev() {
                        pending.push(Work::SemanticActualRecipe(item));
                    }
                    pending.push(Work::Length((environment).len()));
                    for item in (arguments).iter().rev() {
                        pending.push(Work::SemanticActualRecipe(item));
                    }
                    pending.push(Work::Length((arguments).len()));
                    pending.push(Work::SemanticDefinitionId(definition));
                    b.push(22);
                }
                SemanticTypeRecipe::Dyn { owned, interface } => {
                    pending.push(Work::SemanticDynInterfaceRecipe(interface));
                    pending.push(Work::Byte(u8::from(*owned)));
                    b.push(23);
                }
                SemanticTypeRecipe::Opaque {
                    definition,
                    arguments,
                    hidden,
                } => {
                    pending.push(Work::SemanticTypeRecipe(hidden));
                    for item in (arguments).iter().rev() {
                        pending.push(Work::SemanticActualRecipe(item));
                    }
                    pending.push(Work::Length((arguments).len()));
                    pending.push(Work::SemanticDefinitionId(definition));
                    b.push(24);
                }
                SemanticTypeRecipe::AnonymousSum(value_0) => {
                    for item in (value_0).iter().rev() {
                        pending.push(Work::SemanticTypeRecipe(item));
                    }
                    pending.push(Work::Length((value_0).len()));
                    b.push(25);
                }
                SemanticTypeRecipe::UnresolvedNamed(value_0) => {
                    pending.push(Work::String(value_0));
                    b.push(26);
                }
                SemanticTypeRecipe::Unsupported(value_0) => {
                    pending.push(Work::String(value_0));
                    b.push(27);
                }
            },
            Work::SemanticExactTypeRecipe(value) => {
                for item in value.classes.iter().rev() {
                    pending.push(Work::SemanticLifeRecipe(item));
                }
                pending.push(Work::Length(value.classes.len()));
                pending.push(Work::StorageLifeShape(&value.life));
                pending.push(Work::Type(&value.ty));
            }
            Work::SemanticCodeKind(value) => match value {
                SemanticCodeKind::FunctionPointer => {
                    b.push(0);
                }
                SemanticCodeKind::BorrowedFunctionPointer => {
                    b.push(1);
                }
                SemanticCodeKind::ForeignCallback => {
                    b.push(2);
                }
            },
            Work::SemanticActualRecipe(value) => match value {
                SemanticActualRecipe::Type(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(0);
                }
                SemanticActualRecipe::Const { carrier, value } => {
                    pending.push(Work::SemanticConstRecipe(value));
                    pending.push(Work::SemanticTypeRecipe(carrier));
                    b.push(1);
                }
                SemanticActualRecipe::Life(value_0) => {
                    pending.push(Work::SemanticLifeRecipe(value_0));
                    b.push(2);
                }
            },
            Work::SemanticClassSequenceRecipe(value) => {
                for item in value.parts.iter().rev() {
                    pending.push(Work::SemanticClassSequencePart(item));
                }
                pending.push(Work::Length(value.parts.len()));
            }
            Work::SemanticClassSequencePart(value) => match value {
                SemanticClassSequencePart::One(value_0) => {
                    pending.push(Work::SemanticLifeRecipe(value_0));
                    b.push(0);
                }
                SemanticClassSequencePart::AllClasses { ty, path } => {
                    for item in (path).iter().rev() {
                        pending.push(Work::LifeTypeProjection(item));
                    }
                    pending.push(Work::Length((path).len()));
                    pending.push(Work::SemanticTypeRecipe(ty));
                    b.push(1);
                }
            },
            Work::SemanticStorageShapeRecipe(value) => {
                for item in value.nominal.iter().rev() {
                    pending.push(Work::SemanticNominalPart(item));
                }
                pending.push(Work::Length(value.nominal.len()));
                for item in value.retained.iter().rev() {
                    pending.push(Work::SemanticRetainedPart(item));
                }
                pending.push(Work::Length(value.retained.len()));
                for item in value.slots.iter().rev() {
                    pending.push(Work::SemanticSlotRecipe(item));
                }
                pending.push(Work::Length(value.slots.len()));
                pending.push(Work::SemanticClassSequenceRecipe(&value.domain));
            }
            Work::SemanticSlotRecipe(value) => {
                pending.push(Work::SemanticLifeRecipe(&value.class));
                pending.push(Work::StoredBorrowMode(&value.mode));
                for item in value.path.iter().rev() {
                    pending.push(Work::PlaceProjection(item));
                }
                pending.push(Work::Length(value.path.len()));
            }
            Work::SemanticRetainedPart(value) => match value {
                SemanticRetainedPart::One {
                    path,
                    child_class,
                    parent,
                } => {
                    pending.push(Work::SemanticLifeRecipe(parent));
                    pending.push(Work::Word(*child_class));
                    for item in (path).iter().rev() {
                        pending.push(Work::LifeTypeProjection(item));
                    }
                    pending.push(Work::Length((path).len()));
                    b.push(0);
                }
                SemanticRetainedPart::All { path, parents } => {
                    pending.push(Work::SemanticClassSequenceRecipe(parents));
                    for item in (path).iter().rev() {
                        pending.push(Work::LifeTypeProjection(item));
                    }
                    pending.push(Work::Length((path).len()));
                    b.push(1);
                }
            },
            Work::SemanticNominalPart(value) => match value {
                SemanticNominalPart::One {
                    path,
                    declaration,
                    argument,
                    class,
                } => {
                    pending.push(Work::SemanticLifeRecipe(class));
                    pending.push(Work::Word(*argument));
                    pending.push(Work::LifeDeclarationId(declaration));
                    for item in (path).iter().rev() {
                        pending.push(Work::LifeTypeProjection(item));
                    }
                    pending.push(Work::Length((path).len()));
                    b.push(0);
                }
                SemanticNominalPart::FromType { path, ty, classes } => {
                    pending.push(Work::SemanticClassSequenceRecipe(classes));
                    pending.push(Work::SemanticTypeRecipe(ty));
                    for item in (path).iter().rev() {
                        pending.push(Work::LifeTypeProjection(item));
                    }
                    pending.push(Work::Length((path).len()));
                    b.push(1);
                }
            },
            Work::SemanticRetainedShapeRecipe(value) => {
                for item in value.nominal.iter().rev() {
                    pending.push(Work::SemanticNominalPart(item));
                }
                pending.push(Work::Length(value.nominal.len()));
                for item in value.retained.iter().rev() {
                    pending.push(Work::SemanticRetainedPart(item));
                }
                pending.push(Work::Length(value.retained.len()));
                for item in value.ports.iter().rev() {
                    pending.push(Work::SemanticPortPart(item));
                }
                pending.push(Work::Length(value.ports.len()));
                pending.push(Work::SemanticClassSequenceRecipe(&value.domain));
            }
            Work::SemanticPortPart(value) => match value {
                SemanticPortPart::One { leaf, mode, class } => {
                    pending.push(Work::SemanticLifeRecipe(class));
                    pending.push(Work::StoredBorrowMode(mode));
                    pending.push(Work::SemanticTypeRecipe(leaf));
                    b.push(0);
                }
                SemanticPortPart::FromType { ty, classes } => {
                    pending.push(Work::SemanticClassSequenceRecipe(classes));
                    pending.push(Work::SemanticTypeRecipe(ty));
                    b.push(1);
                }
            },
            Work::SemanticConstRecipe(value) => match value {
                SemanticConstRecipe::Value(value_0) => {
                    pending.push(Work::Constant(value_0));
                    b.push(0);
                }
                SemanticConstRecipe::IntegerLiteral(value_0) => {
                    pending.push(Work::String(value_0));
                    b.push(1);
                }
                SemanticConstRecipe::FloatLiteral(value_0) => {
                    pending.push(Work::String(value_0));
                    b.push(2);
                }
                SemanticConstRecipe::Formal(value_0) => {
                    pending.push(Work::SemanticFormalRef(value_0));
                    b.push(3);
                }
                SemanticConstRecipe::Environment { scope, ordinal } => {
                    pending.push(Work::Word(*ordinal));
                    pending.push(Work::SemanticScopeRef(scope));
                    b.push(4);
                }
                SemanticConstRecipe::Unary {
                    op,
                    operand_type,
                    value,
                } => {
                    pending.push(Work::SemanticConstRecipe(value));
                    pending.push(Work::SemanticTypeRecipe(operand_type));
                    pending.push(Work::Byte(*op as u8));
                    b.push(5);
                }
                SemanticConstRecipe::Binary {
                    op,
                    operand_type,
                    left,
                    right,
                } => {
                    pending.push(Work::SemanticConstRecipe(right));
                    pending.push(Work::SemanticConstRecipe(left));
                    pending.push(Work::SemanticTypeRecipe(operand_type));
                    pending.push(Work::Byte(*op as u8));
                    b.push(6);
                }
                SemanticConstRecipe::Convert {
                    source_type,
                    target_type,
                    value,
                } => {
                    pending.push(Work::SemanticConstRecipe(value));
                    pending.push(Work::SemanticTypeRecipe(target_type));
                    pending.push(Work::SemanticTypeRecipe(source_type));
                    b.push(7);
                }
                SemanticConstRecipe::AssociatedConst {
                    receiver,
                    application,
                    member,
                } => {
                    pending.push(Work::Word(*member));
                    pending.push(Work::SemanticTraitRecipe(application));
                    pending.push(Work::SemanticTypeRecipe(receiver));
                    b.push(8);
                }
            },
            Work::SemanticTraitRecipe(value) => {
                for item in value.arguments.iter().rev() {
                    pending.push(Work::SemanticActualRecipe(item));
                }
                pending.push(Work::Length(value.arguments.len()));
                pending.push(Work::LifeDeclarationId(&value.declaration));
            }
            Work::SemanticRequirementRecipe(value) => match value {
                SemanticRequirementRecipe::Structural { ty, bound } => {
                    pending.push(Work::DynStructuralBound(bound));
                    pending.push(Work::SemanticTypeRecipe(ty));
                    b.push(0);
                }
                SemanticRequirementRecipe::Trait { ty, application } => {
                    pending.push(Work::SemanticTraitRecipe(application));
                    pending.push(Work::SemanticTypeRecipe(ty));
                    b.push(1);
                }
                SemanticRequirementRecipe::Callable {
                    ty,
                    kind,
                    signature,
                } => {
                    pending.push(Work::SemanticSignatureRecipe(signature));
                    pending.push(Work::Byte(match kind {
                        CallableKind::FunctionPointer => 0,
                        CallableKind::Fn => 1,
                        CallableKind::FnMut => 2,
                        CallableKind::FnOnce => 3,
                    }));
                    pending.push(Work::SemanticTypeRecipe(ty));
                    b.push(2);
                }
                SemanticRequirementRecipe::TypeEqual { left, right } => {
                    pending.push(Work::SemanticTypeRecipe(right));
                    pending.push(Work::SemanticTypeRecipe(left));
                    b.push(3);
                }
                SemanticRequirementRecipe::ConstEqual {
                    carrier,
                    left,
                    right,
                } => {
                    pending.push(Work::SemanticConstRecipe(right));
                    pending.push(Work::SemanticConstRecipe(left));
                    pending.push(Work::SemanticTypeRecipe(carrier));
                    b.push(4);
                }
                SemanticRequirementRecipe::LifeEqual { left, right } => {
                    pending.push(Work::SemanticLifeRecipe(right));
                    pending.push(Work::SemanticLifeRecipe(left));
                    b.push(5);
                }
                SemanticRequirementRecipe::Outlives { longer, shorter } => {
                    pending.push(Work::SemanticLifeRecipe(shorter));
                    pending.push(Work::SemanticLifeRecipe(longer));
                    b.push(6);
                }
                SemanticRequirementRecipe::ForAll {
                    lifetimes,
                    requirements,
                } => {
                    for item in (requirements).iter().rev() {
                        pending.push(Work::SemanticRequirementRecipe(item));
                    }
                    pending.push(Work::Length((requirements).len()));
                    pending.push(Work::Word(*lifetimes));
                    b.push(7);
                }
            },
            Work::SemanticSignatureRecipe(value) => {
                pending.push(Work::CallKind(&value.call_kind));
                pending.push(Work::PassingMode(&value.result_mode));
                pending.push(Work::SemanticTypeRecipe(&value.result));
                for item in value.parameters.iter().rev() {
                    pending.push(Work::SemanticParameterRecipe(item));
                }
                pending.push(Work::Length(value.parameters.len()));
                pending.push(Work::Word(value.invocation_lifetimes));
                pending.push(Work::Word(value.bound_lifetimes));
            }
            Work::SemanticParameterRecipe(value) => {
                pending.push(Work::PassingMode(&value.mode));
                pending.push(Work::SemanticTypeRecipe(&value.ty));
            }
            Work::SemanticDynContextRecipe(value) => match value {
                SemanticDynContextRecipe::Closed => {
                    b.push(0);
                }
                SemanticDynContextRecipe::Type(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(1);
                }
            },
            Work::SemanticDynInterfaceRecipe(value) => {
                for item in value.associated.iter().rev() {
                    pending.push(Work::SemanticAssociatedBindingRecipe(item));
                }
                pending.push(Work::Length(value.associated.len()));
                pending.push(Work::SemanticDynContextRecipe(&value.context));
                pending.push(Work::SemanticTraitRecipe(&value.application));
            }
            Work::SemanticAssociatedBindingRecipe(value) => {
                pending.push(Work::SemanticAssociatedRecipe(&value.value));
                pending.push(Work::Word(value.member));
                pending.push(Work::SemanticTraitRecipe(&value.application));
            }
            Work::SemanticAssociatedRecipe(value) => match value {
                SemanticAssociatedRecipe::Type(value_0) => {
                    pending.push(Work::SemanticTypeRecipe(value_0));
                    b.push(0);
                }
                SemanticAssociatedRecipe::Const { carrier, value } => {
                    pending.push(Work::SemanticConstRecipe(value));
                    pending.push(Work::SemanticTypeRecipe(carrier));
                    b.push(1);
                }
            },
            Work::DynSelfPattern(value) => {
                for item in value.arguments.iter().rev() {
                    pending.push(Work::DynPatternActual(item));
                }
                pending.push(Work::Length(value.arguments.len()));
                pending.push(Work::LifeDeclarationId(&value.declaration));
            }
            Work::DynPatternActual(value) => match value {
                DynPatternActual::Type(value_0) => {
                    pending.push(Work::DynTypePattern(value_0));
                    b.push(0);
                }
                DynPatternActual::Const(value_0) => {
                    pending.push(Work::DynConstPattern(value_0));
                    b.push(1);
                }
                DynPatternActual::Life(value_0) => {
                    pending.push(Work::DynLifePattern(value_0));
                    b.push(2);
                }
            },
            Work::DynConstPattern(value) => match value {
                DynConstPattern::Value(value_0) => {
                    pending.push(Work::Constant(value_0));
                    b.push(0);
                }
                DynConstPattern::Parameter(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(1);
                }
                DynConstPattern::MethodParameter(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(2);
                }
                DynConstPattern::Unary { op, value, ty } => {
                    pending.push(Work::DynTypePattern(ty));
                    pending.push(Work::DynConstPattern(value));
                    pending.push(Work::Byte(*op as u8));
                    b.push(3);
                }
                DynConstPattern::Binary {
                    op,
                    left,
                    right,
                    ty,
                } => {
                    pending.push(Work::DynTypePattern(ty));
                    pending.push(Work::DynConstPattern(right));
                    pending.push(Work::DynConstPattern(left));
                    pending.push(Work::Byte(*op as u8));
                    b.push(4);
                }
                DynConstPattern::Convert { value, ty } => {
                    pending.push(Work::DynTypePattern(ty));
                    pending.push(Work::DynConstPattern(value));
                    b.push(5);
                }
                DynConstPattern::AssociatedProjection {
                    receiver,
                    application,
                    member,
                } => {
                    pending.push(Work::Word(*member));
                    pending.push(Work::DynSelfPattern(application));
                    pending.push(Work::DynTypePattern(receiver));
                    b.push(6);
                }
                DynConstPattern::IntegerLiteral(value_0) => {
                    pending.push(Work::String(value_0));
                    b.push(7);
                }
                DynConstPattern::FloatLiteral(value_0) => {
                    pending.push(Work::String(value_0));
                    b.push(8);
                }
            },
            Work::DynTypePattern(value) => match value {
                DynTypePattern::Concrete(value_0) => {
                    pending.push(Work::Type(value_0));
                    b.push(0);
                }
                DynTypePattern::Parameter(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(1);
                }
                DynTypePattern::Nominal(value_0) => {
                    pending.push(Work::DynSelfPattern(value_0));
                    b.push(2);
                }
                DynTypePattern::Address {
                    mutable,
                    lifetime,
                    inner,
                } => {
                    pending.push(Work::DynTypePattern(inner));
                    if let Some(item) = lifetime {
                        pending.push(Work::DynLifePattern(item));
                    }
                    pending.push(Work::Byte(u8::from((lifetime).is_some())));
                    pending.push(Work::Byte(u8::from(*mutable)));
                    b.push(3);
                }
                DynTypePattern::Tuple(value_0) => {
                    for item in (value_0).iter().rev() {
                        pending.push(Work::DynTypePattern(item));
                    }
                    pending.push(Work::Length((value_0).len()));
                    b.push(4);
                }
                DynTypePattern::Array { element, length } => {
                    pending.push(Work::DynConstPattern(length));
                    pending.push(Work::DynTypePattern(element));
                    b.push(5);
                }
                DynTypePattern::Slice(value_0) => {
                    pending.push(Work::DynTypePattern(value_0));
                    b.push(6);
                }
                DynTypePattern::Managed(value_0) => {
                    pending.push(Work::DynTypePattern(value_0));
                    b.push(7);
                }
                DynTypePattern::Desc { lifetime, inner } => {
                    pending.push(Work::DynTypePattern(inner));
                    if let Some(item) = lifetime {
                        pending.push(Work::DynLifePattern(item));
                    }
                    pending.push(Work::Byte(u8::from((lifetime).is_some())));
                    b.push(8);
                }
                DynTypePattern::OwnedStore(value_0) => {
                    pending.push(Work::DynTypePattern(value_0));
                    b.push(9);
                }
                DynTypePattern::ErrorUnion { error, result } => {
                    pending.push(Work::DynTypePattern(result));
                    pending.push(Work::DynTypePattern(error));
                    b.push(10);
                }
                DynTypePattern::SelfType => {
                    b.push(11);
                }
                DynTypePattern::MethodParameter(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(12);
                }
                DynTypePattern::AssociatedProjection {
                    receiver,
                    application,
                    member,
                } => {
                    pending.push(Work::Word(*member));
                    pending.push(Work::DynSelfPattern(application));
                    pending.push(Work::DynTypePattern(receiver));
                    b.push(13);
                }
                DynTypePattern::Semantic { scope, ty } => {
                    pending.push(Work::SemanticTypeRecipe(ty));
                    pending.push(Work::SemanticDefinitionId(scope));
                    b.push(14);
                }
            },
            Work::StoredBorrowMode(value) => match value {
                StoredBorrowMode::SharedAddress => {
                    b.push(0);
                }
                StoredBorrowMode::ExclusiveAddress => {
                    b.push(1);
                }
                StoredBorrowMode::ReadOnlyDescription => {
                    b.push(2);
                }
                StoredBorrowMode::WritableDescription => {
                    b.push(3);
                }
            },
            Work::LifeNominalKind(value) => match value {
                LifeNominalKind::Record => {
                    b.push(0);
                }
                LifeNominalKind::Package => {
                    b.push(1);
                }
                LifeNominalKind::Resource => {
                    b.push(2);
                }
                LifeNominalKind::Enum => {
                    b.push(3);
                }
            },
            Work::LifeTypeProjection(value) => match value {
                LifeTypeProjection::Place(value_0) => {
                    pending.push(Work::PlaceProjection(value_0));
                    b.push(0);
                }
                LifeTypeProjection::TypeArgument(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(1);
                }
                LifeTypeProjection::Pointee => {
                    b.push(2);
                }
                LifeTypeProjection::Element => {
                    b.push(3);
                }
                LifeTypeProjection::Parameter(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(4);
                }
                LifeTypeProjection::Result => {
                    b.push(5);
                }
                LifeTypeProjection::Port(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(6);
                }
                LifeTypeProjection::AssociatedType(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(7);
                }
            },
            Work::PlaceProjection(value) => match value {
                PlaceProjection::Field(value_0) => {
                    pending.push(Work::Word(*value_0));
                    b.push(0);
                }
                PlaceProjection::Variant(value_0) => {
                    pending.push(Work::VariantProjection(value_0));
                    b.push(1);
                }
                PlaceProjection::ErrorArm(value_0) => {
                    pending.push(Work::Byte(u8::from(*value_0)));
                    b.push(2);
                }
            },
            Work::VariantProjection(value) => {
                pending.push(Work::Word(value.index));
                pending.push(Work::Word(value.variant));
            }
        }
    }
}
pub(super) fn encode_dyn_type_value(b: &mut impl FingerprintSink, value: &DynTypeValue) {
    encode_recipe(b, Work::DynTypeValue(value));
}
pub(super) fn encode_dyn_implicit_life_declaration(
    b: &mut impl FingerprintSink,
    value: &DynImplicitLifeDeclaration,
) {
    encode_recipe(b, Work::DynImplicitLifeDeclaration(value));
}
pub(super) fn encode_dyn_obligation(b: &mut impl FingerprintSink, value: &DynObligation) {
    encode_recipe(b, Work::DynObligation(value));
}
pub(super) fn encode_dyn_definition_declaration(
    b: &mut impl FingerprintSink,
    value: &DynDefinitionDeclaration,
) {
    encode_recipe(b, Work::DynDefinitionDeclaration(value));
}
pub(super) fn encode_dyn_method_recipe(b: &mut impl FingerprintSink, value: &DynMethodRecipe) {
    encode_recipe(b, Work::DynMethodRecipe(value));
}
pub(super) fn encode_dyn_function_instance(
    b: &mut impl FingerprintSink,
    value: &DynFunctionInstance,
) {
    encode_recipe(b, Work::DynFunctionInstance(value));
}
pub(super) fn encode_dyn_associated_definition(
    b: &mut impl FingerprintSink,
    value: &DynAssociatedDefinition,
) {
    encode_recipe(b, Work::DynAssociatedDefinition(value));
}
pub(super) fn encode_dyn_implementation_witness(
    b: &mut impl FingerprintSink,
    value: &DynImplementationWitness,
) {
    encode_recipe(b, Work::DynImplementationWitness(value));
}
pub(super) fn encode_semantic_module(b: &mut impl FingerprintSink, value: &SemanticModule) {
    encode_recipe(b, Work::SemanticModule(value));
}
pub(super) fn encode_dyn_self_pattern(b: &mut impl FingerprintSink, value: &DynSelfPattern) {
    encode_recipe(b, Work::DynSelfPattern(value));
}
pub(super) fn encode_dyn_type_pattern(b: &mut impl FingerprintSink, value: &DynTypePattern) {
    encode_recipe(b, Work::DynTypePattern(value));
}
