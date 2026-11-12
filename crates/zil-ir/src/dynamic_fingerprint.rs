//! Complete raw module metadata fingerprints, not cross-request Type identity.
use super::*;
pub(super) fn encode_dyn_interface_id(b: &mut impl FingerprintSink, value: &DynInterfaceId) {
    encode_u32(b, value.module.0);
    encode_u32(b, value.index);
}
pub(super) fn encode_dyn_vtable_id(b: &mut impl FingerprintSink, value: &DynVtableId) {
    encode_u32(b, value.module.0);
    encode_u32(b, value.index);
}
pub(super) fn encode_dyn_declaration(b: &mut impl FingerprintSink, value: &DynDeclaration) {
    b.push(u8::from((&value.direct_supertraits).is_some()));
    if let Some(item) = &value.direct_supertraits {
        encode_len(b, (item).len());
        for item in item {
            encode_dyn_self_pattern(b, item);
        }
    }
    b.push(u8::from((&value.requirements).is_some()));
    if let Some(item) = &value.requirements {
        encode_len(b, (item).len());
        for item in item {
            encode_dyn_obligation(b, item);
        }
    }
    b.push(u8::from((&value.associated_definitions).is_some()));
    if let Some(item) = &value.associated_definitions {
        encode_len(b, (item).len());
        for item in item {
            encode_dyn_associated_definition(b, item);
        }
    }
    b.push(u8::from((&value.method_signatures).is_some()));
    if let Some(item) = &value.method_signatures {
        encode_len(b, (item).len());
        for item in item {
            encode_dyn_method_recipe(b, item);
        }
    }
    b.push(u8::from((&value.method_definitions).is_some()));
    if let Some(item) = &value.method_definitions {
        encode_len(b, (item).len());
        for item in item {
            encode_dyn_definition_declaration(b, item);
        }
    }
    encode_declaration_id(b, &value.id);
    encode_declaration_identity(b, &value.identity);
    encode_dyn_declaration_kind(b, &value.kind);
    b.push(u8::from(value.self_pattern.is_some()));
    if let Some(pattern) = &value.self_pattern {
        encode_dyn_type_pattern(b, pattern);
    }
    b.push(u8::from(value.implemented_trait.is_some()));
    if let Some(pattern) = &value.implemented_trait {
        encode_dyn_self_pattern(b, pattern);
    }
    encode_len(b, value.parameters.len());
    for value in &value.parameters {
        encode_parameter_kind(b, value);
    }
    encode_len(b, value.members.len());
    for value in &value.members {
        encode_dyn_member_declaration(b, value);
    }
}
pub(super) fn encode_dyn_member_declaration(
    b: &mut impl FingerprintSink,
    value: &DynMemberDeclaration,
) {
    encode_text(b, &value.name);
    encode_dyn_member_kind(b, &value.kind);
}
pub(super) fn encode_dyn_trait_application(
    b: &mut impl FingerprintSink,
    value: &DynTraitApplication,
) {
    encode_declaration_id(b, &value.declaration);
    encode_len(b, value.actuals.len());
    for value in &value.actuals {
        encode_dyn_generic_actual(b, value);
    }
}
pub(super) fn encode_dyn_member(b: &mut impl FingerprintSink, value: &DynMember) {
    encode_dyn_trait_application(b, &value.application);
    encode_u32(b, value.ordinal);
}
pub(super) fn encode_dyn_associated_type(b: &mut impl FingerprintSink, value: &DynAssociatedType) {
    encode_dyn_member(b, &value.member);
    encode_type(b, &value.ty);
    encode_storage_shape(b, &value.life);
    encode_len(b, value.classes.len());
    for class in &value.classes {
        encode_storage_binder(b, class);
    }
}
pub(super) fn encode_dyn_associated_constant(
    b: &mut impl FingerprintSink,
    value: &DynAssociatedConstant,
) {
    encode_dyn_member(b, &value.member);
    encode_type(b, &value.ty);
    encode_constant(b, &value.value);
}
pub(super) fn encode_dyn_receiver(b: &mut impl FingerprintSink, value: &DynReceiver) {
    b.push(u8::from(value.mutable));
    encode_dyn_passing_mode(b, &value.mode);
}
pub(super) fn encode_dyn_slot(b: &mut impl FingerprintSink, value: &DynSlot) {
    encode_dyn_member(b, &value.member);
    encode_len(b, value.actuals.len());
    for value in &value.actuals {
        encode_dyn_generic_actual(b, value);
    }
    encode_dyn_receiver(b, &value.receiver);
    encode_len(b, value.parameters.len());
    for value in &value.parameters {
        encode_type(b, value);
    }
    encode_len(b, value.parameter_modes.len());
    for value in &value.parameter_modes {
        encode_dyn_passing_mode(b, value);
    }
    encode_type(b, &value.result);
    encode_dyn_passing_mode(b, &value.result_mode);
    encode_dyn_call_kind(b, &value.call_kind);
    b.push(u8::from(value.storage_life.is_some()));
    if let Some(value) = &value.storage_life {
        encode_required_storage_contract(b, value);
    }
    encode_dyn_contract(b, &value.contract);
}
pub(super) fn encode_dyn_interface(b: &mut impl FingerprintSink, value: &DynInterface) {
    b.push(u8::from(value.context.is_some()));
    if let Some(context) = &value.context {
        encode_type(b, context);
    }
    b.push(u8::from(value.owning_cleanup.is_some()));
    if let Some(cleanup) = &value.owning_cleanup {
        encode_dyn_cleanup_contract(b, cleanup);
    }

    encode_dyn_interface_id(b, &value.id);
    encode_dyn_trait_application(b, &value.application);
    encode_len(b, value.inherited.len());
    for value in &value.inherited {
        encode_dyn_trait_application(b, value);
    }
    encode_len(b, value.associated_types.len());
    for value in &value.associated_types {
        encode_dyn_associated_type(b, value);
    }
    encode_len(b, value.associated_constants.len());
    for value in &value.associated_constants {
        encode_dyn_associated_constant(b, value);
    }
    encode_retained_shape(b, &value.retained_life);
    encode_len(b, value.slots.len());
    for value in &value.slots {
        encode_dyn_slot(b, value);
    }
}
pub(super) fn encode_dyn_entry(b: &mut impl FingerprintSink, value: &DynEntry) {
    encode_u32(b, value.slot);
    encode_u32(b, value.function.0);
}
pub(super) fn encode_dyn_vtable(b: &mut impl FingerprintSink, value: &DynVtable) {
    b.push(u8::from((&value.self_life).is_some()));
    if let Some(item) = &value.self_life {
        encode_storage_shape(b, item);
    }
    b.push(u8::from((&value.implementation_witnesses).is_some()));
    if let Some(item) = &value.implementation_witnesses {
        encode_len(b, (item).len());
        for item in item {
            encode_dyn_implementation_witness(b, item);
        }
    }
    encode_dyn_vtable_id(b, &value.id);
    encode_dyn_interface_id(b, &value.interface);
    encode_type(b, &value.self_type);
    encode_len(b, value.self_classes.len());
    for binder in &value.self_classes {
        encode_storage_binder(b, binder);
    }
    encode_declaration_id(b, &value.implementation);
    encode_len(b, value.actuals.len());
    for value in &value.actuals {
        encode_dyn_generic_actual(b, value);
    }
    encode_len(b, value.ports.len());
    for value in &value.ports {
        encode_u32(b, value.port);
        encode_len(b, value.sources.len());
        for source in &value.sources {
            encode_storage_position(b, source);
        }
    }
    encode_len(b, value.entries.len());
    for value in &value.entries {
        encode_dyn_entry(b, value);
    }
    b.push(u8::from(value.cleanup.is_some()));
    if let Some(value) = &value.cleanup {
        encode_dyn_entry(b, value);
    }
    b.push(match value.owned_storage {
        None => 0,
        Some(DynOwnedStorage::Heap) => 1,
    });
}
pub(super) fn encode_dyn_call_access(b: &mut impl FingerprintSink, value: &DynCallAccess) {
    encode_len(b, value.reads.len());
    for value in &value.reads {
        encode_storage_position(b, value);
    }
    encode_len(b, value.writes.len());
    for value in &value.writes {
        encode_storage_position(b, value);
    }
}
pub(super) fn encode_dyn_cleanup_contract(
    b: &mut impl FingerprintSink,
    value: &DynCleanupContract,
) {
    b.push(u8::from(value.environment));
    encode_len(b, value.positions.len());
    for value in &value.positions {
        encode_storage_position(b, value);
    }
    encode_dyn_call_access(b, &value.access);
    encode_len(b, value.effects.len());
    for value in &value.effects {
        encode_dyn_effect(b, value);
    }
}
pub(super) fn encode_dyn_thread_requirements(
    b: &mut impl FingerprintSink,
    value: &DynThreadRequirements,
) {
    b.push(u8::from(value.transfer));
    b.push(u8::from(value.share));
}
pub(super) fn encode_dyn_contract(b: &mut impl FingerprintSink, value: &DynContract) {
    b.push(u8::from(value.requires.is_some()));
    if let Some(value) = &value.requires {
        encode_len(b, (value).len());
        for value in value {
            encode_dyn_requirement(b, value);
        }
    }
    b.push(u8::from(value.guarantees.is_some()));
    if let Some(value) = &value.guarantees {
        encode_len(b, (value).len());
        for value in value {
            encode_dyn_guarantee(b, value);
        }
    }
    b.push(u8::from(value.hold.is_some()));
    if let Some(value) = &value.hold {
        encode_len(b, (value).len());
        for value in value {
            encode_storage_position(b, value);
        }
    }
    b.push(u8::from(value.call.is_some()));
    if let Some(value) = &value.call {
        encode_dyn_call_access(b, value);
    }
    b.push(u8::from(value.result.is_some()));
    if let Some(value) = &value.result {
        encode_dyn_result_source(b, value);
    }
    b.push(u8::from(value.cleanup.is_some()));
    if let Some(value) = &value.cleanup {
        encode_dyn_cleanup_contract(b, value);
    }
    b.push(u8::from(value.effects.is_some()));
    if let Some(value) = &value.effects {
        encode_len(b, (value).len());
        for value in value {
            encode_dyn_effect(b, value);
        }
    }
    b.push(u8::from(value.resume.is_some()));
    if let Some(value) = &value.resume {
        encode_dyn_resume_bound(b, value);
    }
    b.push(u8::from(value.threads.is_some()));
    if let Some(value) = &value.threads {
        encode_dyn_thread_requirements(b, value);
    }
}
pub(super) fn encode_dyn_declaration_kind(
    b: &mut impl FingerprintSink,
    value: &DynDeclarationKind,
) {
    match value {
        DynDeclarationKind::Trait => {
            b.push(0);
        }
        DynDeclarationKind::Impl => {
            b.push(1);
        }
    }
}
pub(super) fn encode_dyn_member_kind(b: &mut impl FingerprintSink, value: &DynMemberKind) {
    match value {
        DynMemberKind::Type => {
            b.push(0);
        }
        DynMemberKind::Constant => {
            b.push(1);
        }
        DynMemberKind::Method(value_0) => {
            b.push(2);
            encode_len(b, value_0.len());
            for value in value_0 {
                encode_parameter_kind(b, value);
            }
        }
    }
}
pub(super) fn encode_dyn_generic_actual(b: &mut impl FingerprintSink, value: &DynGenericActual) {
    match value {
        DynGenericActual::QualifiedType(value) => {
            b.push(3);
            encode_dyn_type_value(b, value);
        }
        DynGenericActual::Type(value_0) => {
            b.push(0);
            encode_type(b, value_0);
        }
        DynGenericActual::Const(value_0) => {
            b.push(1);
            encode_constant(b, value_0);
        }
        DynGenericActual::Life(value_0) => {
            b.push(2);
            encode_storage_binder(b, value_0);
        }
    }
}
pub(super) fn encode_dyn_requirement(b: &mut impl FingerprintSink, value: &DynRequirement) {
    match value {
        DynRequirement::Read(value_0) => {
            b.push(0);
            encode_storage_position(b, value_0);
        }
        DynRequirement::Write(value_0) => {
            b.push(1);
            encode_storage_position(b, value_0);
        }
        DynRequirement::Manage(value_0) => {
            b.push(2);
            encode_storage_position(b, value_0);
        }
        DynRequirement::Apart(value_0, value_1) => {
            b.push(3);
            encode_storage_position(b, value_0);
            encode_storage_position(b, value_1);
        }
        DynRequirement::State {
            position,
            ty,
            variant,
        } => {
            b.push(4);
            encode_storage_position(b, position);
            encode_type(b, ty);
            encode_u32(b, *variant);
        }
    }
}
pub(super) fn encode_dyn_guarantee(b: &mut impl FingerprintSink, value: &DynGuarantee) {
    match value {
        DynGuarantee::StorageSources(binding) => {
            b.push(2);
            encode_storage_position(b, &binding.position);
            encode_len(b, binding.sources.len());
            for source in &binding.sources {
                encode_storage_source(b, source);
            }
        }
        DynGuarantee::ManageResult => {
            b.push(0);
        }
        DynGuarantee::State {
            position,
            ty,
            variant,
        } => {
            b.push(1);
            encode_storage_position(b, position);
            encode_type(b, ty);
            encode_u32(b, *variant);
        }
    }
}
pub(super) fn encode_dyn_effect(b: &mut impl FingerprintSink, value: &DynEffect) {
    match value {
        DynEffect::Alloc => {
            b.push(0);
        }
        DynEffect::Io => {
            b.push(1);
        }
        DynEffect::Block => {
            b.push(2);
        }
        DynEffect::Suspend => {
            b.push(3);
        }
        DynEffect::Spawn => {
            b.push(4);
        }
        DynEffect::Ffi => {
            b.push(5);
        }
        DynEffect::Reenter => {
            b.push(6);
        }
        DynEffect::Panic => {
            b.push(7);
        }
        DynEffect::Call(value_0) => {
            b.push(8);
            encode_storage_position(b, value_0);
        }
        DynEffect::Cleanup(value_0) => {
            b.push(9);
            encode_type(b, value_0);
        }
    }
}
pub(super) fn encode_dyn_result_source(b: &mut impl FingerprintSink, value: &DynResultSource) {
    match value {
        DynResultSource::Value => {
            b.push(0);
        }
        DynResultSource::Create => {
            b.push(1);
        }
        DynResultSource::Give => {
            b.push(2);
        }
        DynResultSource::Borrow { sources, mutable } => {
            b.push(3);
            encode_len(b, sources.len());
            for value in sources {
                encode_storage_source(b, value);
            }
            b.push(u8::from(*mutable));
        }
    }
}
pub(super) fn encode_dyn_resume_bound(b: &mut impl FingerprintSink, value: &DynResumeBound) {
    match value {
        DynResumeBound::Local => {
            b.push(0);
        }
        DynResumeBound::Transfer => {
            b.push(1);
        }
    }
}
pub(super) fn encode_dyn_passing_mode(b: &mut impl FingerprintSink, value: &PassingMode) {
    match value {
        PassingMode::Copy => {
            b.push(0);
        }
        PassingMode::Move => {
            b.push(1);
        }
        PassingMode::Describe => {
            b.push(2);
        }
        PassingMode::Create => {
            b.push(3);
        }
        PassingMode::Give => {
            b.push(4);
        }
        PassingMode::Borrow => {
            b.push(5);
        }
        PassingMode::BorrowMut => {
            b.push(6);
        }
        PassingMode::Infer => {
            b.push(7);
        }
    }
}
pub(super) fn encode_dyn_call_kind(b: &mut impl FingerprintSink, value: &CallKind) {
    match value {
        CallKind::Direct => {
            b.push(0);
        }
        CallKind::Suspend => {
            b.push(1);
        }
    }
}
