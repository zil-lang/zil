//! Bounded recipe transport using explicit grammar stacks.
use super::*;
enum Write<'a> {
    K0(&'a DynTypeValue, usize),
    K1(&'a Type, usize),
    K2(&'a StorageLifeShape, usize),
    K3(&'a Vec<StorageBinder>, usize),
    K4(&'a StorageBinder, usize),
    K5(&'a DynLifePattern, usize),
    K6(&'a u32, usize),
    K7(&'a DynObligation, usize),
    K8(&'a DynTypePattern, usize),
    K9(&'a DynStructuralBound, usize),
    K10(&'a DynSelfPattern, usize),
    K11(&'a CallableKind, usize),
    K12(&'a DynSignatureRecipe, usize),
    K13(&'a DynParameterRecipe, usize),
    K14(&'a PassingMode, usize),
    K15(&'a Vec<DynParameterRecipe>, usize),
    K16(&'a CallKind, usize),
    K17(&'a DynContractRecipe, usize),
    K18(&'a DynRequirementRecipe, usize),
    K19(&'a StoragePosition, usize),
    K20(&'a DynGuaranteeRecipe, usize),
    K21(&'a StorageSourceBinding, usize),
    K22(&'a DynEffectRecipe, usize),
    K23(&'a DynCleanupRecipe, usize),
    K24(&'a bool, usize),
    K25(&'a Vec<StoragePosition>, usize),
    K26(&'a DynCallAccess, usize),
    K27(&'a Vec<DynEffectRecipe>, usize),
    K28(&'a Option<Vec<DynRequirementRecipe>>, usize),
    K29(&'a Vec<DynRequirementRecipe>, usize),
    K30(&'a Option<Vec<DynGuaranteeRecipe>>, usize),
    K31(&'a Vec<DynGuaranteeRecipe>, usize),
    K32(&'a Option<Vec<StoragePosition>>, usize),
    K33(&'a Option<DynCallAccess>, usize),
    K34(&'a Option<DynResultSource>, usize),
    K35(&'a DynResultSource, usize),
    K36(&'a Option<DynCleanupRecipe>, usize),
    K37(&'a Option<Vec<DynEffectRecipe>>, usize),
    K38(&'a Option<DynResumeBound>, usize),
    K39(&'a DynResumeBound, usize),
    K40(&'a Option<DynThreadRequirements>, usize),
    K41(&'a DynThreadRequirements, usize),
    K42(&'a DynDefinitionId, usize),
    K43(&'a ModuleId, usize),
    K44(&'a NodeId, usize),
    K45(&'a DynDefinitionScope, usize),
    K46(&'a DynDefinitionDeclaration, usize),
    K47(&'a Vec<DynObligation>, usize),
    K48(&'a Vec<DynDefinitionSpecialization>, usize),
    K49(&'a DynDefinitionSpecialization, usize),
    K50(&'a LifeDeclarationIdentity, usize),
    K51(&'a DynMethodRecipe, usize),
    K52(&'a Option<DynDefinitionId>, usize),
    K53(&'a DynFunctionInstance, usize),
    K54(&'a Vec<DynGenericActual>, usize),
    K55(&'a DynGenericActual, usize),
    K56(&'a DynAssociatedValueRecipe, usize),
    K57(&'a DynConstPattern, usize),
    K58(&'a DynAssociatedDefinition, usize),
    K59(&'a Option<DynTypePattern>, usize),
    K60(&'a Option<DynAssociatedValueRecipe>, usize),
    K61(&'a DynAssociatedSource, usize),
    K62(&'a LifeDeclarationId, usize),
    K63(&'a DynAssociatedValue, usize),
    K64(&'a Constant, usize),
    K65(&'a DynAssociatedResolution, usize),
    K66(&'a DynMember, usize),
    K67(&'a DynImplementationWitness, usize),
    K68(&'a DynTraitApplication, usize),
    K69(&'a Vec<DynAssociatedResolution>, usize),
    K70(&'a DynFunctionTarget, usize),
    K71(&'a FunctionId, usize),
    K72(&'a Vec<DynPatternActual>, usize),
    K73(&'a DynPatternActual, usize),
    K74(&'a UnaryOp, usize),
    K75(&'a BinaryOp, usize),
    K76(&'a String, usize),
    K77(&'a Option<DynLifePattern>, usize),
    K78(&'a Vec<DynTypePattern>, usize),
}
#[derive(Clone, Copy)]
enum Kind {
    K0,
    K1,
    K2,
    K3,
    K4,
    K5,
    K6,
    K7,
    K8,
    K9,
    K10,
    K11,
    K12,
    K13,
    K14,
    K15,
    K16,
    K17,
    K18,
    K19,
    K20,
    K21,
    K22,
    K23,
    K24,
    K25,
    K26,
    K27,
    K28,
    K29,
    K30,
    K31,
    K32,
    K33,
    K34,
    K35,
    K36,
    K37,
    K38,
    K39,
    K40,
    K41,
    K42,
    K43,
    K44,
    K45,
    K46,
    K47,
    K48,
    K49,
    K50,
    K51,
    K52,
    K53,
    K54,
    K55,
    K56,
    K57,
    K58,
    K59,
    K60,
    K61,
    K62,
    K63,
    K64,
    K65,
    K66,
    K67,
    K68,
    K69,
    K70,
    K71,
    K72,
    K73,
    K74,
    K75,
    K76,
    K77,
    K78,
}
enum Read {
    Start(Kind, usize),
    Finish(Kind, u8, usize),
}
enum Value {
    Rows(Vec<Value>),
    Optional(Option<Box<Value>>),
    K0(Box<DynTypeValue>),
    K1(Box<Type>),
    K2(Box<StorageLifeShape>),
    K4(Box<StorageBinder>),
    K5(Box<DynLifePattern>),
    K6(Box<u32>),
    K7(Box<DynObligation>),
    K8(Box<DynTypePattern>),
    K9(Box<DynStructuralBound>),
    K10(Box<DynSelfPattern>),
    K11(Box<CallableKind>),
    K12(Box<DynSignatureRecipe>),
    K13(Box<DynParameterRecipe>),
    K14(Box<PassingMode>),
    K16(Box<CallKind>),
    K17(Box<DynContractRecipe>),
    K18(Box<DynRequirementRecipe>),
    K19(Box<StoragePosition>),
    K20(Box<DynGuaranteeRecipe>),
    K21(Box<StorageSourceBinding>),
    K22(Box<DynEffectRecipe>),
    K23(Box<DynCleanupRecipe>),
    K24(Box<bool>),
    K26(Box<DynCallAccess>),
    K35(Box<DynResultSource>),
    K39(Box<DynResumeBound>),
    K41(Box<DynThreadRequirements>),
    K42(Box<DynDefinitionId>),
    K43(Box<ModuleId>),
    K44(Box<NodeId>),
    K45(Box<DynDefinitionScope>),
    K46(Box<DynDefinitionDeclaration>),
    K49(Box<DynDefinitionSpecialization>),
    K50(Box<LifeDeclarationIdentity>),
    K51(Box<DynMethodRecipe>),
    K53(Box<DynFunctionInstance>),
    K55(Box<DynGenericActual>),
    K56(Box<DynAssociatedValueRecipe>),
    K57(Box<DynConstPattern>),
    K58(Box<DynAssociatedDefinition>),
    K61(Box<DynAssociatedSource>),
    K62(Box<LifeDeclarationId>),
    K63(Box<DynAssociatedValue>),
    K64(Box<Constant>),
    K65(Box<DynAssociatedResolution>),
    K66(Box<DynMember>),
    K67(Box<DynImplementationWitness>),
    K68(Box<DynTraitApplication>),
    K70(Box<DynFunctionTarget>),
    K71(Box<FunctionId>),
    K73(Box<DynPatternActual>),
    K74(Box<UnaryOp>),
    K75(Box<BinaryOp>),
    K76(Box<String>),
}
impl Writer<'_> {
    fn dyn_recipe_node(&mut self, depth: usize) -> WireResult<()> {
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(quota("dynamic recipe depth exceeded"));
        }
        self.charge(1)
    }
    pub(super) fn dyn_type_value(&mut self, value: &DynTypeValue, depth: usize) -> WireResult<()> {
        self.recipe_write(Write::K0(value, depth))
    }
    pub(super) fn dyn_obligation(&mut self, value: &DynObligation, depth: usize) -> WireResult<()> {
        self.recipe_write(Write::K7(value, depth))
    }
    pub(super) fn dyn_definition_declaration(
        &mut self,
        value: &DynDefinitionDeclaration,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K46(value, depth))
    }
    pub(super) fn dyn_method_recipe(
        &mut self,
        value: &DynMethodRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K51(value, depth))
    }
    pub(super) fn dyn_function_instance(
        &mut self,
        value: &DynFunctionInstance,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K53(value, depth))
    }
    pub(super) fn dyn_associated_definition(
        &mut self,
        value: &DynAssociatedDefinition,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K58(value, depth))
    }
    pub(super) fn dyn_implementation_witness(
        &mut self,
        value: &DynImplementationWitness,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K67(value, depth))
    }
    pub(super) fn dyn_self_pattern(
        &mut self,
        value: &DynSelfPattern,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K10(value, depth))
    }
    pub(super) fn dyn_type_pattern(
        &mut self,
        value: &DynTypePattern,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K8(value, depth))
    }
    fn recipe_write(&mut self, first: Write<'_>) -> WireResult<()> {
        let mut pending = vec![first];
        while let Some(next) = pending.pop() {
            match next {
                Write::K0(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K3(&value.classes, depth + 1));
                    pending.push(Write::K2(&value.life, depth + 1));
                    pending.push(Write::K1(&value.ty, depth + 1));
                }
                Write::K1(value, depth) => {
                    let _ = depth;
                    self.ty(value, depth)?;
                }
                Write::K2(value, depth) => {
                    let _ = depth;
                    self.storage_shape(value)?;
                }
                Write::K3(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K4(item, depth));
                    }
                }
                Write::K4(value, depth) => {
                    let _ = depth;
                    self.storage_binder(value)?;
                }
                Write::K5(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynLifePattern::DeclarationParameter(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynLifePattern::MethodParameter(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynLifePattern::Bound {
                            depth: field_depth,
                            index: field_index,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K6(field_index, depth + 1));
                            pending.push(Write::K6(field_depth, depth + 1));
                        }
                        DynLifePattern::Invocation(field_value_0) => {
                            self.u8(3)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K6(value, depth) => {
                    let _ = depth;
                    self.u32(*value)?;
                }
                Write::K7(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynObligation::Structural {
                            receiver: field_receiver,
                            bound: field_bound,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K9(field_bound, depth + 1));
                            pending.push(Write::K8(field_receiver, depth + 1));
                        }
                        DynObligation::Trait {
                            receiver: field_receiver,
                            application: field_application,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K10(field_application, depth + 1));
                            pending.push(Write::K8(field_receiver, depth + 1));
                        }
                        DynObligation::Callable {
                            receiver: field_receiver,
                            kind: field_kind,
                            quantified_lifetimes: field_quantified_lifetimes,
                            signature: field_signature,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K12(field_signature, depth + 1));
                            pending.push(Write::K6(field_quantified_lifetimes, depth + 1));
                            pending.push(Write::K11(field_kind, depth + 1));
                            pending.push(Write::K8(field_receiver, depth + 1));
                        }
                    }
                }
                Write::K8(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynTypePattern::Concrete(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K1(field_value_0, depth + 1));
                        }
                        DynTypePattern::Parameter(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynTypePattern::Nominal(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K10(field_value_0, depth + 1));
                        }
                        DynTypePattern::Address {
                            mutable: field_mutable,
                            lifetime: field_lifetime,
                            inner: field_inner,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K8(field_inner, depth + 1));
                            pending.push(Write::K77(field_lifetime, depth + 1));
                            pending.push(Write::K24(field_mutable, depth + 1));
                        }
                        DynTypePattern::Tuple(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K78(field_value_0, depth + 1));
                        }
                        DynTypePattern::Array {
                            element: field_element,
                            length: field_length,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K57(field_length, depth + 1));
                            pending.push(Write::K8(field_element, depth + 1));
                        }
                        DynTypePattern::Slice(field_value_0) => {
                            self.u8(6)?;
                            pending.push(Write::K8(field_value_0, depth + 1));
                        }
                        DynTypePattern::Managed(field_value_0) => {
                            self.u8(7)?;
                            pending.push(Write::K8(field_value_0, depth + 1));
                        }
                        DynTypePattern::Desc(field_value_0) => {
                            self.u8(8)?;
                            pending.push(Write::K8(field_value_0, depth + 1));
                        }
                        DynTypePattern::OwnedStore(field_value_0) => {
                            self.u8(9)?;
                            pending.push(Write::K8(field_value_0, depth + 1));
                        }
                        DynTypePattern::ErrorUnion {
                            error: field_error,
                            result: field_result,
                        } => {
                            self.u8(10)?;
                            pending.push(Write::K8(field_result, depth + 1));
                            pending.push(Write::K8(field_error, depth + 1));
                        }
                        DynTypePattern::SelfType => {
                            self.u8(11)?;
                        }
                        DynTypePattern::MethodParameter(field_value_0) => {
                            self.u8(12)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynTypePattern::AssociatedProjection {
                            receiver: field_receiver,
                            application: field_application,
                            member: field_member,
                        } => {
                            self.u8(13)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K10(field_application, depth + 1));
                            pending.push(Write::K8(field_receiver, depth + 1));
                        }
                    }
                }
                Write::K9(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynStructuralBound::Copy => {
                            self.u8(0)?;
                        }
                        DynStructuralBound::Value => {
                            self.u8(1)?;
                        }
                        DynStructuralBound::SyncDrop => {
                            self.u8(2)?;
                        }
                        DynStructuralBound::StoreElement => {
                            self.u8(3)?;
                        }
                    }
                }
                Write::K10(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K72(&value.arguments, depth + 1));
                    pending.push(Write::K62(&value.declaration, depth + 1));
                }
                Write::K11(value, depth) => {
                    let _ = depth;
                    self.u8(match value {
                        CallableKind::FunctionPointer => 0,
                        CallableKind::Fn => 1,
                        CallableKind::FnMut => 2,
                        CallableKind::FnOnce => 3,
                    })?;
                }
                Write::K12(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K17(&value.contract, depth + 1));
                    pending.push(Write::K6(&value.invocation_binder_count, depth + 1));
                    pending.push(Write::K16(&value.call_kind, depth + 1));
                    pending.push(Write::K14(&value.result_mode, depth + 1));
                    pending.push(Write::K8(&value.result, depth + 1));
                    pending.push(Write::K15(&value.parameters, depth + 1));
                }
                Write::K13(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K14(&value.mode, depth + 1));
                    pending.push(Write::K8(&value.ty, depth + 1));
                }
                Write::K14(value, depth) => {
                    let _ = depth;
                    self.dyn_passing_mode(value)?;
                }
                Write::K15(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K13(item, depth));
                    }
                }
                Write::K16(value, depth) => {
                    let _ = depth;
                    self.dyn_call_kind(value)?;
                }
                Write::K17(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K40(&value.threads, depth + 1));
                    pending.push(Write::K38(&value.resume, depth + 1));
                    pending.push(Write::K37(&value.effects, depth + 1));
                    pending.push(Write::K36(&value.cleanup, depth + 1));
                    pending.push(Write::K34(&value.result, depth + 1));
                    pending.push(Write::K33(&value.call, depth + 1));
                    pending.push(Write::K32(&value.hold, depth + 1));
                    pending.push(Write::K30(&value.guarantees, depth + 1));
                    pending.push(Write::K28(&value.requires, depth + 1));
                }
                Write::K18(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynRequirementRecipe::Read(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K19(field_value_0, depth + 1));
                        }
                        DynRequirementRecipe::Write(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K19(field_value_0, depth + 1));
                        }
                        DynRequirementRecipe::Manage(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K19(field_value_0, depth + 1));
                        }
                        DynRequirementRecipe::Apart(field_value_0, field_value_1) => {
                            self.u8(3)?;
                            pending.push(Write::K19(field_value_1, depth + 1));
                            pending.push(Write::K19(field_value_0, depth + 1));
                        }
                        DynRequirementRecipe::State {
                            position: field_position,
                            ty: field_ty,
                            variant: field_variant,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_variant, depth + 1));
                            pending.push(Write::K8(field_ty, depth + 1));
                            pending.push(Write::K19(field_position, depth + 1));
                        }
                    }
                }
                Write::K19(value, depth) => {
                    let _ = depth;
                    self.storage_position(value)?;
                }
                Write::K20(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynGuaranteeRecipe::StorageSources(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K21(field_value_0, depth + 1));
                        }
                        DynGuaranteeRecipe::ManageResult => {
                            self.u8(1)?;
                        }
                        DynGuaranteeRecipe::State {
                            position: field_position,
                            ty: field_ty,
                            variant: field_variant,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K6(field_variant, depth + 1));
                            pending.push(Write::K8(field_ty, depth + 1));
                            pending.push(Write::K19(field_position, depth + 1));
                        }
                    }
                }
                Write::K21(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K25(&value.sources, depth + 1));
                    pending.push(Write::K19(&value.position, depth + 1));
                }
                Write::K22(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynEffectRecipe::Alloc => {
                            self.u8(0)?;
                        }
                        DynEffectRecipe::Io => {
                            self.u8(1)?;
                        }
                        DynEffectRecipe::Block => {
                            self.u8(2)?;
                        }
                        DynEffectRecipe::Suspend => {
                            self.u8(3)?;
                        }
                        DynEffectRecipe::Spawn => {
                            self.u8(4)?;
                        }
                        DynEffectRecipe::Ffi => {
                            self.u8(5)?;
                        }
                        DynEffectRecipe::Reenter => {
                            self.u8(6)?;
                        }
                        DynEffectRecipe::Panic => {
                            self.u8(7)?;
                        }
                        DynEffectRecipe::Call(field_value_0) => {
                            self.u8(8)?;
                            pending.push(Write::K19(field_value_0, depth + 1));
                        }
                        DynEffectRecipe::Cleanup(field_value_0) => {
                            self.u8(9)?;
                            pending.push(Write::K8(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K23(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K27(&value.effects, depth + 1));
                    pending.push(Write::K26(&value.access, depth + 1));
                    pending.push(Write::K25(&value.positions, depth + 1));
                    pending.push(Write::K24(&value.environment, depth + 1));
                }
                Write::K24(value, depth) => {
                    let _ = depth;
                    self.boolean(*value)?;
                }
                Write::K25(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K19(item, depth));
                    }
                }
                Write::K26(value, depth) => {
                    let _ = depth;
                    self.dyn_call_access(value)?;
                }
                Write::K27(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K22(item, depth));
                    }
                }
                Write::K28(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K29(item, depth));
                    }
                }
                Write::K29(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K18(item, depth));
                    }
                }
                Write::K30(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K31(item, depth));
                    }
                }
                Write::K31(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K20(item, depth));
                    }
                }
                Write::K32(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K25(item, depth));
                    }
                }
                Write::K33(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K26(item, depth));
                    }
                }
                Write::K34(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K35(item, depth));
                    }
                }
                Write::K35(value, depth) => {
                    let _ = depth;
                    self.dyn_result_source(value)?;
                }
                Write::K36(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K23(item, depth));
                    }
                }
                Write::K37(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K27(item, depth));
                    }
                }
                Write::K38(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K39(item, depth));
                    }
                }
                Write::K39(value, depth) => {
                    let _ = depth;
                    self.dyn_resume_bound(value)?;
                }
                Write::K40(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K41(item, depth));
                    }
                }
                Write::K41(value, depth) => {
                    let _ = depth;
                    self.dyn_thread_requirements(value)?;
                }
                Write::K42(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K44(&value.source, depth + 1));
                    pending.push(Write::K43(&value.module, depth + 1));
                }
                Write::K43(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K44(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K45(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynDefinitionScope::TraitDefault => {
                            self.u8(0)?;
                        }
                        DynDefinitionScope::ImplOverride => {
                            self.u8(1)?;
                        }
                    }
                }
                Write::K46(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K12(&value.signature, depth + 1));
                    pending.push(Write::K45(&value.scope, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K50(&value.identity, depth + 1));
                    pending.push(Write::K42(&value.id, depth + 1));
                    pending.push(Write::K48(&value.instances, depth + 1));
                    pending.push(Write::K47(&value.requirements, depth + 1));
                }
                Write::K47(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K7(item, depth));
                    }
                }
                Write::K48(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K49(item, depth));
                    }
                }
                Write::K49(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K54(&value.method_actuals, depth + 1));
                    pending.push(Write::K54(&value.declaration_actuals, depth + 1));
                    pending.push(Write::K0(&value.receiver, depth + 1));
                    pending.push(Write::K70(&value.function, depth + 1));
                }
                Write::K50(value, depth) => {
                    let _ = depth;
                    self.declaration_identity(value)?;
                }
                Write::K51(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K52(&value.default_definition, depth + 1));
                    pending.push(Write::K12(&value.signature, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K47(&value.requirements, depth + 1));
                }
                Write::K52(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K42(item, depth));
                    }
                }
                Write::K53(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K54(&value.method_actuals, depth + 1));
                    pending.push(Write::K54(&value.declaration_actuals, depth + 1));
                    pending.push(Write::K0(&value.receiver, depth + 1));
                    pending.push(Write::K42(&value.definition, depth + 1));
                }
                Write::K54(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K55(item, depth));
                    }
                }
                Write::K55(value, depth) => {
                    let _ = depth;
                    self.dyn_generic_actual(value)?;
                }
                Write::K56(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynAssociatedValueRecipe::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K8(field_value_0, depth + 1));
                        }
                        DynAssociatedValueRecipe::Constant {
                            ty: field_ty,
                            value: field_value,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K57(field_value, depth + 1));
                            pending.push(Write::K8(field_ty, depth + 1));
                        }
                    }
                }
                Write::K57(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynConstPattern::Value(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K64(field_value_0, depth + 1));
                        }
                        DynConstPattern::Parameter(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynConstPattern::MethodParameter(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynConstPattern::Unary {
                            op: field_op,
                            value: field_value,
                            ty: field_ty,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K8(field_ty, depth + 1));
                            pending.push(Write::K57(field_value, depth + 1));
                            pending.push(Write::K74(field_op, depth + 1));
                        }
                        DynConstPattern::Binary {
                            op: field_op,
                            left: field_left,
                            right: field_right,
                            ty: field_ty,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K8(field_ty, depth + 1));
                            pending.push(Write::K57(field_right, depth + 1));
                            pending.push(Write::K57(field_left, depth + 1));
                            pending.push(Write::K75(field_op, depth + 1));
                        }
                        DynConstPattern::Convert {
                            value: field_value,
                            ty: field_ty,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K8(field_ty, depth + 1));
                            pending.push(Write::K57(field_value, depth + 1));
                        }
                        DynConstPattern::AssociatedProjection {
                            receiver: field_receiver,
                            application: field_application,
                            member: field_member,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K10(field_application, depth + 1));
                            pending.push(Write::K8(field_receiver, depth + 1));
                        }
                        DynConstPattern::IntegerLiteral(field_value_0) => {
                            self.u8(7)?;
                            pending.push(Write::K76(field_value_0, depth + 1));
                        }
                        DynConstPattern::FloatLiteral(field_value_0) => {
                            self.u8(8)?;
                            pending.push(Write::K76(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K58(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K47(&value.requirements, depth + 1));
                    pending.push(Write::K60(&value.value, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K59(&value.carrier, depth + 1));
                }
                Write::K59(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K8(item, depth));
                    }
                }
                Write::K60(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K56(item, depth));
                    }
                }
                Write::K61(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynAssociatedSource::TraitDefault {
                            declaration: field_declaration,
                            member: field_member,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K62(field_declaration, depth + 1));
                        }
                        DynAssociatedSource::ImplOverride {
                            declaration: field_declaration,
                            member: field_member,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K62(field_declaration, depth + 1));
                        }
                    }
                }
                Write::K62(value, depth) => {
                    let _ = depth;
                    self.declaration_id(value)?;
                }
                Write::K63(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynAssociatedValue::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K0(field_value_0, depth + 1));
                        }
                        DynAssociatedValue::Constant {
                            ty: field_ty,
                            value: field_value,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K64(field_value, depth + 1));
                            pending.push(Write::K0(field_ty, depth + 1));
                        }
                    }
                }
                Write::K64(value, depth) => {
                    let _ = depth;
                    self.constant(value)?;
                }
                Write::K65(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K63(&value.value, depth + 1));
                    pending.push(Write::K61(&value.source, depth + 1));
                    pending.push(Write::K66(&value.member, depth + 1));
                }
                Write::K66(value, depth) => {
                    let _ = depth;
                    self.dyn_member(value)?;
                }
                Write::K67(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K69(&value.associated, depth + 1));
                    pending.push(Write::K54(&value.actuals, depth + 1));
                    pending.push(Write::K62(&value.implementation, depth + 1));
                    pending.push(Write::K68(&value.application, depth + 1));
                    pending.push(Write::K0(&value.receiver, depth + 1));
                }
                Write::K68(value, depth) => {
                    let _ = depth;
                    self.dyn_trait_application(value)?;
                }
                Write::K69(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K65(item, depth));
                    }
                }
                Write::K70(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynFunctionTarget::Core {
                            module: field_module,
                            source: field_source,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K44(field_source, depth + 1));
                            pending.push(Write::K43(field_module, depth + 1));
                        }
                        DynFunctionTarget::Typed(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K71(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K71(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K72(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K73(item, depth));
                    }
                }
                Write::K73(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynPatternActual::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K8(field_value_0, depth + 1));
                        }
                        DynPatternActual::Const(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K57(field_value_0, depth + 1));
                        }
                        DynPatternActual::Life(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K5(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K74(value, depth) => {
                    let _ = depth;
                    self.unary(*value)?;
                }
                Write::K75(value, depth) => {
                    let _ = depth;
                    self.binary(*value)?;
                }
                Write::K76(value, depth) => {
                    let _ = depth;
                    self.text(value)?;
                }
                Write::K77(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K5(item, depth));
                    }
                }
                Write::K78(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K8(item, depth));
                    }
                }
            }
        }
        Ok(())
    }
}
impl Reader<'_> {
    fn dyn_recipe_node(&mut self, depth: usize) -> WireResult<()> {
        if depth >= self.limits.max_call_depth.min(128) {
            return Err(quota("dynamic recipe depth exceeded"));
        }
        self.charge(1)
    }
    pub(super) fn dyn_type_value(&mut self, depth: usize) -> WireResult<DynTypeValue> {
        let value = self.recipe_read(Kind::K0, depth)?;
        Ok(match value {
            Value::K0(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_obligation(&mut self, depth: usize) -> WireResult<DynObligation> {
        let value = self.recipe_read(Kind::K7, depth)?;
        Ok(match value {
            Value::K7(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_definition_declaration(
        &mut self,
        depth: usize,
    ) -> WireResult<DynDefinitionDeclaration> {
        let value = self.recipe_read(Kind::K46, depth)?;
        Ok(match value {
            Value::K46(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_method_recipe(&mut self, depth: usize) -> WireResult<DynMethodRecipe> {
        let value = self.recipe_read(Kind::K51, depth)?;
        Ok(match value {
            Value::K51(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_function_instance(
        &mut self,
        depth: usize,
    ) -> WireResult<DynFunctionInstance> {
        let value = self.recipe_read(Kind::K53, depth)?;
        Ok(match value {
            Value::K53(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_associated_definition(
        &mut self,
        depth: usize,
    ) -> WireResult<DynAssociatedDefinition> {
        let value = self.recipe_read(Kind::K58, depth)?;
        Ok(match value {
            Value::K58(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_implementation_witness(
        &mut self,
        depth: usize,
    ) -> WireResult<DynImplementationWitness> {
        let value = self.recipe_read(Kind::K67, depth)?;
        Ok(match value {
            Value::K67(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_self_pattern(&mut self, depth: usize) -> WireResult<DynSelfPattern> {
        let value = self.recipe_read(Kind::K10, depth)?;
        Ok(match value {
            Value::K10(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_type_pattern(&mut self, depth: usize) -> WireResult<DynTypePattern> {
        let value = self.recipe_read(Kind::K8, depth)?;
        Ok(match value {
            Value::K8(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    fn recipe_read(&mut self, first: Kind, depth: usize) -> WireResult<Value> {
        let mut pending = vec![Read::Start(first, depth)];
        let mut values = Vec::new();
        while let Some(next) = pending.pop() {
            match next {
                Read::Start(kind, depth) => match kind {
                    Kind::K0 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K3, depth + 1));
                        pending.push(Read::Start(Kind::K2, depth + 1));
                        pending.push(Read::Start(Kind::K1, depth + 1));
                    }
                    Kind::K1 => {
                        let _ = depth;
                        values.push(Value::K1(Box::new(self.ty(depth)?)));
                    }
                    Kind::K2 => {
                        let _ = depth;
                        values.push(Value::K2(Box::new(self.storage_shape()?)));
                    }
                    Kind::K3 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K4, depth));
                        }
                    }
                    Kind::K4 => {
                        let _ = depth;
                        values.push(Value::K4(Box::new(self.storage_binder()?)));
                    }
                    Kind::K5 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K6 => {
                        let _ = depth;
                        values.push(Value::K6(Box::new(self.u32()?)));
                    }
                    Kind::K7 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K9, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K10, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 4));
                                pending.push(Read::Start(Kind::K12, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K11, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K8 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K1, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K10, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 3));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                                pending.push(Read::Start(Kind::K77, depth + 1));
                                pending.push(Read::Start(Kind::K24, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K78, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K57, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            10 => {
                                pending.push(Read::Finish(kind, 10, 2));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            11 => {
                                pending.push(Read::Finish(kind, 11, 0));
                            }
                            12 => {
                                pending.push(Read::Finish(kind, 12, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            13 => {
                                pending.push(Read::Finish(kind, 13, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K10, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K9 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 0));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 0));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K10 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K72, depth + 1));
                        pending.push(Read::Start(Kind::K62, depth + 1));
                    }
                    Kind::K11 => {
                        let _ = depth;
                        values.push(Value::K11(Box::new(self.callable_kind()?)));
                    }
                    Kind::K12 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 6));
                        pending.push(Read::Start(Kind::K17, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K16, depth + 1));
                        pending.push(Read::Start(Kind::K14, depth + 1));
                        pending.push(Read::Start(Kind::K8, depth + 1));
                        pending.push(Read::Start(Kind::K15, depth + 1));
                    }
                    Kind::K13 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K14, depth + 1));
                        pending.push(Read::Start(Kind::K8, depth + 1));
                    }
                    Kind::K14 => {
                        let _ = depth;
                        values.push(Value::K14(Box::new(self.dyn_passing_mode()?)));
                    }
                    Kind::K15 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K13, depth));
                        }
                    }
                    Kind::K16 => {
                        let _ = depth;
                        values.push(Value::K16(Box::new(self.dyn_call_kind()?)));
                    }
                    Kind::K17 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 9));
                        pending.push(Read::Start(Kind::K40, depth + 1));
                        pending.push(Read::Start(Kind::K38, depth + 1));
                        pending.push(Read::Start(Kind::K37, depth + 1));
                        pending.push(Read::Start(Kind::K36, depth + 1));
                        pending.push(Read::Start(Kind::K34, depth + 1));
                        pending.push(Read::Start(Kind::K33, depth + 1));
                        pending.push(Read::Start(Kind::K32, depth + 1));
                        pending.push(Read::Start(Kind::K30, depth + 1));
                        pending.push(Read::Start(Kind::K28, depth + 1));
                    }
                    Kind::K18 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K19 => {
                        let _ = depth;
                        values.push(Value::K19(Box::new(self.storage_position()?)));
                    }
                    Kind::K20 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K21 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K25, depth + 1));
                        pending.push(Read::Start(Kind::K19, depth + 1));
                    }
                    Kind::K22 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 0));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 0));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 0));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 0));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 0));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 0));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 0));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K23 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K27, depth + 1));
                        pending.push(Read::Start(Kind::K26, depth + 1));
                        pending.push(Read::Start(Kind::K25, depth + 1));
                        pending.push(Read::Start(Kind::K24, depth + 1));
                    }
                    Kind::K24 => {
                        let _ = depth;
                        values.push(Value::K24(Box::new(self.boolean()?)));
                    }
                    Kind::K25 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K19, depth));
                        }
                    }
                    Kind::K26 => {
                        let _ = depth;
                        values.push(Value::K26(Box::new(self.dyn_call_access()?)));
                    }
                    Kind::K27 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K22, depth));
                        }
                    }
                    Kind::K28 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K29, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K29 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K18, depth));
                        }
                    }
                    Kind::K30 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K31, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K31 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K20, depth));
                        }
                    }
                    Kind::K32 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K25, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K33 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K26, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K34 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K35, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K35 => {
                        let _ = depth;
                        values.push(Value::K35(Box::new(self.dyn_result_source()?)));
                    }
                    Kind::K36 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K23, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K37 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K27, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K38 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K39, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K39 => {
                        let _ = depth;
                        values.push(Value::K39(Box::new(self.dyn_resume_bound()?)));
                    }
                    Kind::K40 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K41, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K41 => {
                        let _ = depth;
                        values.push(Value::K41(Box::new(self.dyn_thread_requirements()?)));
                    }
                    Kind::K42 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K44, depth + 1));
                        pending.push(Read::Start(Kind::K43, depth + 1));
                    }
                    Kind::K43 => {
                        let _ = depth;
                        values.push(Value::K43(Box::new(ModuleId(self.u32()?))));
                    }
                    Kind::K44 => {
                        let _ = depth;
                        values.push(Value::K44(Box::new(NodeId(self.u32()?))));
                    }
                    Kind::K45 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 0));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K46 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 7));
                        pending.push(Read::Start(Kind::K12, depth + 1));
                        pending.push(Read::Start(Kind::K45, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K50, depth + 1));
                        pending.push(Read::Start(Kind::K42, depth + 1));
                        pending.push(Read::Start(Kind::K48, depth + 1));
                        pending.push(Read::Start(Kind::K47, depth + 1));
                    }
                    Kind::K47 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K7, depth));
                        }
                    }
                    Kind::K48 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K49, depth));
                        }
                    }
                    Kind::K49 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K54, depth + 1));
                        pending.push(Read::Start(Kind::K54, depth + 1));
                        pending.push(Read::Start(Kind::K0, depth + 1));
                        pending.push(Read::Start(Kind::K70, depth + 1));
                    }
                    Kind::K50 => {
                        let _ = depth;
                        values.push(Value::K50(Box::new(self.declaration_identity()?)));
                    }
                    Kind::K51 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K52, depth + 1));
                        pending.push(Read::Start(Kind::K12, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K47, depth + 1));
                    }
                    Kind::K52 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K42, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K53 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K54, depth + 1));
                        pending.push(Read::Start(Kind::K54, depth + 1));
                        pending.push(Read::Start(Kind::K0, depth + 1));
                        pending.push(Read::Start(Kind::K42, depth + 1));
                    }
                    Kind::K54 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K55, depth));
                        }
                    }
                    Kind::K55 => {
                        let _ = depth;
                        values.push(Value::K55(Box::new(self.dyn_generic_actual()?)));
                    }
                    Kind::K56 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K57, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K57 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K64, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 3));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                                pending.push(Read::Start(Kind::K57, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 4));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                                pending.push(Read::Start(Kind::K57, depth + 1));
                                pending.push(Read::Start(Kind::K57, depth + 1));
                                pending.push(Read::Start(Kind::K75, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                                pending.push(Read::Start(Kind::K57, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K10, depth + 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 1));
                                pending.push(Read::Start(Kind::K76, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 1));
                                pending.push(Read::Start(Kind::K76, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K58 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K47, depth + 1));
                        pending.push(Read::Start(Kind::K60, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K59, depth + 1));
                    }
                    Kind::K59 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K8, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K60 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K56, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K61 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K62, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K62, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K62 => {
                        let _ = depth;
                        values.push(Value::K62(Box::new(self.declaration_id()?)));
                    }
                    Kind::K63 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K0, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K64, depth + 1));
                                pending.push(Read::Start(Kind::K0, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K64 => {
                        let _ = depth;
                        values.push(Value::K64(Box::new(self.constant()?)));
                    }
                    Kind::K65 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K63, depth + 1));
                        pending.push(Read::Start(Kind::K61, depth + 1));
                        pending.push(Read::Start(Kind::K66, depth + 1));
                    }
                    Kind::K66 => {
                        let _ = depth;
                        values.push(Value::K66(Box::new(self.dyn_member()?)));
                    }
                    Kind::K67 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 5));
                        pending.push(Read::Start(Kind::K69, depth + 1));
                        pending.push(Read::Start(Kind::K54, depth + 1));
                        pending.push(Read::Start(Kind::K62, depth + 1));
                        pending.push(Read::Start(Kind::K68, depth + 1));
                        pending.push(Read::Start(Kind::K0, depth + 1));
                    }
                    Kind::K68 => {
                        let _ = depth;
                        values.push(Value::K68(Box::new(self.dyn_trait_application()?)));
                    }
                    Kind::K69 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K65, depth));
                        }
                    }
                    Kind::K70 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K44, depth + 1));
                                pending.push(Read::Start(Kind::K43, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K71, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K71 => {
                        let _ = depth;
                        values.push(Value::K71(Box::new(FunctionId(self.u32()?))));
                    }
                    Kind::K72 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K73, depth));
                        }
                    }
                    Kind::K73 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K8, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K57, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K5, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K74 => {
                        let _ = depth;
                        values.push(Value::K74(Box::new(self.unary()?)));
                    }
                    Kind::K75 => {
                        let _ = depth;
                        values.push(Value::K75(Box::new(self.binary()?)));
                    }
                    Kind::K76 => {
                        let _ = depth;
                        values.push(Value::K76(Box::new(self.text()?)));
                    }
                    Kind::K77 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K5, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K78 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K8, depth));
                        }
                    }
                },
                Read::Finish(kind, tag, count) => {
                    let at = values
                        .len()
                        .checked_sub(count)
                        .expect("recipe grammar arity");
                    let mut fields = values.split_off(at).into_iter();
                    let value = match kind {
                        Kind::K0 => {
                            let _ = tag;
                            Value::K0(Box::new(DynTypeValue {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K1(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                life: match fields.next().expect("recipe field") {
                                    Value::K2(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                classes: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K4(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K3 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K5 => Value::K5(Box::new(match tag {
                            0 => DynLifePattern::DeclarationParameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynLifePattern::MethodParameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => DynLifePattern::Bound {
                                depth: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                index: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => DynLifePattern::Invocation(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K7 => Value::K7(Box::new(match tag {
                            0 => DynObligation::Structural {
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                bound: match fields.next().expect("recipe field") {
                                    Value::K9(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => DynObligation::Trait {
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                application: match fields.next().expect("recipe field") {
                                    Value::K10(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => DynObligation::Callable {
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                kind: match fields.next().expect("recipe field") {
                                    Value::K11(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                quantified_lifetimes: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                signature: Box::new(match fields.next().expect("recipe field") {
                                    Value::K12(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K8 => Value::K8(Box::new(match tag {
                            0 => DynTypePattern::Concrete(
                                match fields.next().expect("recipe field") {
                                    Value::K1(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynTypePattern::Parameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => DynTypePattern::Nominal(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K10(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            3 => DynTypePattern::Address {
                                mutable: match fields.next().expect("recipe field") {
                                    Value::K24(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                lifetime: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K5(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                inner: Box::new(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            4 => {
                                DynTypePattern::Tuple(match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K8(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                })
                            }
                            5 => DynTypePattern::Array {
                                element: Box::new(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                length: match fields.next().expect("recipe field") {
                                    Value::K57(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            6 => DynTypePattern::Slice(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            7 => DynTypePattern::Managed(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            8 => DynTypePattern::Desc(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            9 => DynTypePattern::OwnedStore(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            10 => DynTypePattern::ErrorUnion {
                                error: Box::new(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                result: Box::new(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            11 => DynTypePattern::SelfType,
                            12 => DynTypePattern::MethodParameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            13 => DynTypePattern::AssociatedProjection {
                                receiver: Box::new(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                application: Box::new(match fields.next().expect("recipe field") {
                                    Value::K10(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K9 => Value::K9(Box::new(match tag {
                            0 => DynStructuralBound::Copy,
                            1 => DynStructuralBound::Value,
                            2 => DynStructuralBound::SyncDrop,
                            3 => DynStructuralBound::StoreElement,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K10 => {
                            let _ = tag;
                            Value::K10(Box::new(DynSelfPattern {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K62(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K73(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K12 => {
                            let _ = tag;
                            Value::K12(Box::new(DynSignatureRecipe {
                                parameters: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K13(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                result: match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                result_mode: match fields.next().expect("recipe field") {
                                    Value::K14(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                call_kind: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                invocation_binder_count: match fields.next().expect("recipe field")
                                {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                contract: match fields.next().expect("recipe field") {
                                    Value::K17(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K13 => {
                            let _ = tag;
                            Value::K13(Box::new(DynParameterRecipe {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K14(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K15 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K17 => {
                            let _ = tag;
                            Value::K17(Box::new(DynContractRecipe {
                                requires: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::Rows(rows) => rows
                                            .into_iter()
                                            .map(|item| match item {
                                                Value::K18(value) => *value,
                                                _ => unreachable!("recipe field grammar"),
                                            })
                                            .collect(),
                                        _ => unreachable!("recipe row grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                guarantees: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::Rows(rows) => rows
                                            .into_iter()
                                            .map(|item| match item {
                                                Value::K20(value) => *value,
                                                _ => unreachable!("recipe field grammar"),
                                            })
                                            .collect(),
                                        _ => unreachable!("recipe row grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                hold: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::Rows(rows) => rows
                                            .into_iter()
                                            .map(|item| match item {
                                                Value::K19(value) => *value,
                                                _ => unreachable!("recipe field grammar"),
                                            })
                                            .collect(),
                                        _ => unreachable!("recipe row grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                call: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K26(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                result: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K35(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                cleanup: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K23(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                effects: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::Rows(rows) => rows
                                            .into_iter()
                                            .map(|item| match item {
                                                Value::K22(value) => *value,
                                                _ => unreachable!("recipe field grammar"),
                                            })
                                            .collect(),
                                        _ => unreachable!("recipe row grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                resume: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K39(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                threads: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K41(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            }))
                        }
                        Kind::K18 => Value::K18(Box::new(match tag {
                            0 => DynRequirementRecipe::Read(
                                match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynRequirementRecipe::Write(
                                match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => DynRequirementRecipe::Manage(
                                match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => DynRequirementRecipe::Apart(
                                match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            4 => DynRequirementRecipe::State {
                                position: match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                variant: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K20 => Value::K20(Box::new(match tag {
                            0 => DynGuaranteeRecipe::StorageSources(
                                match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynGuaranteeRecipe::ManageResult,
                            2 => DynGuaranteeRecipe::State {
                                position: match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                variant: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K21 => {
                            let _ = tag;
                            Value::K21(Box::new(StorageSourceBinding {
                                position: match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                sources: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K19(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K22 => Value::K22(Box::new(match tag {
                            0 => DynEffectRecipe::Alloc,
                            1 => DynEffectRecipe::Io,
                            2 => DynEffectRecipe::Block,
                            3 => DynEffectRecipe::Suspend,
                            4 => DynEffectRecipe::Spawn,
                            5 => DynEffectRecipe::Ffi,
                            6 => DynEffectRecipe::Reenter,
                            7 => DynEffectRecipe::Panic,
                            8 => {
                                DynEffectRecipe::Call(match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            9 => DynEffectRecipe::Cleanup(
                                match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K23 => {
                            let _ = tag;
                            Value::K23(Box::new(DynCleanupRecipe {
                                environment: match fields.next().expect("recipe field") {
                                    Value::K24(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                positions: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K19(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                access: match fields.next().expect("recipe field") {
                                    Value::K26(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                effects: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K22(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K25 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K27 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K28 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K29 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K30 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K31 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K32 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K33 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K34 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K36 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K37 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K38 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K40 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K42 => {
                            let _ = tag;
                            Value::K42(Box::new(DynDefinitionId {
                                module: match fields.next().expect("recipe field") {
                                    Value::K43(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K44(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K45 => Value::K45(Box::new(match tag {
                            0 => DynDefinitionScope::TraitDefault,
                            1 => DynDefinitionScope::ImplOverride,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K46 => {
                            let _ = tag;
                            Value::K46(Box::new(DynDefinitionDeclaration {
                                requirements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K7(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                instances: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K49(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                id: match fields.next().expect("recipe field") {
                                    Value::K42(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                identity: match fields.next().expect("recipe field") {
                                    Value::K50(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                scope: match fields.next().expect("recipe field") {
                                    Value::K45(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                signature: match fields.next().expect("recipe field") {
                                    Value::K12(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K47 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K48 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K49 => {
                            let _ = tag;
                            Value::K49(Box::new(DynDefinitionSpecialization {
                                function: match fields.next().expect("recipe field") {
                                    Value::K70(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K0(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                declaration_actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K55(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                method_actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K55(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K51 => {
                            let _ = tag;
                            Value::K51(Box::new(DynMethodRecipe {
                                requirements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K7(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                signature: match fields.next().expect("recipe field") {
                                    Value::K12(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                default_definition: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K42(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            }))
                        }
                        Kind::K52 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K53 => {
                            let _ = tag;
                            Value::K53(Box::new(DynFunctionInstance {
                                definition: match fields.next().expect("recipe field") {
                                    Value::K42(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K0(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                declaration_actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K55(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                method_actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K55(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K54 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K56 => Value::K56(Box::new(match tag {
                            0 => DynAssociatedValueRecipe::Type(
                                match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynAssociatedValueRecipe::Constant {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K57(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K57 => Value::K57(Box::new(match tag {
                            0 => {
                                DynConstPattern::Value(match fields.next().expect("recipe field") {
                                    Value::K64(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            1 => DynConstPattern::Parameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => DynConstPattern::MethodParameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => DynConstPattern::Unary {
                                op: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K57(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            4 => DynConstPattern::Binary {
                                op: match fields.next().expect("recipe field") {
                                    Value::K75(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                left: Box::new(match fields.next().expect("recipe field") {
                                    Value::K57(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                right: Box::new(match fields.next().expect("recipe field") {
                                    Value::K57(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            5 => DynConstPattern::Convert {
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K57(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            6 => DynConstPattern::AssociatedProjection {
                                receiver: Box::new(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                application: Box::new(match fields.next().expect("recipe field") {
                                    Value::K10(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            7 => DynConstPattern::IntegerLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K76(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            8 => DynConstPattern::FloatLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K76(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K58 => {
                            let _ = tag;
                            Value::K58(Box::new(DynAssociatedDefinition {
                                carrier: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K8(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K56(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                requirements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K7(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K59 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K60 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K61 => Value::K61(Box::new(match tag {
                            0 => DynAssociatedSource::TraitDefault {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K62(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => DynAssociatedSource::ImplOverride {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K62(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K63 => Value::K63(Box::new(match tag {
                            0 => DynAssociatedValue::Type(
                                match fields.next().expect("recipe field") {
                                    Value::K0(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynAssociatedValue::Constant {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K0(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K64(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K65 => {
                            let _ = tag;
                            Value::K65(Box::new(DynAssociatedResolution {
                                member: match fields.next().expect("recipe field") {
                                    Value::K66(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K61(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K63(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K67 => {
                            let _ = tag;
                            Value::K67(Box::new(DynImplementationWitness {
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K0(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                application: match fields.next().expect("recipe field") {
                                    Value::K68(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                implementation: match fields.next().expect("recipe field") {
                                    Value::K62(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K55(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                associated: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K65(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K69 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K70 => Value::K70(Box::new(match tag {
                            0 => DynFunctionTarget::Core {
                                module: match fields.next().expect("recipe field") {
                                    Value::K43(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K44(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => DynFunctionTarget::Typed(
                                match fields.next().expect("recipe field") {
                                    Value::K71(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K72 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K73 => Value::K73(Box::new(match tag {
                            0 => {
                                DynPatternActual::Type(match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            1 => DynPatternActual::Const(
                                match fields.next().expect("recipe field") {
                                    Value::K57(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => {
                                DynPatternActual::Life(match fields.next().expect("recipe field") {
                                    Value::K5(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K77 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K78 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        _ => unreachable!("recipe leaf has no assembly"),
                    };
                    values.push(value);
                }
            }
        }
        Ok(values.pop().expect("complete recipe grammar"))
    }
}
