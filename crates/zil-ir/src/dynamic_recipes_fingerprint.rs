//! Complete raw recipe identity with an explicit traversal stack.
use super::*;
enum Work<'a> {
    Byte(u8),
    Word(u32),
    Length(usize),
    DynTypeValue(&'a DynTypeValue),
    DynLifePattern(&'a DynLifePattern),
    DynObligation(&'a DynObligation),
    DynParameterRecipe(&'a DynParameterRecipe),
    DynSignatureRecipe(&'a DynSignatureRecipe),
    DynRequirementRecipe(&'a DynRequirementRecipe),
    DynGuaranteeRecipe(&'a DynGuaranteeRecipe),
    DynEffectRecipe(&'a DynEffectRecipe),
    DynCleanupRecipe(&'a DynCleanupRecipe),
    DynContractRecipe(&'a DynContractRecipe),
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
    DynSelfPattern(&'a DynSelfPattern),
    DynPatternActual(&'a DynPatternActual),
    DynConstPattern(&'a DynConstPattern),
    DynTypePattern(&'a DynTypePattern),
    StorageSourceBinding(&'a StorageSourceBinding),
    String(&'a String),
    Type(&'a Type),
    StorageLifeShape(&'a StorageLifeShape),
    StorageBinder(&'a StorageBinder),
    DynGenericActual(&'a DynGenericActual),
    StoragePosition(&'a StoragePosition),
    StorageSource(&'a StorageSource),
    DynCallAccess(&'a DynCallAccess),
    DynResultSource(&'a DynResultSource),
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
            Work::StorageSource(value) => encode_storage_source(b, value),
            Work::DynCallAccess(value) => encode_dyn_call_access(b, value),
            Work::DynResultSource(value) => encode_dyn_result_source(b, value),
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
                    pending.push(Work::StorageSourceBinding(value_0));
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
                    pending.push(Work::DynResultSource(item));
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
                DynTypePattern::Desc(value_0) => {
                    pending.push(Work::DynTypePattern(value_0));
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
            },
            Work::StorageSourceBinding(value) => {
                for item in value.sources.iter().rev() {
                    pending.push(Work::StorageSource(item));
                }
                pending.push(Work::Length(value.sources.len()));
                pending.push(Work::StoragePosition(&value.position));
            }
        }
    }
}
pub(super) fn encode_dyn_type_value(b: &mut impl FingerprintSink, value: &DynTypeValue) {
    encode_recipe(b, Work::DynTypeValue(value));
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
pub(super) fn encode_dyn_self_pattern(b: &mut impl FingerprintSink, value: &DynSelfPattern) {
    encode_recipe(b, Work::DynSelfPattern(value));
}
pub(super) fn encode_dyn_type_pattern(b: &mut impl FingerprintSink, value: &DynTypePattern) {
    encode_recipe(b, Work::DynTypePattern(value));
}
