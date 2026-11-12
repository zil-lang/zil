//! Bounded recipe transport using explicit grammar stacks.
use super::*;
enum Write<'a> {
    K0(&'a DynTypeValue, usize),
    K1(&'a Type, usize),
    K2(&'a StorageLifeShape, usize),
    K3(&'a Vec<StorageBinder>, usize),
    K4(&'a StorageBinder, usize),
    K5(&'a DynInstantiationScope, usize),
    K6(&'a u32, usize),
    K7(&'a DynLifeEmbedding, usize),
    K8(&'a DynImplicitLifeKind, usize),
    K9(&'a DynImplicitLifeRoot, usize),
    K10(&'a DynImplicitLifeOccurrence, usize),
    K11(&'a Vec<u32>, usize),
    K12(&'a DynImplicitLifeDeclaration, usize),
    K13(&'a Vec<DynImplicitLifeOccurrence>, usize),
    K14(&'a DynLifePattern, usize),
    K15(&'a DynObligation, usize),
    K16(&'a DynTypePattern, usize),
    K17(&'a DynStructuralBound, usize),
    K18(&'a DynSelfPattern, usize),
    K19(&'a CallableKind, usize),
    K20(&'a DynSignatureRecipe, usize),
    K21(&'a DynConstPattern, usize),
    K22(&'a DynParameterRecipe, usize),
    K23(&'a PassingMode, usize),
    K24(&'a Vec<DynParameterRecipe>, usize),
    K25(&'a CallKind, usize),
    K26(&'a DynContractRecipe, usize),
    K27(&'a DynRequirementRecipe, usize),
    K28(&'a StoragePosition, usize),
    K29(&'a DynGuaranteeRecipe, usize),
    K30(&'a StorageSourceBinding, usize),
    K31(&'a DynEffectRecipe, usize),
    K32(&'a DynCleanupRecipe, usize),
    K33(&'a bool, usize),
    K34(&'a Vec<StoragePosition>, usize),
    K35(&'a DynCallAccess, usize),
    K36(&'a Vec<DynEffectRecipe>, usize),
    K37(&'a Option<Vec<DynRequirementRecipe>>, usize),
    K38(&'a Vec<DynRequirementRecipe>, usize),
    K39(&'a Option<Vec<DynGuaranteeRecipe>>, usize),
    K40(&'a Vec<DynGuaranteeRecipe>, usize),
    K41(&'a Option<Vec<StoragePosition>>, usize),
    K42(&'a Option<DynCallAccess>, usize),
    K43(&'a Option<DynResultSource>, usize),
    K44(&'a DynResultSource, usize),
    K45(&'a Option<DynCleanupRecipe>, usize),
    K46(&'a Option<Vec<DynEffectRecipe>>, usize),
    K47(&'a Option<DynResumeBound>, usize),
    K48(&'a DynResumeBound, usize),
    K49(&'a Option<DynThreadRequirements>, usize),
    K50(&'a DynThreadRequirements, usize),
    K51(&'a DynDefinitionId, usize),
    K52(&'a ModuleId, usize),
    K53(&'a NodeId, usize),
    K54(&'a DynDefinitionScope, usize),
    K55(&'a DynDefinitionDeclaration, usize),
    K56(&'a Vec<DynObligation>, usize),
    K57(&'a Vec<DynDefinitionSpecialization>, usize),
    K58(&'a DynDefinitionSpecialization, usize),
    K59(&'a LifeDeclarationIdentity, usize),
    K60(&'a DynMethodRecipe, usize),
    K61(&'a Option<DynDefinitionId>, usize),
    K62(&'a DynFunctionInstance, usize),
    K63(&'a Vec<DynLifeEmbedding>, usize),
    K64(&'a Vec<DynGenericActual>, usize),
    K65(&'a DynGenericActual, usize),
    K66(&'a DynAssociatedValueRecipe, usize),
    K67(&'a DynAssociatedDefinition, usize),
    K68(&'a Option<DynTypePattern>, usize),
    K69(&'a Option<DynAssociatedValueRecipe>, usize),
    K70(&'a DynAssociatedSource, usize),
    K71(&'a LifeDeclarationId, usize),
    K72(&'a DynAssociatedValue, usize),
    K73(&'a Constant, usize),
    K74(&'a DynAssociatedResolution, usize),
    K75(&'a DynMember, usize),
    K76(&'a DynImplementationWitness, usize),
    K77(&'a DynTraitApplication, usize),
    K78(&'a Vec<DynAssociatedResolution>, usize),
    K79(&'a DynFunctionTarget, usize),
    K80(&'a FunctionId, usize),
    K81(&'a Vec<DynPatternActual>, usize),
    K82(&'a DynPatternActual, usize),
    K83(&'a UnaryOp, usize),
    K84(&'a BinaryOp, usize),
    K85(&'a String, usize),
    K86(&'a Option<DynLifePattern>, usize),
    K87(&'a Vec<DynTypePattern>, usize),
    K88(&'a Vec<StorageSource>, usize),
    K89(&'a StorageSource, usize),
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
    K79,
    K80,
    K81,
    K82,
    K83,
    K84,
    K85,
    K86,
    K87,
    K88,
    K89,
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
    K5(Box<DynInstantiationScope>),
    K6(Box<u32>),
    K7(Box<DynLifeEmbedding>),
    K8(Box<DynImplicitLifeKind>),
    K9(Box<DynImplicitLifeRoot>),
    K10(Box<DynImplicitLifeOccurrence>),
    K12(Box<DynImplicitLifeDeclaration>),
    K14(Box<DynLifePattern>),
    K15(Box<DynObligation>),
    K16(Box<DynTypePattern>),
    K17(Box<DynStructuralBound>),
    K18(Box<DynSelfPattern>),
    K19(Box<CallableKind>),
    K20(Box<DynSignatureRecipe>),
    K21(Box<DynConstPattern>),
    K22(Box<DynParameterRecipe>),
    K23(Box<PassingMode>),
    K25(Box<CallKind>),
    K26(Box<DynContractRecipe>),
    K27(Box<DynRequirementRecipe>),
    K28(Box<StoragePosition>),
    K29(Box<DynGuaranteeRecipe>),
    K30(Box<StorageSourceBinding>),
    K31(Box<DynEffectRecipe>),
    K32(Box<DynCleanupRecipe>),
    K33(Box<bool>),
    K35(Box<DynCallAccess>),
    K44(Box<DynResultSource>),
    K48(Box<DynResumeBound>),
    K50(Box<DynThreadRequirements>),
    K51(Box<DynDefinitionId>),
    K52(Box<ModuleId>),
    K53(Box<NodeId>),
    K54(Box<DynDefinitionScope>),
    K55(Box<DynDefinitionDeclaration>),
    K58(Box<DynDefinitionSpecialization>),
    K59(Box<LifeDeclarationIdentity>),
    K60(Box<DynMethodRecipe>),
    K62(Box<DynFunctionInstance>),
    K65(Box<DynGenericActual>),
    K66(Box<DynAssociatedValueRecipe>),
    K67(Box<DynAssociatedDefinition>),
    K70(Box<DynAssociatedSource>),
    K71(Box<LifeDeclarationId>),
    K72(Box<DynAssociatedValue>),
    K73(Box<Constant>),
    K74(Box<DynAssociatedResolution>),
    K75(Box<DynMember>),
    K76(Box<DynImplementationWitness>),
    K77(Box<DynTraitApplication>),
    K79(Box<DynFunctionTarget>),
    K80(Box<FunctionId>),
    K82(Box<DynPatternActual>),
    K83(Box<UnaryOp>),
    K84(Box<BinaryOp>),
    K85(String),
    K89(Box<StorageSource>),
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
    pub(super) fn dyn_implicit_life_declaration(
        &mut self,
        value: &DynImplicitLifeDeclaration,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K12(value, depth))
    }
    pub(super) fn dyn_obligation(&mut self, value: &DynObligation, depth: usize) -> WireResult<()> {
        self.recipe_write(Write::K15(value, depth))
    }
    pub(super) fn dyn_definition_declaration(
        &mut self,
        value: &DynDefinitionDeclaration,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K55(value, depth))
    }
    pub(super) fn dyn_method_recipe(
        &mut self,
        value: &DynMethodRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K60(value, depth))
    }
    pub(super) fn dyn_function_instance(
        &mut self,
        value: &DynFunctionInstance,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K62(value, depth))
    }
    pub(super) fn dyn_associated_definition(
        &mut self,
        value: &DynAssociatedDefinition,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K67(value, depth))
    }
    pub(super) fn dyn_implementation_witness(
        &mut self,
        value: &DynImplementationWitness,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K76(value, depth))
    }
    pub(super) fn dyn_self_pattern(
        &mut self,
        value: &DynSelfPattern,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K18(value, depth))
    }
    pub(super) fn dyn_type_pattern(
        &mut self,
        value: &DynTypePattern,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K16(value, depth))
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
                    pending.push(Write::K6(&value.invocation_binder_count, depth + 1));
                    pending.push(Write::K6(&value.fixed_binder_count, depth + 1));
                }
                Write::K6(value, depth) => {
                    let _ = depth;
                    self.u32(*value)?;
                }
                Write::K7(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K4(&value.to, depth + 1));
                    pending.push(Write::K4(&value.from, depth + 1));
                }
                Write::K8(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynImplicitLifeKind::Address => {
                            self.u8(0)?;
                        }
                        DynImplicitLifeKind::Description => {
                            self.u8(1)?;
                        }
                        DynImplicitLifeKind::LifetimeArgument => {
                            self.u8(2)?;
                        }
                    }
                }
                Write::K9(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynImplicitLifeRoot::SelfPattern => {
                            self.u8(0)?;
                        }
                        DynImplicitLifeRoot::ImplementedTrait => {
                            self.u8(1)?;
                        }
                        DynImplicitLifeRoot::Supertrait(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynImplicitLifeRoot::Requirement(field_value_0) => {
                            self.u8(3)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynImplicitLifeRoot::AssociatedDefinition(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynImplicitLifeRoot::MethodSignature(field_value_0) => {
                            self.u8(5)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        DynImplicitLifeRoot::MethodDefinition(field_value_0) => {
                            self.u8(6)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K10(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K8(&value.kind, depth + 1));
                    pending.push(Write::K11(&value.path, depth + 1));
                    pending.push(Write::K9(&value.root, depth + 1));
                }
                Write::K11(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K6(item, depth));
                    }
                }
                Write::K12(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K13(&value.occurrences, depth + 1));
                }
                Write::K13(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K10(item, depth));
                    }
                }
                Write::K14(value, depth) => {
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
                        DynLifePattern::ImplicitDeclaration(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K15(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynObligation::Structural {
                            receiver: field_receiver,
                            bound: field_bound,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K17(field_bound, depth + 1));
                            pending.push(Write::K16(field_receiver, depth + 1));
                        }
                        DynObligation::Trait {
                            receiver: field_receiver,
                            application: field_application,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K18(field_application, depth + 1));
                            pending.push(Write::K16(field_receiver, depth + 1));
                        }
                        DynObligation::Callable {
                            receiver: field_receiver,
                            kind: field_kind,
                            quantified_lifetimes: field_quantified_lifetimes,
                            signature: field_signature,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K20(field_signature, depth + 1));
                            pending.push(Write::K6(field_quantified_lifetimes, depth + 1));
                            pending.push(Write::K19(field_kind, depth + 1));
                            pending.push(Write::K16(field_receiver, depth + 1));
                        }
                        DynObligation::TypeEqual {
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K16(field_right, depth + 1));
                            pending.push(Write::K16(field_left, depth + 1));
                        }
                        DynObligation::ConstEqual {
                            ty: field_ty,
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K21(field_right, depth + 1));
                            pending.push(Write::K21(field_left, depth + 1));
                            pending.push(Write::K16(field_ty, depth + 1));
                        }
                    }
                }
                Write::K16(value, depth) => {
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
                            pending.push(Write::K18(field_value_0, depth + 1));
                        }
                        DynTypePattern::Address {
                            mutable: field_mutable,
                            lifetime: field_lifetime,
                            inner: field_inner,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K16(field_inner, depth + 1));
                            pending.push(Write::K86(field_lifetime, depth + 1));
                            pending.push(Write::K33(field_mutable, depth + 1));
                        }
                        DynTypePattern::Tuple(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K87(field_value_0, depth + 1));
                        }
                        DynTypePattern::Array {
                            element: field_element,
                            length: field_length,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K21(field_length, depth + 1));
                            pending.push(Write::K16(field_element, depth + 1));
                        }
                        DynTypePattern::Slice(field_value_0) => {
                            self.u8(6)?;
                            pending.push(Write::K16(field_value_0, depth + 1));
                        }
                        DynTypePattern::Managed(field_value_0) => {
                            self.u8(7)?;
                            pending.push(Write::K16(field_value_0, depth + 1));
                        }
                        DynTypePattern::Desc {
                            lifetime: field_lifetime,
                            inner: field_inner,
                        } => {
                            self.u8(8)?;
                            pending.push(Write::K16(field_inner, depth + 1));
                            pending.push(Write::K86(field_lifetime, depth + 1));
                        }
                        DynTypePattern::OwnedStore(field_value_0) => {
                            self.u8(9)?;
                            pending.push(Write::K16(field_value_0, depth + 1));
                        }
                        DynTypePattern::ErrorUnion {
                            error: field_error,
                            result: field_result,
                        } => {
                            self.u8(10)?;
                            pending.push(Write::K16(field_result, depth + 1));
                            pending.push(Write::K16(field_error, depth + 1));
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
                            pending.push(Write::K18(field_application, depth + 1));
                            pending.push(Write::K16(field_receiver, depth + 1));
                        }
                    }
                }
                Write::K17(value, depth) => {
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
                Write::K18(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K81(&value.arguments, depth + 1));
                    pending.push(Write::K71(&value.declaration, depth + 1));
                }
                Write::K19(value, depth) => {
                    let _ = depth;
                    self.u8(match value {
                        CallableKind::FunctionPointer => 0,
                        CallableKind::Fn => 1,
                        CallableKind::FnMut => 2,
                        CallableKind::FnOnce => 3,
                    })?;
                }
                Write::K20(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K26(&value.contract, depth + 1));
                    pending.push(Write::K6(&value.invocation_binder_count, depth + 1));
                    pending.push(Write::K25(&value.call_kind, depth + 1));
                    pending.push(Write::K23(&value.result_mode, depth + 1));
                    pending.push(Write::K16(&value.result, depth + 1));
                    pending.push(Write::K24(&value.parameters, depth + 1));
                }
                Write::K21(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynConstPattern::Value(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K73(field_value_0, depth + 1));
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
                            pending.push(Write::K16(field_ty, depth + 1));
                            pending.push(Write::K21(field_value, depth + 1));
                            pending.push(Write::K83(field_op, depth + 1));
                        }
                        DynConstPattern::Binary {
                            op: field_op,
                            left: field_left,
                            right: field_right,
                            ty: field_ty,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K16(field_ty, depth + 1));
                            pending.push(Write::K21(field_right, depth + 1));
                            pending.push(Write::K21(field_left, depth + 1));
                            pending.push(Write::K84(field_op, depth + 1));
                        }
                        DynConstPattern::Convert {
                            value: field_value,
                            ty: field_ty,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K16(field_ty, depth + 1));
                            pending.push(Write::K21(field_value, depth + 1));
                        }
                        DynConstPattern::AssociatedProjection {
                            receiver: field_receiver,
                            application: field_application,
                            member: field_member,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K18(field_application, depth + 1));
                            pending.push(Write::K16(field_receiver, depth + 1));
                        }
                        DynConstPattern::IntegerLiteral(field_value_0) => {
                            self.u8(7)?;
                            pending.push(Write::K85(field_value_0, depth + 1));
                        }
                        DynConstPattern::FloatLiteral(field_value_0) => {
                            self.u8(8)?;
                            pending.push(Write::K85(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K22(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K23(&value.mode, depth + 1));
                    pending.push(Write::K16(&value.ty, depth + 1));
                }
                Write::K23(value, depth) => {
                    let _ = depth;
                    self.dyn_passing_mode(value)?;
                }
                Write::K24(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K22(item, depth));
                    }
                }
                Write::K25(value, depth) => {
                    let _ = depth;
                    self.dyn_call_kind(value)?;
                }
                Write::K26(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K49(&value.threads, depth + 1));
                    pending.push(Write::K47(&value.resume, depth + 1));
                    pending.push(Write::K46(&value.effects, depth + 1));
                    pending.push(Write::K45(&value.cleanup, depth + 1));
                    pending.push(Write::K43(&value.result, depth + 1));
                    pending.push(Write::K42(&value.call, depth + 1));
                    pending.push(Write::K41(&value.hold, depth + 1));
                    pending.push(Write::K39(&value.guarantees, depth + 1));
                    pending.push(Write::K37(&value.requires, depth + 1));
                }
                Write::K27(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynRequirementRecipe::Read(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K28(field_value_0, depth + 1));
                        }
                        DynRequirementRecipe::Write(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K28(field_value_0, depth + 1));
                        }
                        DynRequirementRecipe::Manage(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K28(field_value_0, depth + 1));
                        }
                        DynRequirementRecipe::Apart(field_value_0, field_value_1) => {
                            self.u8(3)?;
                            pending.push(Write::K28(field_value_1, depth + 1));
                            pending.push(Write::K28(field_value_0, depth + 1));
                        }
                        DynRequirementRecipe::State {
                            position: field_position,
                            ty: field_ty,
                            variant: field_variant,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_variant, depth + 1));
                            pending.push(Write::K16(field_ty, depth + 1));
                            pending.push(Write::K28(field_position, depth + 1));
                        }
                    }
                }
                Write::K28(value, depth) => {
                    let _ = depth;
                    self.storage_position(value)?;
                }
                Write::K29(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynGuaranteeRecipe::StorageSources(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K30(field_value_0, depth + 1));
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
                            pending.push(Write::K16(field_ty, depth + 1));
                            pending.push(Write::K28(field_position, depth + 1));
                        }
                    }
                }
                Write::K30(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K88(&value.sources, depth + 1));
                    pending.push(Write::K28(&value.position, depth + 1));
                }
                Write::K31(value, depth) => {
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
                            pending.push(Write::K28(field_value_0, depth + 1));
                        }
                        DynEffectRecipe::Cleanup(field_value_0) => {
                            self.u8(9)?;
                            pending.push(Write::K16(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K32(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K36(&value.effects, depth + 1));
                    pending.push(Write::K35(&value.access, depth + 1));
                    pending.push(Write::K34(&value.positions, depth + 1));
                    pending.push(Write::K33(&value.environment, depth + 1));
                }
                Write::K33(value, depth) => {
                    let _ = depth;
                    self.boolean(*value)?;
                }
                Write::K34(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K28(item, depth));
                    }
                }
                Write::K35(value, depth) => {
                    let _ = depth;
                    self.dyn_call_access(value)?;
                }
                Write::K36(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K31(item, depth));
                    }
                }
                Write::K37(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K38(item, depth));
                    }
                }
                Write::K38(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K27(item, depth));
                    }
                }
                Write::K39(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K40(item, depth));
                    }
                }
                Write::K40(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K29(item, depth));
                    }
                }
                Write::K41(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K34(item, depth));
                    }
                }
                Write::K42(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K35(item, depth));
                    }
                }
                Write::K43(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K44(item, depth));
                    }
                }
                Write::K44(value, depth) => {
                    let _ = depth;
                    self.dyn_result_source(value)?;
                }
                Write::K45(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K32(item, depth));
                    }
                }
                Write::K46(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K36(item, depth));
                    }
                }
                Write::K47(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K48(item, depth));
                    }
                }
                Write::K48(value, depth) => {
                    let _ = depth;
                    self.dyn_resume_bound(value)?;
                }
                Write::K49(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K50(item, depth));
                    }
                }
                Write::K50(value, depth) => {
                    let _ = depth;
                    self.dyn_thread_requirements(value)?;
                }
                Write::K51(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K53(&value.source, depth + 1));
                    pending.push(Write::K52(&value.module, depth + 1));
                }
                Write::K52(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K53(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K54(value, depth) => {
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
                Write::K55(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K20(&value.signature, depth + 1));
                    pending.push(Write::K54(&value.scope, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K59(&value.identity, depth + 1));
                    pending.push(Write::K51(&value.id, depth + 1));
                    pending.push(Write::K57(&value.instances, depth + 1));
                    pending.push(Write::K56(&value.requirements, depth + 1));
                }
                Write::K56(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K15(item, depth));
                    }
                }
                Write::K57(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K58(item, depth));
                    }
                }
                Write::K58(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K64(&value.method_actuals, depth + 1));
                    pending.push(Write::K64(&value.declaration_actuals, depth + 1));
                    pending.push(Write::K0(&value.receiver, depth + 1));
                    pending.push(Write::K79(&value.function, depth + 1));
                }
                Write::K59(value, depth) => {
                    let _ = depth;
                    self.declaration_identity(value)?;
                }
                Write::K60(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K61(&value.default_definition, depth + 1));
                    pending.push(Write::K20(&value.signature, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K56(&value.requirements, depth + 1));
                }
                Write::K61(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K51(item, depth));
                    }
                }
                Write::K62(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K64(&value.method_actuals, depth + 1));
                    pending.push(Write::K64(&value.declaration_actuals, depth + 1));
                    pending.push(Write::K0(&value.receiver, depth + 1));
                    pending.push(Write::K51(&value.definition, depth + 1));
                    pending.push(Write::K63(&value.header_embedding, depth + 1));
                    pending.push(Write::K5(&value.scope, depth + 1));
                }
                Write::K63(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K7(item, depth));
                    }
                }
                Write::K64(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K65(item, depth));
                    }
                }
                Write::K65(value, depth) => {
                    let _ = depth;
                    self.dyn_generic_actual(value)?;
                }
                Write::K66(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynAssociatedValueRecipe::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K16(field_value_0, depth + 1));
                        }
                        DynAssociatedValueRecipe::Constant {
                            ty: field_ty,
                            value: field_value,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K21(field_value, depth + 1));
                            pending.push(Write::K16(field_ty, depth + 1));
                        }
                    }
                }
                Write::K67(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K56(&value.requirements, depth + 1));
                    pending.push(Write::K69(&value.value, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K68(&value.carrier, depth + 1));
                }
                Write::K68(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K16(item, depth));
                    }
                }
                Write::K69(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K66(item, depth));
                    }
                }
                Write::K70(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynAssociatedSource::TraitDefault {
                            declaration: field_declaration,
                            member: field_member,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K71(field_declaration, depth + 1));
                        }
                        DynAssociatedSource::ImplOverride {
                            declaration: field_declaration,
                            member: field_member,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K71(field_declaration, depth + 1));
                        }
                    }
                }
                Write::K71(value, depth) => {
                    let _ = depth;
                    self.declaration_id(value)?;
                }
                Write::K72(value, depth) => {
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
                            pending.push(Write::K73(field_value, depth + 1));
                            pending.push(Write::K0(field_ty, depth + 1));
                        }
                    }
                }
                Write::K73(value, depth) => {
                    let _ = depth;
                    self.constant(value)?;
                }
                Write::K74(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K72(&value.value, depth + 1));
                    pending.push(Write::K70(&value.source, depth + 1));
                    pending.push(Write::K75(&value.member, depth + 1));
                }
                Write::K75(value, depth) => {
                    let _ = depth;
                    self.dyn_member(value)?;
                }
                Write::K76(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K78(&value.associated, depth + 1));
                    pending.push(Write::K64(&value.actuals, depth + 1));
                    pending.push(Write::K71(&value.implementation, depth + 1));
                    pending.push(Write::K77(&value.application, depth + 1));
                    pending.push(Write::K0(&value.receiver, depth + 1));
                    pending.push(Write::K63(&value.to_context, depth + 1));
                    pending.push(Write::K5(&value.scope, depth + 1));
                }
                Write::K77(value, depth) => {
                    let _ = depth;
                    self.dyn_trait_application(value)?;
                }
                Write::K78(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K74(item, depth));
                    }
                }
                Write::K79(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynFunctionTarget::Core {
                            module: field_module,
                            source: field_source,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K53(field_source, depth + 1));
                            pending.push(Write::K52(field_module, depth + 1));
                        }
                        DynFunctionTarget::Typed(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K80(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K80(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K81(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K82(item, depth));
                    }
                }
                Write::K82(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynPatternActual::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K16(field_value_0, depth + 1));
                        }
                        DynPatternActual::Const(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K21(field_value_0, depth + 1));
                        }
                        DynPatternActual::Life(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K14(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K83(value, depth) => {
                    let _ = depth;
                    self.unary(*value)?;
                }
                Write::K84(value, depth) => {
                    let _ = depth;
                    self.binary(*value)?;
                }
                Write::K85(value, depth) => {
                    let _ = depth;
                    self.text(value)?;
                }
                Write::K86(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K14(item, depth));
                    }
                }
                Write::K87(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K16(item, depth));
                    }
                }
                Write::K88(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K89(item, depth));
                    }
                }
                Write::K89(value, depth) => {
                    let _ = depth;
                    self.storage_source(value, depth)?;
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
    pub(super) fn dyn_implicit_life_declaration(
        &mut self,
        depth: usize,
    ) -> WireResult<DynImplicitLifeDeclaration> {
        let value = self.recipe_read(Kind::K12, depth)?;
        Ok(match value {
            Value::K12(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_obligation(&mut self, depth: usize) -> WireResult<DynObligation> {
        let value = self.recipe_read(Kind::K15, depth)?;
        Ok(match value {
            Value::K15(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_definition_declaration(
        &mut self,
        depth: usize,
    ) -> WireResult<DynDefinitionDeclaration> {
        let value = self.recipe_read(Kind::K55, depth)?;
        Ok(match value {
            Value::K55(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_method_recipe(&mut self, depth: usize) -> WireResult<DynMethodRecipe> {
        let value = self.recipe_read(Kind::K60, depth)?;
        Ok(match value {
            Value::K60(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_function_instance(
        &mut self,
        depth: usize,
    ) -> WireResult<DynFunctionInstance> {
        let value = self.recipe_read(Kind::K62, depth)?;
        Ok(match value {
            Value::K62(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_associated_definition(
        &mut self,
        depth: usize,
    ) -> WireResult<DynAssociatedDefinition> {
        let value = self.recipe_read(Kind::K67, depth)?;
        Ok(match value {
            Value::K67(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_implementation_witness(
        &mut self,
        depth: usize,
    ) -> WireResult<DynImplementationWitness> {
        let value = self.recipe_read(Kind::K76, depth)?;
        Ok(match value {
            Value::K76(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_self_pattern(&mut self, depth: usize) -> WireResult<DynSelfPattern> {
        let value = self.recipe_read(Kind::K18, depth)?;
        Ok(match value {
            Value::K18(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_type_pattern(&mut self, depth: usize) -> WireResult<DynTypePattern> {
        let value = self.recipe_read(Kind::K16, depth)?;
        Ok(match value {
            Value::K16(value) => *value,
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
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                    }
                    Kind::K6 => {
                        let _ = depth;
                        values.push(Value::K6(Box::new(self.u32()?)));
                    }
                    Kind::K7 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K4, depth + 1));
                        pending.push(Read::Start(Kind::K4, depth + 1));
                    }
                    Kind::K8 => {
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
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K10 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K8, depth + 1));
                        pending.push(Read::Start(Kind::K11, depth + 1));
                        pending.push(Read::Start(Kind::K9, depth + 1));
                    }
                    Kind::K11 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K6, depth));
                        }
                    }
                    Kind::K12 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 1));
                        pending.push(Read::Start(Kind::K13, depth + 1));
                    }
                    Kind::K13 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K10, depth));
                        }
                    }
                    Kind::K14 => {
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
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K15 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K17, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K18, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 4));
                                pending.push(Read::Start(Kind::K20, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 3));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K16 => {
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
                                pending.push(Read::Start(Kind::K18, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 3));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K86, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K87, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 2));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K86, depth + 1));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            10 => {
                                pending.push(Read::Finish(kind, 10, 2));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
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
                                pending.push(Read::Start(Kind::K18, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K17 => {
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
                    Kind::K18 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K81, depth + 1));
                        pending.push(Read::Start(Kind::K71, depth + 1));
                    }
                    Kind::K19 => {
                        let _ = depth;
                        values.push(Value::K19(Box::new(self.callable_kind()?)));
                    }
                    Kind::K20 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 6));
                        pending.push(Read::Start(Kind::K26, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K25, depth + 1));
                        pending.push(Read::Start(Kind::K23, depth + 1));
                        pending.push(Read::Start(Kind::K16, depth + 1));
                        pending.push(Read::Start(Kind::K24, depth + 1));
                    }
                    Kind::K21 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K73, depth + 1));
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
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K83, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 4));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K18, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 1));
                                pending.push(Read::Start(Kind::K85, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 1));
                                pending.push(Read::Start(Kind::K85, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K22 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K23, depth + 1));
                        pending.push(Read::Start(Kind::K16, depth + 1));
                    }
                    Kind::K23 => {
                        let _ = depth;
                        values.push(Value::K23(Box::new(self.dyn_passing_mode()?)));
                    }
                    Kind::K24 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K22, depth));
                        }
                    }
                    Kind::K25 => {
                        let _ = depth;
                        values.push(Value::K25(Box::new(self.dyn_call_kind()?)));
                    }
                    Kind::K26 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 9));
                        pending.push(Read::Start(Kind::K49, depth + 1));
                        pending.push(Read::Start(Kind::K47, depth + 1));
                        pending.push(Read::Start(Kind::K46, depth + 1));
                        pending.push(Read::Start(Kind::K45, depth + 1));
                        pending.push(Read::Start(Kind::K43, depth + 1));
                        pending.push(Read::Start(Kind::K42, depth + 1));
                        pending.push(Read::Start(Kind::K41, depth + 1));
                        pending.push(Read::Start(Kind::K39, depth + 1));
                        pending.push(Read::Start(Kind::K37, depth + 1));
                    }
                    Kind::K27 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K28 => {
                        let _ = depth;
                        values.push(Value::K28(Box::new(self.storage_position()?)));
                    }
                    Kind::K29 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K30, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K30 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K88, depth + 1));
                        pending.push(Read::Start(Kind::K28, depth + 1));
                    }
                    Kind::K31 => {
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
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K32 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K36, depth + 1));
                        pending.push(Read::Start(Kind::K35, depth + 1));
                        pending.push(Read::Start(Kind::K34, depth + 1));
                        pending.push(Read::Start(Kind::K33, depth + 1));
                    }
                    Kind::K33 => {
                        let _ = depth;
                        values.push(Value::K33(Box::new(self.boolean()?)));
                    }
                    Kind::K34 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K28, depth));
                        }
                    }
                    Kind::K35 => {
                        let _ = depth;
                        values.push(Value::K35(Box::new(self.dyn_call_access()?)));
                    }
                    Kind::K36 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K31, depth));
                        }
                    }
                    Kind::K37 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K38, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K38 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K27, depth));
                        }
                    }
                    Kind::K39 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K40, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K40 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K29, depth));
                        }
                    }
                    Kind::K41 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K34, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K42 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K35, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K43 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K44, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K44 => {
                        let _ = depth;
                        values.push(Value::K44(Box::new(self.dyn_result_source()?)));
                    }
                    Kind::K45 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K32, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K46 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K36, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K47 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K48, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K48 => {
                        let _ = depth;
                        values.push(Value::K48(Box::new(self.dyn_resume_bound()?)));
                    }
                    Kind::K49 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K50, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K50 => {
                        let _ = depth;
                        values.push(Value::K50(Box::new(self.dyn_thread_requirements()?)));
                    }
                    Kind::K51 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K53, depth + 1));
                        pending.push(Read::Start(Kind::K52, depth + 1));
                    }
                    Kind::K52 => {
                        let _ = depth;
                        values.push(Value::K52(Box::new(ModuleId(self.u32()?))));
                    }
                    Kind::K53 => {
                        let _ = depth;
                        values.push(Value::K53(Box::new(NodeId(self.u32()?))));
                    }
                    Kind::K54 => {
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
                    Kind::K55 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 7));
                        pending.push(Read::Start(Kind::K20, depth + 1));
                        pending.push(Read::Start(Kind::K54, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K59, depth + 1));
                        pending.push(Read::Start(Kind::K51, depth + 1));
                        pending.push(Read::Start(Kind::K57, depth + 1));
                        pending.push(Read::Start(Kind::K56, depth + 1));
                    }
                    Kind::K56 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K15, depth));
                        }
                    }
                    Kind::K57 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K58, depth));
                        }
                    }
                    Kind::K58 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K64, depth + 1));
                        pending.push(Read::Start(Kind::K64, depth + 1));
                        pending.push(Read::Start(Kind::K0, depth + 1));
                        pending.push(Read::Start(Kind::K79, depth + 1));
                    }
                    Kind::K59 => {
                        let _ = depth;
                        values.push(Value::K59(Box::new(self.declaration_identity()?)));
                    }
                    Kind::K60 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K61, depth + 1));
                        pending.push(Read::Start(Kind::K20, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K56, depth + 1));
                    }
                    Kind::K61 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K51, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K62 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 6));
                        pending.push(Read::Start(Kind::K64, depth + 1));
                        pending.push(Read::Start(Kind::K64, depth + 1));
                        pending.push(Read::Start(Kind::K0, depth + 1));
                        pending.push(Read::Start(Kind::K51, depth + 1));
                        pending.push(Read::Start(Kind::K63, depth + 1));
                        pending.push(Read::Start(Kind::K5, depth + 1));
                    }
                    Kind::K63 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K7, depth));
                        }
                    }
                    Kind::K64 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K65, depth));
                        }
                    }
                    Kind::K65 => {
                        let _ = depth;
                        values.push(Value::K65(Box::new(self.dyn_generic_actual()?)));
                    }
                    Kind::K66 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K67 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K56, depth + 1));
                        pending.push(Read::Start(Kind::K69, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K68, depth + 1));
                    }
                    Kind::K68 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K16, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K69 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K66, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K70 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K71, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K71, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K71 => {
                        let _ = depth;
                        values.push(Value::K71(Box::new(self.declaration_id()?)));
                    }
                    Kind::K72 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K0, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K73, depth + 1));
                                pending.push(Read::Start(Kind::K0, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K73 => {
                        let _ = depth;
                        values.push(Value::K73(Box::new(self.constant()?)));
                    }
                    Kind::K74 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K72, depth + 1));
                        pending.push(Read::Start(Kind::K70, depth + 1));
                        pending.push(Read::Start(Kind::K75, depth + 1));
                    }
                    Kind::K75 => {
                        let _ = depth;
                        values.push(Value::K75(Box::new(self.dyn_member()?)));
                    }
                    Kind::K76 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 7));
                        pending.push(Read::Start(Kind::K78, depth + 1));
                        pending.push(Read::Start(Kind::K64, depth + 1));
                        pending.push(Read::Start(Kind::K71, depth + 1));
                        pending.push(Read::Start(Kind::K77, depth + 1));
                        pending.push(Read::Start(Kind::K0, depth + 1));
                        pending.push(Read::Start(Kind::K63, depth + 1));
                        pending.push(Read::Start(Kind::K5, depth + 1));
                    }
                    Kind::K77 => {
                        let _ = depth;
                        values.push(Value::K77(Box::new(self.dyn_trait_application()?)));
                    }
                    Kind::K78 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K74, depth));
                        }
                    }
                    Kind::K79 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K53, depth + 1));
                                pending.push(Read::Start(Kind::K52, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K80, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K80 => {
                        let _ = depth;
                        values.push(Value::K80(Box::new(FunctionId(self.u32()?))));
                    }
                    Kind::K81 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K82, depth));
                        }
                    }
                    Kind::K82 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K14, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K83 => {
                        let _ = depth;
                        values.push(Value::K83(Box::new(self.unary()?)));
                    }
                    Kind::K84 => {
                        let _ = depth;
                        values.push(Value::K84(Box::new(self.binary()?)));
                    }
                    Kind::K85 => {
                        let _ = depth;
                        values.push(Value::K85(self.text()?));
                    }
                    Kind::K86 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K14, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K87 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K16, depth));
                        }
                    }
                    Kind::K88 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K89, depth));
                        }
                    }
                    Kind::K89 => {
                        let _ = depth;
                        values.push(Value::K89(Box::new(self.storage_source(depth)?)));
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
                        Kind::K5 => {
                            let _ = tag;
                            Value::K5(Box::new(DynInstantiationScope {
                                fixed_binder_count: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                invocation_binder_count: match fields.next().expect("recipe field")
                                {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K7 => {
                            let _ = tag;
                            Value::K7(Box::new(DynLifeEmbedding {
                                from: match fields.next().expect("recipe field") {
                                    Value::K4(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                to: match fields.next().expect("recipe field") {
                                    Value::K4(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K8 => Value::K8(Box::new(match tag {
                            0 => DynImplicitLifeKind::Address,
                            1 => DynImplicitLifeKind::Description,
                            2 => DynImplicitLifeKind::LifetimeArgument,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K9 => Value::K9(Box::new(match tag {
                            0 => DynImplicitLifeRoot::SelfPattern,
                            1 => DynImplicitLifeRoot::ImplementedTrait,
                            2 => DynImplicitLifeRoot::Supertrait(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => DynImplicitLifeRoot::Requirement(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            4 => DynImplicitLifeRoot::AssociatedDefinition(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            5 => DynImplicitLifeRoot::MethodSignature(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            6 => DynImplicitLifeRoot::MethodDefinition(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K10 => {
                            let _ = tag;
                            Value::K10(Box::new(DynImplicitLifeOccurrence {
                                root: match fields.next().expect("recipe field") {
                                    Value::K9(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K6(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                kind: match fields.next().expect("recipe field") {
                                    Value::K8(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K11 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K12 => {
                            let _ = tag;
                            Value::K12(Box::new(DynImplicitLifeDeclaration {
                                occurrences: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K10(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K13 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K14 => Value::K14(Box::new(match tag {
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
                            4 => DynLifePattern::ImplicitDeclaration(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K15 => Value::K15(Box::new(match tag {
                            0 => DynObligation::Structural {
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                bound: match fields.next().expect("recipe field") {
                                    Value::K17(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => DynObligation::Trait {
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                application: match fields.next().expect("recipe field") {
                                    Value::K18(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => DynObligation::Callable {
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                kind: match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                quantified_lifetimes: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                signature: Box::new(match fields.next().expect("recipe field") {
                                    Value::K20(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            3 => DynObligation::TypeEqual {
                                left: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                right: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => DynObligation::ConstEqual {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                left: match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                right: match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K16 => Value::K16(Box::new(match tag {
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
                                    Value::K18(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            3 => DynTypePattern::Address {
                                mutable: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                lifetime: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K14(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                inner: Box::new(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            4 => {
                                DynTypePattern::Tuple(match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K16(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                })
                            }
                            5 => DynTypePattern::Array {
                                element: Box::new(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                length: match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            6 => DynTypePattern::Slice(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            7 => DynTypePattern::Managed(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            8 => DynTypePattern::Desc {
                                lifetime: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K14(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                inner: Box::new(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            9 => DynTypePattern::OwnedStore(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            10 => DynTypePattern::ErrorUnion {
                                error: Box::new(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                result: Box::new(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
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
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                application: Box::new(match fields.next().expect("recipe field") {
                                    Value::K18(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K17 => Value::K17(Box::new(match tag {
                            0 => DynStructuralBound::Copy,
                            1 => DynStructuralBound::Value,
                            2 => DynStructuralBound::SyncDrop,
                            3 => DynStructuralBound::StoreElement,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K18 => {
                            let _ = tag;
                            Value::K18(Box::new(DynSelfPattern {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K71(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K82(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K20 => {
                            let _ = tag;
                            Value::K20(Box::new(DynSignatureRecipe {
                                parameters: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K22(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                result: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                result_mode: match fields.next().expect("recipe field") {
                                    Value::K23(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                call_kind: match fields.next().expect("recipe field") {
                                    Value::K25(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                invocation_binder_count: match fields.next().expect("recipe field")
                                {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                contract: match fields.next().expect("recipe field") {
                                    Value::K26(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K21 => Value::K21(Box::new(match tag {
                            0 => {
                                DynConstPattern::Value(match fields.next().expect("recipe field") {
                                    Value::K73(value) => *value,
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
                                    Value::K83(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            4 => DynConstPattern::Binary {
                                op: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                left: Box::new(match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                right: Box::new(match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            5 => DynConstPattern::Convert {
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            6 => DynConstPattern::AssociatedProjection {
                                receiver: Box::new(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                application: Box::new(match fields.next().expect("recipe field") {
                                    Value::K18(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            7 => DynConstPattern::IntegerLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K85(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            8 => DynConstPattern::FloatLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K85(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K22 => {
                            let _ = tag;
                            Value::K22(Box::new(DynParameterRecipe {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K23(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K24 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K26 => {
                            let _ = tag;
                            Value::K26(Box::new(DynContractRecipe {
                                requires: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::Rows(rows) => rows
                                            .into_iter()
                                            .map(|item| match item {
                                                Value::K27(value) => *value,
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
                                                Value::K29(value) => *value,
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
                                                Value::K28(value) => *value,
                                                _ => unreachable!("recipe field grammar"),
                                            })
                                            .collect(),
                                        _ => unreachable!("recipe row grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                call: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K35(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                result: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K44(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                cleanup: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K32(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                effects: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::Rows(rows) => rows
                                            .into_iter()
                                            .map(|item| match item {
                                                Value::K31(value) => *value,
                                                _ => unreachable!("recipe field grammar"),
                                            })
                                            .collect(),
                                        _ => unreachable!("recipe row grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                resume: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K48(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                threads: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K50(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            }))
                        }
                        Kind::K27 => Value::K27(Box::new(match tag {
                            0 => DynRequirementRecipe::Read(
                                match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynRequirementRecipe::Write(
                                match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => DynRequirementRecipe::Manage(
                                match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => DynRequirementRecipe::Apart(
                                match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            4 => DynRequirementRecipe::State {
                                position: match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                variant: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K29 => Value::K29(Box::new(match tag {
                            0 => DynGuaranteeRecipe::StorageSources(
                                match fields.next().expect("recipe field") {
                                    Value::K30(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynGuaranteeRecipe::ManageResult,
                            2 => DynGuaranteeRecipe::State {
                                position: match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                variant: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K30 => {
                            let _ = tag;
                            Value::K30(Box::new(StorageSourceBinding {
                                position: match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                sources: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K89(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K31 => Value::K31(Box::new(match tag {
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
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            9 => DynEffectRecipe::Cleanup(
                                match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K32 => {
                            let _ = tag;
                            Value::K32(Box::new(DynCleanupRecipe {
                                environment: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                positions: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K28(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                access: match fields.next().expect("recipe field") {
                                    Value::K35(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                effects: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K31(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K34 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K36 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K37 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K38 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K39 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K40 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K41 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K42 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K43 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K45 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K46 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K47 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K49 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K51 => {
                            let _ = tag;
                            Value::K51(Box::new(DynDefinitionId {
                                module: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K53(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K54 => Value::K54(Box::new(match tag {
                            0 => DynDefinitionScope::TraitDefault,
                            1 => DynDefinitionScope::ImplOverride,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K55 => {
                            let _ = tag;
                            Value::K55(Box::new(DynDefinitionDeclaration {
                                requirements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K15(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                instances: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K58(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                id: match fields.next().expect("recipe field") {
                                    Value::K51(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                identity: match fields.next().expect("recipe field") {
                                    Value::K59(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                scope: match fields.next().expect("recipe field") {
                                    Value::K54(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                signature: match fields.next().expect("recipe field") {
                                    Value::K20(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K56 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K57 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K58 => {
                            let _ = tag;
                            Value::K58(Box::new(DynDefinitionSpecialization {
                                function: match fields.next().expect("recipe field") {
                                    Value::K79(value) => *value,
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
                                            Value::K65(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                method_actuals: match fields.next().expect("recipe field") {
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
                        Kind::K60 => {
                            let _ = tag;
                            Value::K60(Box::new(DynMethodRecipe {
                                requirements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K15(value) => *value,
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
                                    Value::K20(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                default_definition: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K51(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            }))
                        }
                        Kind::K61 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K62 => {
                            let _ = tag;
                            Value::K62(Box::new(DynFunctionInstance {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K5(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                header_embedding: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K7(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                definition: match fields.next().expect("recipe field") {
                                    Value::K51(value) => *value,
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
                                            Value::K65(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                method_actuals: match fields.next().expect("recipe field") {
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
                        Kind::K63 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K64 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K66 => Value::K66(Box::new(match tag {
                            0 => DynAssociatedValueRecipe::Type(
                                match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynAssociatedValueRecipe::Constant {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K67 => {
                            let _ = tag;
                            Value::K67(Box::new(DynAssociatedDefinition {
                                carrier: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K16(value) => *value,
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
                                        Value::K66(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                requirements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K15(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K68 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K69 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K70 => Value::K70(Box::new(match tag {
                            0 => DynAssociatedSource::TraitDefault {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K71(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => DynAssociatedSource::ImplOverride {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K71(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K72 => Value::K72(Box::new(match tag {
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
                                    Value::K73(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K74 => {
                            let _ = tag;
                            Value::K74(Box::new(DynAssociatedResolution {
                                member: match fields.next().expect("recipe field") {
                                    Value::K75(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K70(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K72(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K76 => {
                            let _ = tag;
                            Value::K76(Box::new(DynImplementationWitness {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K5(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                to_context: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K7(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                receiver: match fields.next().expect("recipe field") {
                                    Value::K0(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                application: match fields.next().expect("recipe field") {
                                    Value::K77(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                implementation: match fields.next().expect("recipe field") {
                                    Value::K71(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K65(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                associated: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K74(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K78 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K79 => Value::K79(Box::new(match tag {
                            0 => DynFunctionTarget::Core {
                                module: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K53(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => DynFunctionTarget::Typed(
                                match fields.next().expect("recipe field") {
                                    Value::K80(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K81 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K82 => Value::K82(Box::new(match tag {
                            0 => {
                                DynPatternActual::Type(match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            1 => DynPatternActual::Const(
                                match fields.next().expect("recipe field") {
                                    Value::K21(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => {
                                DynPatternActual::Life(match fields.next().expect("recipe field") {
                                    Value::K14(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K86 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K87 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K88 => {
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
