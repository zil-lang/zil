//! Complete raw dynamic recipe identity. This does not certify recipes.
use super::*;
pub(super) fn encode_dyn_type_value(b: &mut impl FingerprintSink, value: &DynTypeValue) {
    encode_type(b, &value.ty);
    encode_storage_shape(b, &value.life);
    encode_len(b, (&value.classes).len());
    for item in &value.classes {
        encode_storage_binder(b, item);
    }
}
pub(super) fn encode_dyn_life_pattern(b: &mut impl FingerprintSink, value: &DynLifePattern) {
    match value {
        DynLifePattern::DeclarationParameter(value_0) => {
            b.push(0);
            encode_u32(b, *value_0);
        }
        DynLifePattern::MethodParameter(value_0) => {
            b.push(1);
            encode_u32(b, *value_0);
        }
        DynLifePattern::Bound { depth, index } => {
            b.push(2);
            encode_u32(b, *depth);
            encode_u32(b, *index);
        }
        DynLifePattern::Invocation(value_0) => {
            b.push(3);
            encode_u32(b, *value_0);
        }
    }
}
pub(super) fn encode_dyn_obligation(b: &mut impl FingerprintSink, value: &DynObligation) {
    match value {
        DynObligation::Trait {
            receiver,
            application,
        } => {
            b.push(0);
            encode_dyn_type_pattern(b, receiver);
            encode_dyn_self_pattern(b, application);
        }
        DynObligation::Callable {
            receiver,
            kind,
            quantified_lifetimes,
            signature,
        } => {
            b.push(1);
            encode_dyn_type_pattern(b, receiver);
            b.push(match kind {
                CallableKind::FunctionPointer => 0,
                CallableKind::Fn => 1,
                CallableKind::FnMut => 2,
                CallableKind::FnOnce => 3,
            });
            encode_u32(b, *quantified_lifetimes);
            encode_dyn_signature_recipe(b, signature);
        }
    }
}
pub(super) fn encode_dyn_parameter_recipe(
    b: &mut impl FingerprintSink,
    value: &DynParameterRecipe,
) {
    encode_dyn_type_pattern(b, &value.ty);
    encode_dyn_passing_mode(b, &value.mode);
}
pub(super) fn encode_dyn_signature_recipe(
    b: &mut impl FingerprintSink,
    value: &DynSignatureRecipe,
) {
    encode_len(b, (&value.parameters).len());
    for item in &value.parameters {
        encode_dyn_parameter_recipe(b, item);
    }
    encode_dyn_type_pattern(b, &value.result);
    encode_dyn_passing_mode(b, &value.result_mode);
    encode_dyn_call_kind(b, &value.call_kind);
    encode_u32(b, *&value.invocation_binder_count);
    encode_dyn_contract_recipe(b, &value.contract);
}
pub(super) fn encode_dyn_requirement_recipe(
    b: &mut impl FingerprintSink,
    value: &DynRequirementRecipe,
) {
    match value {
        DynRequirementRecipe::Read(value_0) => {
            b.push(0);
            encode_storage_position(b, value_0);
        }
        DynRequirementRecipe::Write(value_0) => {
            b.push(1);
            encode_storage_position(b, value_0);
        }
        DynRequirementRecipe::Manage(value_0) => {
            b.push(2);
            encode_storage_position(b, value_0);
        }
        DynRequirementRecipe::Apart(value_0, value_1) => {
            b.push(3);
            encode_storage_position(b, value_0);
            encode_storage_position(b, value_1);
        }
        DynRequirementRecipe::State {
            position,
            ty,
            variant,
        } => {
            b.push(4);
            encode_storage_position(b, position);
            encode_dyn_type_pattern(b, ty);
            encode_u32(b, *variant);
        }
    }
}
pub(super) fn encode_dyn_guarantee_recipe(
    b: &mut impl FingerprintSink,
    value: &DynGuaranteeRecipe,
) {
    match value {
        DynGuaranteeRecipe::StorageSources(value_0) => {
            b.push(0);
            encode_storage_source_binding(b, value_0);
        }
        DynGuaranteeRecipe::ManageResult => {
            b.push(1);
        }
        DynGuaranteeRecipe::State {
            position,
            ty,
            variant,
        } => {
            b.push(2);
            encode_storage_position(b, position);
            encode_dyn_type_pattern(b, ty);
            encode_u32(b, *variant);
        }
    }
}
pub(super) fn encode_dyn_effect_recipe(b: &mut impl FingerprintSink, value: &DynEffectRecipe) {
    match value {
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
            b.push(8);
            encode_storage_position(b, value_0);
        }
        DynEffectRecipe::Cleanup(value_0) => {
            b.push(9);
            encode_dyn_type_pattern(b, value_0);
        }
    }
}
pub(super) fn encode_dyn_cleanup_recipe(b: &mut impl FingerprintSink, value: &DynCleanupRecipe) {
    b.push(u8::from(*&value.environment));
    encode_len(b, (&value.positions).len());
    for item in &value.positions {
        encode_storage_position(b, item);
    }
    encode_dyn_call_access(b, &value.access);
    encode_len(b, (&value.effects).len());
    for item in &value.effects {
        encode_dyn_effect_recipe(b, item);
    }
}
pub(super) fn encode_dyn_contract_recipe(b: &mut impl FingerprintSink, value: &DynContractRecipe) {
    b.push(u8::from((&value.requires).is_some()));
    if let Some(item) = &value.requires {
        encode_len(b, (item).len());
        for item in item {
            encode_dyn_requirement_recipe(b, item);
        }
    }
    b.push(u8::from((&value.guarantees).is_some()));
    if let Some(item) = &value.guarantees {
        encode_len(b, (item).len());
        for item in item {
            encode_dyn_guarantee_recipe(b, item);
        }
    }
    b.push(u8::from((&value.hold).is_some()));
    if let Some(item) = &value.hold {
        encode_len(b, (item).len());
        for item in item {
            encode_storage_position(b, item);
        }
    }
    b.push(u8::from((&value.call).is_some()));
    if let Some(item) = &value.call {
        encode_dyn_call_access(b, item);
    }
    b.push(u8::from((&value.result).is_some()));
    if let Some(item) = &value.result {
        encode_dyn_result_source(b, item);
    }
    b.push(u8::from((&value.cleanup).is_some()));
    if let Some(item) = &value.cleanup {
        encode_dyn_cleanup_recipe(b, item);
    }
    b.push(u8::from((&value.effects).is_some()));
    if let Some(item) = &value.effects {
        encode_len(b, (item).len());
        for item in item {
            encode_dyn_effect_recipe(b, item);
        }
    }
    b.push(u8::from((&value.resume).is_some()));
    if let Some(item) = &value.resume {
        encode_dyn_resume_bound(b, item);
    }
    b.push(u8::from((&value.threads).is_some()));
    if let Some(item) = &value.threads {
        encode_dyn_thread_requirements(b, item);
    }
}
pub(super) fn encode_dyn_definition_id(b: &mut impl FingerprintSink, value: &DynDefinitionId) {
    encode_u32(b, &value.module.0);
    encode_u32(b, &value.source.0);
}
pub(super) fn encode_dyn_definition_scope(
    b: &mut impl FingerprintSink,
    value: &DynDefinitionScope,
) {
    match value {
        DynDefinitionScope::TraitDefault => {
            b.push(0);
        }
        DynDefinitionScope::ImplOverride => {
            b.push(1);
        }
    }
}
pub(super) fn encode_dyn_definition_declaration(
    b: &mut impl FingerprintSink,
    value: &DynDefinitionDeclaration,
) {
    encode_dyn_definition_id(b, &value.id);
    encode_declaration_identity(b, &value.identity);
    encode_u32(b, *&value.member);
    encode_dyn_definition_scope(b, &value.scope);
    encode_dyn_signature_recipe(b, &value.signature);
}
pub(super) fn encode_dyn_method_recipe(b: &mut impl FingerprintSink, value: &DynMethodRecipe) {
    encode_u32(b, *&value.member);
    encode_dyn_signature_recipe(b, &value.signature);
    b.push(u8::from((&value.default_definition).is_some()));
    if let Some(item) = &value.default_definition {
        encode_dyn_definition_id(b, item);
    }
}
pub(super) fn encode_dyn_function_instance(
    b: &mut impl FingerprintSink,
    value: &DynFunctionInstance,
) {
    encode_dyn_definition_id(b, &value.definition);
    encode_dyn_type_value(b, &value.receiver);
    encode_len(b, (&value.declaration_actuals).len());
    for item in &value.declaration_actuals {
        encode_dyn_generic_actual(b, item);
    }
    encode_len(b, (&value.method_actuals).len());
    for item in &value.method_actuals {
        encode_dyn_generic_actual(b, item);
    }
}
pub(super) fn encode_dyn_associated_value_recipe(
    b: &mut impl FingerprintSink,
    value: &DynAssociatedValueRecipe,
) {
    match value {
        DynAssociatedValueRecipe::Type(value_0) => {
            b.push(0);
            encode_dyn_type_pattern(b, value_0);
        }
        DynAssociatedValueRecipe::Constant { ty, value } => {
            b.push(1);
            encode_dyn_type_pattern(b, ty);
            encode_dyn_const_pattern(b, value);
        }
    }
}
pub(super) fn encode_dyn_associated_definition(
    b: &mut impl FingerprintSink,
    value: &DynAssociatedDefinition,
) {
    encode_u32(b, *&value.member);
    b.push(u8::from((&value.value).is_some()));
    if let Some(item) = &value.value {
        encode_dyn_associated_value_recipe(b, item);
    }
    encode_len(b, (&value.requirements).len());
    for item in &value.requirements {
        encode_dyn_obligation(b, item);
    }
}
pub(super) fn encode_dyn_associated_source(
    b: &mut impl FingerprintSink,
    value: &DynAssociatedSource,
) {
    match value {
        DynAssociatedSource::TraitDefault {
            declaration,
            member,
        } => {
            b.push(0);
            encode_declaration_id(b, declaration);
            encode_u32(b, *member);
        }
        DynAssociatedSource::ImplOverride {
            declaration,
            member,
        } => {
            b.push(1);
            encode_declaration_id(b, declaration);
            encode_u32(b, *member);
        }
    }
}
pub(super) fn encode_dyn_associated_value(
    b: &mut impl FingerprintSink,
    value: &DynAssociatedValue,
) {
    match value {
        DynAssociatedValue::Type(value_0) => {
            b.push(0);
            encode_dyn_type_value(b, value_0);
        }
        DynAssociatedValue::Constant { ty, value } => {
            b.push(1);
            encode_dyn_type_value(b, ty);
            encode_constant(b, value);
        }
    }
}
pub(super) fn encode_dyn_associated_resolution(
    b: &mut impl FingerprintSink,
    value: &DynAssociatedResolution,
) {
    encode_dyn_member(b, &value.member);
    encode_dyn_associated_source(b, &value.source);
    encode_dyn_associated_value(b, &value.value);
}
pub(super) fn encode_dyn_implementation_witness(
    b: &mut impl FingerprintSink,
    value: &DynImplementationWitness,
) {
    encode_dyn_type_value(b, &value.receiver);
    encode_dyn_trait_application(b, &value.application);
    encode_declaration_id(b, &value.implementation);
    encode_len(b, (&value.actuals).len());
    for item in &value.actuals {
        encode_dyn_generic_actual(b, item);
    }
    encode_len(b, (&value.associated).len());
    for item in &value.associated {
        encode_dyn_associated_resolution(b, item);
    }
}
pub(super) fn encode_storage_source_binding(
    b: &mut impl FingerprintSink,
    value: &StorageSourceBinding,
) {
    encode_storage_position(b, &value.position);
    encode_len(b, (&value.sources).len());
    for item in &value.sources {
        encode_storage_position(b, item);
    }
}
