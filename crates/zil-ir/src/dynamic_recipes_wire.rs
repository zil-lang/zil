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
    K30(&'a DynStorageSourceBindingRecipe, usize),
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
    K43(&'a Option<DynResultSourceRecipe>, usize),
    K44(&'a DynResultSourceRecipe, usize),
    K45(&'a Option<DynCleanupRecipe>, usize),
    K46(&'a Option<Vec<DynEffectRecipe>>, usize),
    K47(&'a Option<DynResumeBound>, usize),
    K48(&'a DynResumeBound, usize),
    K49(&'a Option<DynThreadRequirements>, usize),
    K50(&'a DynThreadRequirements, usize),
    K51(&'a DynStorageSourceRecipe, usize),
    K52(&'a StoredBorrowMode, usize),
    K53(&'a Vec<DynStorageSourceRecipe>, usize),
    K54(&'a DynDefinitionId, usize),
    K55(&'a ModuleId, usize),
    K56(&'a NodeId, usize),
    K57(&'a DynDefinitionScope, usize),
    K58(&'a DynDefinitionDeclaration, usize),
    K59(&'a Vec<DynObligation>, usize),
    K60(&'a Vec<DynDefinitionSpecialization>, usize),
    K61(&'a DynDefinitionSpecialization, usize),
    K62(&'a LifeDeclarationIdentity, usize),
    K63(&'a DynMethodRecipe, usize),
    K64(&'a Option<DynDefinitionId>, usize),
    K65(&'a DynFunctionInstance, usize),
    K66(&'a Vec<DynLifeEmbedding>, usize),
    K67(&'a Vec<DynGenericActual>, usize),
    K68(&'a DynGenericActual, usize),
    K69(&'a DynAssociatedValueRecipe, usize),
    K70(&'a DynAssociatedDefinition, usize),
    K71(&'a Option<DynTypePattern>, usize),
    K72(&'a Option<DynAssociatedValueRecipe>, usize),
    K73(&'a DynAssociatedSource, usize),
    K74(&'a LifeDeclarationId, usize),
    K75(&'a DynAssociatedValue, usize),
    K76(&'a Constant, usize),
    K77(&'a DynAssociatedResolution, usize),
    K78(&'a DynMember, usize),
    K79(&'a DynImplementationWitness, usize),
    K80(&'a DynTraitApplication, usize),
    K81(&'a Vec<DynAssociatedResolution>, usize),
    K82(&'a DynFunctionTarget, usize),
    K83(&'a FunctionId, usize),
    K84(&'a SemanticDefinitionId, usize),
    K85(&'a SemanticInstanceId, usize),
    K86(&'a SemanticModule, usize),
    K87(&'a Vec<SemanticDefinition>, usize),
    K88(&'a SemanticDefinition, usize),
    K89(&'a Vec<SemanticInstance>, usize),
    K90(&'a SemanticInstance, usize),
    K91(&'a Vec<SemanticSite>, usize),
    K92(&'a SemanticSite, usize),
    K93(&'a Vec<SemanticDynScopeBinding>, usize),
    K94(&'a SemanticDynScopeBinding, usize),
    K95(&'a SemanticDynOwner, usize),
    K96(&'a Option<SemanticDefinitionId>, usize),
    K97(&'a SemanticDefinitionKind, usize),
    K98(&'a Vec<SemanticFormal>, usize),
    K99(&'a SemanticFormal, usize),
    K100(&'a Vec<SemanticImplicitLifetime>, usize),
    K101(&'a SemanticImplicitLifetime, usize),
    K102(&'a Vec<SemanticRequirementRecipe>, usize),
    K103(&'a SemanticRequirementRecipe, usize),
    K104(&'a Vec<SemanticEnvironmentFormal>, usize),
    K105(&'a SemanticEnvironmentFormal, usize),
    K106(&'a SemanticDefinitionBody, usize),
    K107(&'a SemanticSignatureRecipe, usize),
    K108(&'a Vec<SemanticSiteRecipe>, usize),
    K109(&'a SemanticSiteRecipe, usize),
    K110(&'a LifeNominalKind, usize),
    K111(&'a SemanticNominalBody, usize),
    K112(&'a SemanticScopeRecipe, usize),
    K113(&'a SemanticTypeRecipe, usize),
    K114(&'a SemanticTraitRecipe, usize),
    K115(&'a Vec<SemanticFieldRecipe>, usize),
    K116(&'a SemanticFieldRecipe, usize),
    K117(&'a Vec<SemanticVariantRecipe>, usize),
    K118(&'a SemanticVariantRecipe, usize),
    K119(&'a String, usize),
    K120(&'a Vec<SemanticTypeRecipe>, usize),
    K121(&'a SemanticActualRecipe, usize),
    K122(&'a SemanticInstanceTarget, usize),
    K123(&'a SemanticLifetimeInstantiation, usize),
    K124(&'a Option<SemanticInstanceId>, usize),
    K125(&'a Vec<SemanticActual>, usize),
    K126(&'a SemanticActual, usize),
    K127(&'a Vec<SemanticHeaderBinding>, usize),
    K128(&'a SemanticHeaderBinding, usize),
    K129(&'a Option<SemanticEnvironmentInstance>, usize),
    K130(&'a SemanticEnvironmentInstance, usize),
    K131(&'a Option<SemanticTypeValue>, usize),
    K132(&'a SemanticTypeValue, usize),
    K133(&'a SemanticFunctionTarget, usize),
    K134(&'a SemanticClosureTarget, usize),
    K135(&'a ClosureId, usize),
    K136(&'a SemanticClassExpr, usize),
    K137(&'a SemanticCoreTypeSource, usize),
    K138(&'a Vec<SemanticClassExpr>, usize),
    K139(&'a SemanticScopeRef, usize),
    K140(&'a SemanticOperand, usize),
    K141(&'a Vec<LifeTypeProjection>, usize),
    K142(&'a LifeTypeProjection, usize),
    K143(&'a ValueId, usize),
    K144(&'a LocalId, usize),
    K145(&'a SemanticSiteId, usize),
    K146(&'a SemanticSiteRecipeKind, usize),
    K147(&'a Vec<SemanticActualRecipe>, usize),
    K148(&'a SemanticSiteAddress, usize),
    K149(&'a SemanticSiteAction, usize),
    K150(&'a SemanticEmissionRole, usize),
    K151(&'a BlockId, usize),
    K152(&'a SemanticSiteEdge, usize),
    K153(&'a SemanticLocalTarget, usize),
    K154(&'a SemanticRecipeOperand, usize),
    K155(&'a SemanticFormalRef, usize),
    K156(&'a SemanticLifeRecipe, usize),
    K157(&'a SemanticOuterLifeRecipe, usize),
    K158(&'a SemanticExactTypeRecipe, usize),
    K159(&'a SemanticConstRecipe, usize),
    K160(&'a SemanticRetainedShapeRecipe, usize),
    K161(&'a SemanticStorageShapeRecipe, usize),
    K162(&'a SemanticCodeKind, usize),
    K163(&'a SemanticDynInterfaceRecipe, usize),
    K164(&'a Vec<SemanticLifeRecipe>, usize),
    K165(&'a SemanticClassSequenceRecipe, usize),
    K166(&'a Vec<SemanticClassSequencePart>, usize),
    K167(&'a SemanticClassSequencePart, usize),
    K168(&'a Vec<SemanticSlotRecipe>, usize),
    K169(&'a SemanticSlotRecipe, usize),
    K170(&'a Vec<SemanticRetainedPart>, usize),
    K171(&'a SemanticRetainedPart, usize),
    K172(&'a Vec<SemanticNominalPart>, usize),
    K173(&'a SemanticNominalPart, usize),
    K174(&'a Vec<PlaceProjection>, usize),
    K175(&'a PlaceProjection, usize),
    K176(&'a Vec<SemanticPortPart>, usize),
    K177(&'a SemanticPortPart, usize),
    K178(&'a UnaryOp, usize),
    K179(&'a BinaryOp, usize),
    K180(&'a Vec<SemanticParameterRecipe>, usize),
    K181(&'a SemanticParameterRecipe, usize),
    K182(&'a SemanticDynContextRecipe, usize),
    K183(&'a Vec<SemanticAssociatedBindingRecipe>, usize),
    K184(&'a SemanticAssociatedBindingRecipe, usize),
    K185(&'a SemanticAssociatedRecipe, usize),
    K186(&'a Vec<DynPatternActual>, usize),
    K187(&'a DynPatternActual, usize),
    K188(&'a Option<DynLifePattern>, usize),
    K189(&'a Vec<DynTypePattern>, usize),
    K190(&'a VariantProjection, usize),
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
    K90,
    K91,
    K92,
    K93,
    K94,
    K95,
    K96,
    K97,
    K98,
    K99,
    K100,
    K101,
    K102,
    K103,
    K104,
    K105,
    K106,
    K107,
    K108,
    K109,
    K110,
    K111,
    K112,
    K113,
    K114,
    K115,
    K116,
    K117,
    K118,
    K119,
    K120,
    K121,
    K122,
    K123,
    K124,
    K125,
    K126,
    K127,
    K128,
    K129,
    K130,
    K131,
    K132,
    K133,
    K134,
    K135,
    K136,
    K137,
    K138,
    K139,
    K140,
    K141,
    K142,
    K143,
    K144,
    K145,
    K146,
    K147,
    K148,
    K149,
    K150,
    K151,
    K152,
    K153,
    K154,
    K155,
    K156,
    K157,
    K158,
    K159,
    K160,
    K161,
    K162,
    K163,
    K164,
    K165,
    K166,
    K167,
    K168,
    K169,
    K170,
    K171,
    K172,
    K173,
    K174,
    K175,
    K176,
    K177,
    K178,
    K179,
    K180,
    K181,
    K182,
    K183,
    K184,
    K185,
    K186,
    K187,
    K188,
    K189,
    K190,
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
    K30(Box<DynStorageSourceBindingRecipe>),
    K31(Box<DynEffectRecipe>),
    K32(Box<DynCleanupRecipe>),
    K33(Box<bool>),
    K35(Box<DynCallAccess>),
    K44(Box<DynResultSourceRecipe>),
    K48(Box<DynResumeBound>),
    K50(Box<DynThreadRequirements>),
    K51(Box<DynStorageSourceRecipe>),
    K52(Box<StoredBorrowMode>),
    K54(Box<DynDefinitionId>),
    K55(Box<ModuleId>),
    K56(Box<NodeId>),
    K57(Box<DynDefinitionScope>),
    K58(Box<DynDefinitionDeclaration>),
    K61(Box<DynDefinitionSpecialization>),
    K62(Box<LifeDeclarationIdentity>),
    K63(Box<DynMethodRecipe>),
    K65(Box<DynFunctionInstance>),
    K68(Box<DynGenericActual>),
    K69(Box<DynAssociatedValueRecipe>),
    K70(Box<DynAssociatedDefinition>),
    K73(Box<DynAssociatedSource>),
    K74(Box<LifeDeclarationId>),
    K75(Box<DynAssociatedValue>),
    K76(Box<Constant>),
    K77(Box<DynAssociatedResolution>),
    K78(Box<DynMember>),
    K79(Box<DynImplementationWitness>),
    K80(Box<DynTraitApplication>),
    K82(Box<DynFunctionTarget>),
    K83(Box<FunctionId>),
    K84(Box<SemanticDefinitionId>),
    K85(Box<SemanticInstanceId>),
    K86(Box<SemanticModule>),
    K88(Box<SemanticDefinition>),
    K90(Box<SemanticInstance>),
    K92(Box<SemanticSite>),
    K94(Box<SemanticDynScopeBinding>),
    K95(Box<SemanticDynOwner>),
    K97(Box<SemanticDefinitionKind>),
    K99(Box<SemanticFormal>),
    K101(Box<SemanticImplicitLifetime>),
    K103(Box<SemanticRequirementRecipe>),
    K105(Box<SemanticEnvironmentFormal>),
    K106(Box<SemanticDefinitionBody>),
    K107(Box<SemanticSignatureRecipe>),
    K109(Box<SemanticSiteRecipe>),
    K110(Box<LifeNominalKind>),
    K111(Box<SemanticNominalBody>),
    K112(Box<SemanticScopeRecipe>),
    K113(Box<SemanticTypeRecipe>),
    K114(Box<SemanticTraitRecipe>),
    K116(Box<SemanticFieldRecipe>),
    K118(Box<SemanticVariantRecipe>),
    K119(String),
    K121(Box<SemanticActualRecipe>),
    K122(Box<SemanticInstanceTarget>),
    K123(Box<SemanticLifetimeInstantiation>),
    K126(Box<SemanticActual>),
    K128(Box<SemanticHeaderBinding>),
    K130(Box<SemanticEnvironmentInstance>),
    K132(Box<SemanticTypeValue>),
    K133(Box<SemanticFunctionTarget>),
    K134(Box<SemanticClosureTarget>),
    K135(Box<ClosureId>),
    K136(Box<SemanticClassExpr>),
    K137(Box<SemanticCoreTypeSource>),
    K139(Box<SemanticScopeRef>),
    K140(Box<SemanticOperand>),
    K142(Box<LifeTypeProjection>),
    K143(Box<ValueId>),
    K144(Box<LocalId>),
    K145(Box<SemanticSiteId>),
    K146(Box<SemanticSiteRecipeKind>),
    K148(Box<SemanticSiteAddress>),
    K149(Box<SemanticSiteAction>),
    K150(Box<SemanticEmissionRole>),
    K151(Box<BlockId>),
    K152(Box<SemanticSiteEdge>),
    K153(Box<SemanticLocalTarget>),
    K154(Box<SemanticRecipeOperand>),
    K155(Box<SemanticFormalRef>),
    K156(Box<SemanticLifeRecipe>),
    K157(Box<SemanticOuterLifeRecipe>),
    K158(Box<SemanticExactTypeRecipe>),
    K159(Box<SemanticConstRecipe>),
    K160(Box<SemanticRetainedShapeRecipe>),
    K161(Box<SemanticStorageShapeRecipe>),
    K162(Box<SemanticCodeKind>),
    K163(Box<SemanticDynInterfaceRecipe>),
    K165(Box<SemanticClassSequenceRecipe>),
    K167(Box<SemanticClassSequencePart>),
    K169(Box<SemanticSlotRecipe>),
    K171(Box<SemanticRetainedPart>),
    K173(Box<SemanticNominalPart>),
    K175(Box<PlaceProjection>),
    K177(Box<SemanticPortPart>),
    K178(Box<UnaryOp>),
    K179(Box<BinaryOp>),
    K181(Box<SemanticParameterRecipe>),
    K182(Box<SemanticDynContextRecipe>),
    K184(Box<SemanticAssociatedBindingRecipe>),
    K185(Box<SemanticAssociatedRecipe>),
    K187(Box<DynPatternActual>),
    K190(Box<VariantProjection>),
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
        self.recipe_write(Write::K58(value, depth))
    }
    pub(super) fn dyn_method_recipe(
        &mut self,
        value: &DynMethodRecipe,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K63(value, depth))
    }
    pub(super) fn dyn_function_instance(
        &mut self,
        value: &DynFunctionInstance,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K65(value, depth))
    }
    pub(super) fn dyn_associated_definition(
        &mut self,
        value: &DynAssociatedDefinition,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K70(value, depth))
    }
    pub(super) fn dyn_implementation_witness(
        &mut self,
        value: &DynImplementationWitness,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K79(value, depth))
    }
    pub(super) fn semantic_module(
        &mut self,
        value: &SemanticModule,
        depth: usize,
    ) -> WireResult<()> {
        self.recipe_write(Write::K86(value, depth))
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
                            pending.push(Write::K188(field_lifetime, depth + 1));
                            pending.push(Write::K33(field_mutable, depth + 1));
                        }
                        DynTypePattern::Tuple(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K189(field_value_0, depth + 1));
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
                            pending.push(Write::K188(field_lifetime, depth + 1));
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
                        DynTypePattern::Semantic {
                            scope: field_scope,
                            ty: field_ty,
                        } => {
                            self.u8(14)?;
                            pending.push(Write::K113(field_ty, depth + 1));
                            pending.push(Write::K84(field_scope, depth + 1));
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
                    pending.push(Write::K186(&value.arguments, depth + 1));
                    pending.push(Write::K74(&value.declaration, depth + 1));
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
                            pending.push(Write::K76(field_value_0, depth + 1));
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
                            pending.push(Write::K178(field_op, depth + 1));
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
                            pending.push(Write::K179(field_op, depth + 1));
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
                            pending.push(Write::K119(field_value_0, depth + 1));
                        }
                        DynConstPattern::FloatLiteral(field_value_0) => {
                            self.u8(8)?;
                            pending.push(Write::K119(field_value_0, depth + 1));
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
                    pending.push(Write::K53(&value.sources, depth + 1));
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
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynResultSourceRecipe::Value => {
                            self.u8(0)?;
                        }
                        DynResultSourceRecipe::Create => {
                            self.u8(1)?;
                        }
                        DynResultSourceRecipe::Give => {
                            self.u8(2)?;
                        }
                        DynResultSourceRecipe::Borrow {
                            sources: field_sources,
                            mutable: field_mutable,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K33(field_mutable, depth + 1));
                            pending.push(Write::K53(field_sources, depth + 1));
                        }
                    }
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
                    match value {
                        DynStorageSourceRecipe::Exact(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K28(field_value_0, depth + 1));
                        }
                        DynStorageSourceRecipe::PlaceAddress {
                            place: field_place,
                            mode: field_mode,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K52(field_mode, depth + 1));
                            pending.push(Write::K28(field_place, depth + 1));
                        }
                        DynStorageSourceRecipe::OwnedView {
                            head: field_head,
                            leaf: field_leaf,
                            mode: field_mode,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K52(field_mode, depth + 1));
                            pending.push(Write::K16(field_leaf, depth + 1));
                            pending.push(Write::K28(field_head, depth + 1));
                        }
                        DynStorageSourceRecipe::ReborrowedLeaf {
                            source: field_source,
                            head: field_head,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K28(field_head, depth + 1));
                            pending.push(Write::K28(field_source, depth + 1));
                        }
                        DynStorageSourceRecipe::Static {
                            leaf: field_leaf,
                            mode: field_mode,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K52(field_mode, depth + 1));
                            pending.push(Write::K16(field_leaf, depth + 1));
                        }
                    }
                }
                Write::K52(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        StoredBorrowMode::SharedAddress => {
                            self.u8(0)?;
                        }
                        StoredBorrowMode::ExclusiveAddress => {
                            self.u8(1)?;
                        }
                        StoredBorrowMode::ReadOnlyDescription => {
                            self.u8(2)?;
                        }
                        StoredBorrowMode::WritableDescription => {
                            self.u8(3)?;
                        }
                    }
                }
                Write::K53(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K51(item, depth));
                    }
                }
                Write::K54(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K56(&value.source, depth + 1));
                    pending.push(Write::K55(&value.module, depth + 1));
                }
                Write::K55(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K56(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K57(value, depth) => {
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
                Write::K58(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K20(&value.signature, depth + 1));
                    pending.push(Write::K57(&value.scope, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K62(&value.identity, depth + 1));
                    pending.push(Write::K54(&value.id, depth + 1));
                    pending.push(Write::K60(&value.instances, depth + 1));
                    pending.push(Write::K59(&value.requirements, depth + 1));
                }
                Write::K59(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K15(item, depth));
                    }
                }
                Write::K60(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K61(item, depth));
                    }
                }
                Write::K61(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K67(&value.method_actuals, depth + 1));
                    pending.push(Write::K67(&value.declaration_actuals, depth + 1));
                    pending.push(Write::K0(&value.receiver, depth + 1));
                    pending.push(Write::K82(&value.function, depth + 1));
                }
                Write::K62(value, depth) => {
                    let _ = depth;
                    self.declaration_identity(value)?;
                }
                Write::K63(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K64(&value.default_definition, depth + 1));
                    pending.push(Write::K20(&value.signature, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K59(&value.requirements, depth + 1));
                }
                Write::K64(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K54(item, depth));
                    }
                }
                Write::K65(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K67(&value.method_actuals, depth + 1));
                    pending.push(Write::K67(&value.declaration_actuals, depth + 1));
                    pending.push(Write::K0(&value.receiver, depth + 1));
                    pending.push(Write::K54(&value.definition, depth + 1));
                    pending.push(Write::K66(&value.header_embedding, depth + 1));
                    pending.push(Write::K5(&value.scope, depth + 1));
                }
                Write::K66(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K7(item, depth));
                    }
                }
                Write::K67(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K68(item, depth));
                    }
                }
                Write::K68(value, depth) => {
                    let _ = depth;
                    self.dyn_generic_actual(value)?;
                }
                Write::K69(value, depth) => {
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
                Write::K70(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K59(&value.requirements, depth + 1));
                    pending.push(Write::K72(&value.value, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K71(&value.carrier, depth + 1));
                }
                Write::K71(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K16(item, depth));
                    }
                }
                Write::K72(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K69(item, depth));
                    }
                }
                Write::K73(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynAssociatedSource::TraitDefault {
                            declaration: field_declaration,
                            member: field_member,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                        }
                        DynAssociatedSource::ImplOverride {
                            declaration: field_declaration,
                            member: field_member,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                        }
                    }
                }
                Write::K74(value, depth) => {
                    let _ = depth;
                    self.declaration_id(value)?;
                }
                Write::K75(value, depth) => {
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
                            pending.push(Write::K76(field_value, depth + 1));
                            pending.push(Write::K0(field_ty, depth + 1));
                        }
                    }
                }
                Write::K76(value, depth) => {
                    let _ = depth;
                    self.constant(value)?;
                }
                Write::K77(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K75(&value.value, depth + 1));
                    pending.push(Write::K73(&value.source, depth + 1));
                    pending.push(Write::K78(&value.member, depth + 1));
                }
                Write::K78(value, depth) => {
                    let _ = depth;
                    self.dyn_member(value)?;
                }
                Write::K79(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K81(&value.associated, depth + 1));
                    pending.push(Write::K67(&value.actuals, depth + 1));
                    pending.push(Write::K74(&value.implementation, depth + 1));
                    pending.push(Write::K80(&value.application, depth + 1));
                    pending.push(Write::K0(&value.receiver, depth + 1));
                    pending.push(Write::K66(&value.to_context, depth + 1));
                    pending.push(Write::K5(&value.scope, depth + 1));
                }
                Write::K80(value, depth) => {
                    let _ = depth;
                    self.dyn_trait_application(value)?;
                }
                Write::K81(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K77(item, depth));
                    }
                }
                Write::K82(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynFunctionTarget::Core {
                            module: field_module,
                            source: field_source,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K56(field_source, depth + 1));
                            pending.push(Write::K55(field_module, depth + 1));
                        }
                        DynFunctionTarget::Typed(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K83(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K83(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K84(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K6(&value.index, depth + 1));
                    pending.push(Write::K55(&value.module, depth + 1));
                }
                Write::K85(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K6(&value.index, depth + 1));
                    pending.push(Write::K55(&value.module, depth + 1));
                }
                Write::K86(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K93(&value.dyn_scopes, depth + 1));
                    pending.push(Write::K91(&value.sites, depth + 1));
                    pending.push(Write::K89(&value.instances, depth + 1));
                    pending.push(Write::K87(&value.definitions, depth + 1));
                }
                Write::K87(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K88(item, depth));
                    }
                }
                Write::K88(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K106(&value.body, depth + 1));
                    pending.push(Write::K104(&value.environment, depth + 1));
                    pending.push(Write::K102(&value.requirements, depth + 1));
                    pending.push(Write::K100(&value.implicit_lifetimes, depth + 1));
                    pending.push(Write::K98(&value.parameters, depth + 1));
                    pending.push(Write::K97(&value.kind, depth + 1));
                    pending.push(Write::K96(&value.parent, depth + 1));
                    pending.push(Write::K62(&value.identity, depth + 1));
                    pending.push(Write::K84(&value.id, depth + 1));
                }
                Write::K89(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K90(item, depth));
                    }
                }
                Write::K90(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K131(&value.self_type, depth + 1));
                    pending.push(Write::K129(&value.environment, depth + 1));
                    pending.push(Write::K127(&value.header_embedding, depth + 1));
                    pending.push(Write::K125(&value.actuals, depth + 1));
                    pending.push(Write::K124(&value.parent, depth + 1));
                    pending.push(Write::K123(&value.lifetime_instantiation, depth + 1));
                    pending.push(Write::K122(&value.target, depth + 1));
                    pending.push(Write::K84(&value.definition, depth + 1));
                    pending.push(Write::K85(&value.id, depth + 1));
                }
                Write::K91(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K92(item, depth));
                    }
                }
                Write::K92(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K149(&value.action, depth + 1));
                    pending.push(Write::K148(&value.at, depth + 1));
                    pending.push(Write::K145(&value.recipe, depth + 1));
                    pending.push(Write::K85(&value.instance, depth + 1));
                }
                Write::K93(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K94(item, depth));
                    }
                }
                Write::K94(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K84(&value.scope, depth + 1));
                    pending.push(Write::K95(&value.owner, depth + 1));
                }
                Write::K95(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticDynOwner::Declaration(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K74(field_value_0, depth + 1));
                        }
                        SemanticDynOwner::MethodSignature {
                            declaration: field_declaration,
                            member: field_member,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                        }
                        SemanticDynOwner::MethodDefinition(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K54(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K96(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K84(item, depth));
                    }
                }
                Write::K97(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticDefinitionKind::Function => {
                            self.u8(0)?;
                        }
                        SemanticDefinitionKind::Method => {
                            self.u8(1)?;
                        }
                        SemanticDefinitionKind::Closure => {
                            self.u8(2)?;
                        }
                        SemanticDefinitionKind::TaskEntry => {
                            self.u8(3)?;
                        }
                        SemanticDefinitionKind::Nominal => {
                            self.u8(4)?;
                        }
                        SemanticDefinitionKind::Scope => {
                            self.u8(5)?;
                        }
                    }
                }
                Write::K98(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K99(item, depth));
                    }
                }
                Write::K99(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticFormal::Type {
                            requirements: field_requirements,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K102(field_requirements, depth + 1));
                        }
                        SemanticFormal::Const {
                            carrier: field_carrier,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K113(field_carrier, depth + 1));
                        }
                        SemanticFormal::Lifetime => {
                            self.u8(2)?;
                        }
                    }
                }
                Write::K100(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K101(item, depth));
                    }
                }
                Write::K101(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K11(&value.occurrence_path, depth + 1));
                    pending.push(Write::K6(&value.ordinal, depth + 1));
                }
                Write::K102(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K103(item, depth));
                    }
                }
                Write::K103(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticRequirementRecipe::Structural {
                            ty: field_ty,
                            bound: field_bound,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K17(field_bound, depth + 1));
                            pending.push(Write::K113(field_ty, depth + 1));
                        }
                        SemanticRequirementRecipe::Trait {
                            ty: field_ty,
                            application: field_application,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K114(field_application, depth + 1));
                            pending.push(Write::K113(field_ty, depth + 1));
                        }
                        SemanticRequirementRecipe::Callable {
                            ty: field_ty,
                            kind: field_kind,
                            signature: field_signature,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K107(field_signature, depth + 1));
                            pending.push(Write::K19(field_kind, depth + 1));
                            pending.push(Write::K113(field_ty, depth + 1));
                        }
                        SemanticRequirementRecipe::TypeEqual {
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K113(field_right, depth + 1));
                            pending.push(Write::K113(field_left, depth + 1));
                        }
                        SemanticRequirementRecipe::ConstEqual {
                            carrier: field_carrier,
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K159(field_right, depth + 1));
                            pending.push(Write::K159(field_left, depth + 1));
                            pending.push(Write::K113(field_carrier, depth + 1));
                        }
                        SemanticRequirementRecipe::LifeEqual {
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K156(field_right, depth + 1));
                            pending.push(Write::K156(field_left, depth + 1));
                        }
                        SemanticRequirementRecipe::Outlives {
                            longer: field_longer,
                            shorter: field_shorter,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K156(field_shorter, depth + 1));
                            pending.push(Write::K156(field_longer, depth + 1));
                        }
                        SemanticRequirementRecipe::ForAll {
                            lifetimes: field_lifetimes,
                            requirements: field_requirements,
                        } => {
                            self.u8(7)?;
                            pending.push(Write::K102(field_requirements, depth + 1));
                            pending.push(Write::K6(field_lifetimes, depth + 1));
                        }
                    }
                }
                Write::K104(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K105(item, depth));
                    }
                }
                Write::K105(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K121(&value.source, depth + 1));
                    pending.push(Write::K99(&value.kind, depth + 1));
                }
                Write::K106(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticDefinitionBody::Function {
                            signature: field_signature,
                            sites: field_sites,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K108(field_sites, depth + 1));
                            pending.push(Write::K107(field_signature, depth + 1));
                        }
                        SemanticDefinitionBody::Nominal {
                            declaration: field_declaration,
                            kind: field_kind,
                            body: field_body,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K111(field_body, depth + 1));
                            pending.push(Write::K110(field_kind, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                        }
                        SemanticDefinitionBody::Scope(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K112(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K107(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K25(&value.call_kind, depth + 1));
                    pending.push(Write::K23(&value.result_mode, depth + 1));
                    pending.push(Write::K113(&value.result, depth + 1));
                    pending.push(Write::K180(&value.parameters, depth + 1));
                    pending.push(Write::K6(&value.invocation_lifetimes, depth + 1));
                    pending.push(Write::K6(&value.bound_lifetimes, depth + 1));
                }
                Write::K108(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K109(item, depth));
                    }
                }
                Write::K109(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K146(&value.kind, depth + 1));
                    pending.push(Write::K11(&value.occurrence_path, depth + 1));
                    pending.push(Write::K145(&value.id, depth + 1));
                }
                Write::K110(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        LifeNominalKind::Record => {
                            self.u8(0)?;
                        }
                        LifeNominalKind::Package => {
                            self.u8(1)?;
                        }
                        LifeNominalKind::Resource => {
                            self.u8(2)?;
                        }
                        LifeNominalKind::Enum => {
                            self.u8(3)?;
                        }
                    }
                }
                Write::K111(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticNominalBody::Product {
                            fields: field_fields,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K115(field_fields, depth + 1));
                        }
                        SemanticNominalBody::Enum {
                            variants: field_variants,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K117(field_variants, depth + 1));
                        }
                    }
                }
                Write::K112(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticScopeRecipe::Trait {
                            declaration: field_declaration,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K74(field_declaration, depth + 1));
                        }
                        SemanticScopeRecipe::InherentImpl {
                            declaration: field_declaration,
                            self_type: field_self_type,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K113(field_self_type, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                        }
                        SemanticScopeRecipe::TraitImpl {
                            declaration: field_declaration,
                            self_type: field_self_type,
                            trait_application: field_trait_application,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K114(field_trait_application, depth + 1));
                            pending.push(Write::K113(field_self_type, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                        }
                    }
                }
                Write::K113(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticTypeRecipe::Atom(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K1(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Exact(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K158(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Formal(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K155(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                        SemanticTypeRecipe::SelfType { scope: field_scope } => {
                            self.u8(4)?;
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                        SemanticTypeRecipe::Nominal {
                            definition: field_definition,
                            arguments: field_arguments,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K147(field_arguments, depth + 1));
                            pending.push(Write::K84(field_definition, depth + 1));
                        }
                        SemanticTypeRecipe::AssociatedType {
                            receiver: field_receiver,
                            application: field_application,
                            member: field_member,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K114(field_application, depth + 1));
                            pending.push(Write::K113(field_receiver, depth + 1));
                        }
                        SemanticTypeRecipe::Tuple(field_value_0) => {
                            self.u8(7)?;
                            pending.push(Write::K120(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Array {
                            element: field_element,
                            length: field_length,
                        } => {
                            self.u8(8)?;
                            pending.push(Write::K159(field_length, depth + 1));
                            pending.push(Write::K113(field_element, depth + 1));
                        }
                        SemanticTypeRecipe::Slice(field_value_0) => {
                            self.u8(9)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Address {
                            mutable: field_mutable,
                            lifetime: field_lifetime,
                            inner: field_inner,
                        } => {
                            self.u8(10)?;
                            pending.push(Write::K113(field_inner, depth + 1));
                            pending.push(Write::K157(field_lifetime, depth + 1));
                            pending.push(Write::K33(field_mutable, depth + 1));
                        }
                        SemanticTypeRecipe::Desc {
                            lifetime: field_lifetime,
                            inner: field_inner,
                        } => {
                            self.u8(11)?;
                            pending.push(Write::K113(field_inner, depth + 1));
                            pending.push(Write::K157(field_lifetime, depth + 1));
                        }
                        SemanticTypeRecipe::Managed(field_value_0) => {
                            self.u8(12)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::OwnedStore(field_value_0) => {
                            self.u8(13)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Mutex(field_value_0) => {
                            self.u8(14)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::MutexGuard {
                            payload: field_payload,
                            retained: field_retained,
                        } => {
                            self.u8(15)?;
                            pending.push(Write::K160(field_retained, depth + 1));
                            pending.push(Write::K113(field_payload, depth + 1));
                        }
                        SemanticTypeRecipe::Registration(field_value_0) => {
                            self.u8(16)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::AsyncRegistration(field_value_0) => {
                            self.u8(17)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Task {
                            result: field_result,
                            retained: field_retained,
                        } => {
                            self.u8(18)?;
                            pending.push(Write::K160(field_retained, depth + 1));
                            pending.push(Write::K113(field_result, depth + 1));
                        }
                        SemanticTypeRecipe::ErrorUnion {
                            error: field_error,
                            result: field_result,
                        } => {
                            self.u8(19)?;
                            pending.push(Write::K113(field_result, depth + 1));
                            pending.push(Write::K113(field_error, depth + 1));
                        }
                        SemanticTypeRecipe::Qualified {
                            storage: field_storage,
                            life: field_life,
                        } => {
                            self.u8(20)?;
                            pending.push(Write::K161(field_life, depth + 1));
                            pending.push(Write::K113(field_storage, depth + 1));
                        }
                        SemanticTypeRecipe::Code {
                            kind: field_kind,
                            signature: field_signature,
                        } => {
                            self.u8(21)?;
                            pending.push(Write::K107(field_signature, depth + 1));
                            pending.push(Write::K162(field_kind, depth + 1));
                        }
                        SemanticTypeRecipe::Closure {
                            definition: field_definition,
                            arguments: field_arguments,
                            environment: field_environment,
                        } => {
                            self.u8(22)?;
                            pending.push(Write::K147(field_environment, depth + 1));
                            pending.push(Write::K147(field_arguments, depth + 1));
                            pending.push(Write::K84(field_definition, depth + 1));
                        }
                        SemanticTypeRecipe::Dyn {
                            owned: field_owned,
                            interface: field_interface,
                        } => {
                            self.u8(23)?;
                            pending.push(Write::K163(field_interface, depth + 1));
                            pending.push(Write::K33(field_owned, depth + 1));
                        }
                        SemanticTypeRecipe::Opaque {
                            definition: field_definition,
                            arguments: field_arguments,
                            hidden: field_hidden,
                        } => {
                            self.u8(24)?;
                            pending.push(Write::K113(field_hidden, depth + 1));
                            pending.push(Write::K147(field_arguments, depth + 1));
                            pending.push(Write::K84(field_definition, depth + 1));
                        }
                        SemanticTypeRecipe::AnonymousSum(field_value_0) => {
                            self.u8(25)?;
                            pending.push(Write::K120(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::UnresolvedNamed(field_value_0) => {
                            self.u8(26)?;
                            pending.push(Write::K119(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Unsupported(field_value_0) => {
                            self.u8(27)?;
                            pending.push(Write::K119(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K114(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K147(&value.arguments, depth + 1));
                    pending.push(Write::K74(&value.declaration, depth + 1));
                }
                Write::K115(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K116(item, depth));
                    }
                }
                Write::K116(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K113(&value.ty, depth + 1));
                    pending.push(Write::K119(&value.name, depth + 1));
                }
                Write::K117(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K118(item, depth));
                    }
                }
                Write::K118(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K120(&value.fields, depth + 1));
                    pending.push(Write::K119(&value.name, depth + 1));
                }
                Write::K119(value, depth) => {
                    let _ = depth;
                    self.text(value)?;
                }
                Write::K120(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K113(item, depth));
                    }
                }
                Write::K121(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticActualRecipe::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                        SemanticActualRecipe::Const {
                            carrier: field_carrier,
                            value: field_value,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K159(field_value, depth + 1));
                            pending.push(Write::K113(field_carrier, depth + 1));
                        }
                        SemanticActualRecipe::Life(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K156(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K122(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticInstanceTarget::Function(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K133(field_value_0, depth + 1));
                        }
                        SemanticInstanceTarget::Scope => {
                            self.u8(1)?;
                        }
                    }
                }
                Write::K123(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticLifetimeInstantiation::Rigid => {
                            self.u8(0)?;
                        }
                        SemanticLifetimeInstantiation::Partition { rigid: field_rigid } => {
                            self.u8(1)?;
                            pending.push(Write::K85(field_rigid, depth + 1));
                        }
                    }
                }
                Write::K124(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K85(item, depth));
                    }
                }
                Write::K125(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K126(item, depth));
                    }
                }
                Write::K126(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticActual::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K132(field_value_0, depth + 1));
                        }
                        SemanticActual::Const {
                            ty: field_ty,
                            value: field_value,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K76(field_value, depth + 1));
                            pending.push(Write::K132(field_ty, depth + 1));
                        }
                        SemanticActual::Life(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K136(field_value_0, depth + 1));
                        }
                        SemanticActual::CoreRecipe {
                            scope: field_scope,
                            recipe: field_recipe,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K121(field_recipe, depth + 1));
                            pending.push(Write::K85(field_scope, depth + 1));
                        }
                    }
                }
                Write::K127(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K128(item, depth));
                    }
                }
                Write::K128(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K136(&value.actual, depth + 1));
                    pending.push(Write::K4(&value.header, depth + 1));
                }
                Write::K129(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K130(item, depth));
                    }
                }
                Write::K130(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K125(&value.actuals, depth + 1));
                    pending.push(Write::K134(&value.closure, depth + 1));
                }
                Write::K131(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K132(item, depth));
                    }
                }
                Write::K132(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticTypeValue::Resolved {
                            ty: field_ty,
                            life: field_life,
                            classes: field_classes,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K138(field_classes, depth + 1));
                            pending.push(Write::K2(field_life, depth + 1));
                            pending.push(Write::K1(field_ty, depth + 1));
                        }
                        SemanticTypeValue::Core {
                            scope: field_scope,
                            source: field_source,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K137(field_source, depth + 1));
                            pending.push(Write::K85(field_scope, depth + 1));
                        }
                    }
                }
                Write::K133(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticFunctionTarget::Core {
                            module: field_module,
                            function: field_function,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K56(field_function, depth + 1));
                            pending.push(Write::K55(field_module, depth + 1));
                        }
                        SemanticFunctionTarget::CoreClosure {
                            module: field_module,
                            creator: field_creator,
                            expression: field_expression,
                            occurrence: field_occurrence,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_occurrence, depth + 1));
                            pending.push(Write::K56(field_expression, depth + 1));
                            pending.push(Write::K56(field_creator, depth + 1));
                            pending.push(Write::K55(field_module, depth + 1));
                        }
                        SemanticFunctionTarget::Typed(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K83(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K134(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticClosureTarget::Core {
                            module: field_module,
                            creator: field_creator,
                            expression: field_expression,
                            occurrence: field_occurrence,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_occurrence, depth + 1));
                            pending.push(Write::K56(field_expression, depth + 1));
                            pending.push(Write::K56(field_creator, depth + 1));
                            pending.push(Write::K55(field_module, depth + 1));
                        }
                        SemanticClosureTarget::Typed(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K135(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K135(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K136(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticClassExpr::Formal {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                        SemanticClassExpr::Implicit {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                        SemanticClassExpr::Bound {
                            depth: field_depth,
                            index: field_index,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K6(field_index, depth + 1));
                            pending.push(Write::K6(field_depth, depth + 1));
                        }
                        SemanticClassExpr::OuterOccurrence {
                            operand: field_operand,
                            path: field_path,
                            mode: field_mode,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K52(field_mode, depth + 1));
                            pending.push(Write::K141(field_path, depth + 1));
                            pending.push(Write::K140(field_operand, depth + 1));
                        }
                        SemanticClassExpr::IntrinsicClass {
                            operand: field_operand,
                            path: field_path,
                            class: field_class,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_class, depth + 1));
                            pending.push(Write::K141(field_path, depth + 1));
                            pending.push(Write::K140(field_operand, depth + 1));
                        }
                        SemanticClassExpr::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                    }
                }
                Write::K137(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticCoreTypeSource::OriginalRecipe(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                        SemanticCoreTypeSource::Expression {
                            module: field_module,
                            function: field_function,
                            expression: field_expression,
                            occurrence: field_occurrence,
                            recipe: field_recipe,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K113(field_recipe, depth + 1));
                            pending.push(Write::K6(field_occurrence, depth + 1));
                            pending.push(Write::K56(field_expression, depth + 1));
                            pending.push(Write::K56(field_function, depth + 1));
                            pending.push(Write::K55(field_module, depth + 1));
                        }
                    }
                }
                Write::K138(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K136(item, depth));
                    }
                }
                Write::K139(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticScopeRef::Ancestor(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K140(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOperand::Parameter(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        SemanticOperand::Capture(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        SemanticOperand::Receiver => {
                            self.u8(2)?;
                        }
                        SemanticOperand::Value(field_value_0) => {
                            self.u8(3)?;
                            pending.push(Write::K143(field_value_0, depth + 1));
                        }
                        SemanticOperand::Local(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K144(field_value_0, depth + 1));
                        }
                        SemanticOperand::TypeArgument {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                        SemanticOperand::Environment(field_value_0) => {
                            self.u8(6)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        SemanticOperand::CoreExpression {
                            expression: field_expression,
                            occurrence: field_occurrence,
                        } => {
                            self.u8(7)?;
                            pending.push(Write::K6(field_occurrence, depth + 1));
                            pending.push(Write::K56(field_expression, depth + 1));
                        }
                    }
                }
                Write::K141(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K142(item, depth));
                    }
                }
                Write::K142(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        LifeTypeProjection::Place(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K175(field_value_0, depth + 1));
                        }
                        LifeTypeProjection::TypeArgument(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        LifeTypeProjection::Pointee => {
                            self.u8(2)?;
                        }
                        LifeTypeProjection::Element => {
                            self.u8(3)?;
                        }
                        LifeTypeProjection::Parameter(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        LifeTypeProjection::Result => {
                            self.u8(5)?;
                        }
                        LifeTypeProjection::Port(field_value_0) => {
                            self.u8(6)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        LifeTypeProjection::AssociatedType(field_value_0) => {
                            self.u8(7)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K143(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K144(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K145(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K146(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticSiteRecipeKind::Construct {
                            result: field_result,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K113(field_result, depth + 1));
                        }
                        SemanticSiteRecipeKind::Call {
                            target: field_target,
                            actuals: field_actuals,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K147(field_actuals, depth + 1));
                            pending.push(Write::K84(field_target, depth + 1));
                        }
                        SemanticSiteRecipeKind::CallableCall {
                            callee: field_callee,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K113(field_callee, depth + 1));
                        }
                        SemanticSiteRecipeKind::TypeUse {
                            expected: field_expected,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K113(field_expected, depth + 1));
                        }
                        SemanticSiteRecipeKind::DeclareLocal {
                            expected: field_expected,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K113(field_expected, depth + 1));
                        }
                        SemanticSiteRecipeKind::Closure {
                            definition: field_definition,
                            environment: field_environment,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K147(field_environment, depth + 1));
                            pending.push(Write::K84(field_definition, depth + 1));
                        }
                    }
                }
                Write::K147(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K121(item, depth));
                    }
                }
                Write::K148(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticSiteAddress::Core {
                            expression: field_expression,
                            occurrence: field_occurrence,
                            role: field_role,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K150(field_role, depth + 1));
                            pending.push(Write::K6(field_occurrence, depth + 1));
                            pending.push(Write::K56(field_expression, depth + 1));
                        }
                        SemanticSiteAddress::Typed {
                            block: field_block,
                            instruction: field_instruction,
                            edge: field_edge,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K152(field_edge, depth + 1));
                            pending.push(Write::K6(field_instruction, depth + 1));
                            pending.push(Write::K151(field_block, depth + 1));
                        }
                    }
                }
                Write::K149(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticSiteAction::Construct {
                            result: field_result,
                            value: field_value,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K132(field_value, depth + 1));
                            pending.push(Write::K140(field_result, depth + 1));
                        }
                        SemanticSiteAction::Call {
                            target: field_target,
                            actuals: field_actuals,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K125(field_actuals, depth + 1));
                            pending.push(Write::K133(field_target, depth + 1));
                        }
                        SemanticSiteAction::CallableCall {
                            callee: field_callee,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K140(field_callee, depth + 1));
                        }
                        SemanticSiteAction::TypeUse {
                            subject: field_subject,
                            expected: field_expected,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K132(field_expected, depth + 1));
                            pending.push(Write::K140(field_subject, depth + 1));
                        }
                        SemanticSiteAction::DeclareLocal {
                            local: field_local,
                            expected: field_expected,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K132(field_expected, depth + 1));
                            pending.push(Write::K153(field_local, depth + 1));
                        }
                        SemanticSiteAction::Closure {
                            result: field_result,
                            closure: field_closure,
                            environment: field_environment,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K125(field_environment, depth + 1));
                            pending.push(Write::K134(field_closure, depth + 1));
                            pending.push(Write::K140(field_result, depth + 1));
                        }
                    }
                }
                Write::K150(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticEmissionRole::Construct => {
                            self.u8(0)?;
                        }
                        SemanticEmissionRole::Call => {
                            self.u8(1)?;
                        }
                        SemanticEmissionRole::Ascription => {
                            self.u8(2)?;
                        }
                        SemanticEmissionRole::Binding => {
                            self.u8(3)?;
                        }
                        SemanticEmissionRole::Closure => {
                            self.u8(4)?;
                        }
                    }
                }
                Write::K151(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K152(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticSiteEdge::Before => {
                            self.u8(0)?;
                        }
                        SemanticSiteEdge::After => {
                            self.u8(1)?;
                        }
                    }
                }
                Write::K153(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticLocalTarget::Core {
                            statement: field_statement,
                            occurrence: field_occurrence,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_occurrence, depth + 1));
                            pending.push(Write::K56(field_statement, depth + 1));
                        }
                        SemanticLocalTarget::Typed(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K144(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K154(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticRecipeOperand::TypeFormal(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K155(field_value_0, depth + 1));
                        }
                        SemanticRecipeOperand::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                        SemanticRecipeOperand::Parameter(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        SemanticRecipeOperand::Receiver => {
                            self.u8(3)?;
                        }
                    }
                }
                Write::K155(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K6(&value.ordinal, depth + 1));
                    pending.push(Write::K139(&value.scope, depth + 1));
                }
                Write::K156(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticLifeRecipe::Formal(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K155(field_value_0, depth + 1));
                        }
                        SemanticLifeRecipe::Implicit {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                        SemanticLifeRecipe::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                        SemanticLifeRecipe::Bound {
                            depth: field_depth,
                            index: field_index,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K6(field_index, depth + 1));
                            pending.push(Write::K6(field_depth, depth + 1));
                        }
                        SemanticLifeRecipe::Invocation(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        SemanticLifeRecipe::OuterOccurrence {
                            operand: field_operand,
                            path: field_path,
                            mode: field_mode,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K52(field_mode, depth + 1));
                            pending.push(Write::K141(field_path, depth + 1));
                            pending.push(Write::K154(field_operand, depth + 1));
                        }
                        SemanticLifeRecipe::IntrinsicClass {
                            operand: field_operand,
                            path: field_path,
                            class: field_class,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K6(field_class, depth + 1));
                            pending.push(Write::K141(field_path, depth + 1));
                            pending.push(Write::K154(field_operand, depth + 1));
                        }
                    }
                }
                Write::K157(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOuterLifeRecipe::Class(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K156(field_value_0, depth + 1));
                        }
                        SemanticOuterLifeRecipe::Static => {
                            self.u8(1)?;
                        }
                    }
                }
                Write::K158(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K164(&value.classes, depth + 1));
                    pending.push(Write::K2(&value.life, depth + 1));
                    pending.push(Write::K1(&value.ty, depth + 1));
                }
                Write::K159(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticConstRecipe::Value(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K76(field_value_0, depth + 1));
                        }
                        SemanticConstRecipe::IntegerLiteral(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K119(field_value_0, depth + 1));
                        }
                        SemanticConstRecipe::FloatLiteral(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K119(field_value_0, depth + 1));
                        }
                        SemanticConstRecipe::Formal(field_value_0) => {
                            self.u8(3)?;
                            pending.push(Write::K155(field_value_0, depth + 1));
                        }
                        SemanticConstRecipe::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K139(field_scope, depth + 1));
                        }
                        SemanticConstRecipe::Unary {
                            op: field_op,
                            operand_type: field_operand_type,
                            value: field_value,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K159(field_value, depth + 1));
                            pending.push(Write::K113(field_operand_type, depth + 1));
                            pending.push(Write::K178(field_op, depth + 1));
                        }
                        SemanticConstRecipe::Binary {
                            op: field_op,
                            operand_type: field_operand_type,
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K159(field_right, depth + 1));
                            pending.push(Write::K159(field_left, depth + 1));
                            pending.push(Write::K113(field_operand_type, depth + 1));
                            pending.push(Write::K179(field_op, depth + 1));
                        }
                        SemanticConstRecipe::Convert {
                            source_type: field_source_type,
                            target_type: field_target_type,
                            value: field_value,
                        } => {
                            self.u8(7)?;
                            pending.push(Write::K159(field_value, depth + 1));
                            pending.push(Write::K113(field_target_type, depth + 1));
                            pending.push(Write::K113(field_source_type, depth + 1));
                        }
                        SemanticConstRecipe::AssociatedConst {
                            receiver: field_receiver,
                            application: field_application,
                            member: field_member,
                        } => {
                            self.u8(8)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K114(field_application, depth + 1));
                            pending.push(Write::K113(field_receiver, depth + 1));
                        }
                    }
                }
                Write::K160(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K172(&value.nominal, depth + 1));
                    pending.push(Write::K170(&value.retained, depth + 1));
                    pending.push(Write::K176(&value.ports, depth + 1));
                    pending.push(Write::K165(&value.domain, depth + 1));
                }
                Write::K161(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K172(&value.nominal, depth + 1));
                    pending.push(Write::K170(&value.retained, depth + 1));
                    pending.push(Write::K168(&value.slots, depth + 1));
                    pending.push(Write::K165(&value.domain, depth + 1));
                }
                Write::K162(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticCodeKind::FunctionPointer => {
                            self.u8(0)?;
                        }
                        SemanticCodeKind::BorrowedFunctionPointer => {
                            self.u8(1)?;
                        }
                        SemanticCodeKind::ForeignCallback => {
                            self.u8(2)?;
                        }
                    }
                }
                Write::K163(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K183(&value.associated, depth + 1));
                    pending.push(Write::K182(&value.context, depth + 1));
                    pending.push(Write::K114(&value.application, depth + 1));
                }
                Write::K164(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K156(item, depth));
                    }
                }
                Write::K165(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K166(&value.parts, depth + 1));
                }
                Write::K166(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K167(item, depth));
                    }
                }
                Write::K167(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticClassSequencePart::One(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K156(field_value_0, depth + 1));
                        }
                        SemanticClassSequencePart::AllClasses {
                            ty: field_ty,
                            path: field_path,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K141(field_path, depth + 1));
                            pending.push(Write::K113(field_ty, depth + 1));
                        }
                    }
                }
                Write::K168(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K169(item, depth));
                    }
                }
                Write::K169(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K156(&value.class, depth + 1));
                    pending.push(Write::K52(&value.mode, depth + 1));
                    pending.push(Write::K174(&value.path, depth + 1));
                }
                Write::K170(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K171(item, depth));
                    }
                }
                Write::K171(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticRetainedPart::One {
                            path: field_path,
                            child_class: field_child_class,
                            parent: field_parent,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K156(field_parent, depth + 1));
                            pending.push(Write::K6(field_child_class, depth + 1));
                            pending.push(Write::K141(field_path, depth + 1));
                        }
                        SemanticRetainedPart::All {
                            path: field_path,
                            parents: field_parents,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K165(field_parents, depth + 1));
                            pending.push(Write::K141(field_path, depth + 1));
                        }
                    }
                }
                Write::K172(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K173(item, depth));
                    }
                }
                Write::K173(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticNominalPart::One {
                            path: field_path,
                            declaration: field_declaration,
                            argument: field_argument,
                            class: field_class,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K156(field_class, depth + 1));
                            pending.push(Write::K6(field_argument, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                            pending.push(Write::K141(field_path, depth + 1));
                        }
                        SemanticNominalPart::FromType {
                            path: field_path,
                            ty: field_ty,
                            classes: field_classes,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K165(field_classes, depth + 1));
                            pending.push(Write::K113(field_ty, depth + 1));
                            pending.push(Write::K141(field_path, depth + 1));
                        }
                    }
                }
                Write::K174(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K175(item, depth));
                    }
                }
                Write::K175(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        PlaceProjection::Field(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        PlaceProjection::Variant(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K190(field_value_0, depth + 1));
                        }
                        PlaceProjection::ErrorArm(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K33(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K176(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K177(item, depth));
                    }
                }
                Write::K177(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticPortPart::One {
                            leaf: field_leaf,
                            mode: field_mode,
                            class: field_class,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K156(field_class, depth + 1));
                            pending.push(Write::K52(field_mode, depth + 1));
                            pending.push(Write::K113(field_leaf, depth + 1));
                        }
                        SemanticPortPart::FromType {
                            ty: field_ty,
                            classes: field_classes,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K165(field_classes, depth + 1));
                            pending.push(Write::K113(field_ty, depth + 1));
                        }
                    }
                }
                Write::K178(value, depth) => {
                    let _ = depth;
                    self.unary(*value)?;
                }
                Write::K179(value, depth) => {
                    let _ = depth;
                    self.binary(*value)?;
                }
                Write::K180(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K181(item, depth));
                    }
                }
                Write::K181(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K23(&value.mode, depth + 1));
                    pending.push(Write::K113(&value.ty, depth + 1));
                }
                Write::K182(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticDynContextRecipe::Closed => {
                            self.u8(0)?;
                        }
                        SemanticDynContextRecipe::Type(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K183(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K184(item, depth));
                    }
                }
                Write::K184(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K185(&value.value, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K114(&value.application, depth + 1));
                }
                Write::K185(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticAssociatedRecipe::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K113(field_value_0, depth + 1));
                        }
                        SemanticAssociatedRecipe::Const {
                            carrier: field_carrier,
                            value: field_value,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K159(field_value, depth + 1));
                            pending.push(Write::K113(field_carrier, depth + 1));
                        }
                    }
                }
                Write::K186(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K187(item, depth));
                    }
                }
                Write::K187(value, depth) => {
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
                Write::K188(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K14(item, depth));
                    }
                }
                Write::K189(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K16(item, depth));
                    }
                }
                Write::K190(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K6(&value.index, depth + 1));
                    pending.push(Write::K6(&value.variant, depth + 1));
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
        let value = self.recipe_read(Kind::K58, depth)?;
        Ok(match value {
            Value::K58(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_method_recipe(&mut self, depth: usize) -> WireResult<DynMethodRecipe> {
        let value = self.recipe_read(Kind::K63, depth)?;
        Ok(match value {
            Value::K63(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_function_instance(
        &mut self,
        depth: usize,
    ) -> WireResult<DynFunctionInstance> {
        let value = self.recipe_read(Kind::K65, depth)?;
        Ok(match value {
            Value::K65(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_associated_definition(
        &mut self,
        depth: usize,
    ) -> WireResult<DynAssociatedDefinition> {
        let value = self.recipe_read(Kind::K70, depth)?;
        Ok(match value {
            Value::K70(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn dyn_implementation_witness(
        &mut self,
        depth: usize,
    ) -> WireResult<DynImplementationWitness> {
        let value = self.recipe_read(Kind::K79, depth)?;
        Ok(match value {
            Value::K79(value) => *value,
            _ => unreachable!("recipe field grammar"),
        })
    }
    pub(super) fn semantic_module(&mut self, depth: usize) -> WireResult<SemanticModule> {
        let value = self.recipe_read(Kind::K86, depth)?;
        Ok(match value {
            Value::K86(value) => *value,
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
                                pending.push(Read::Start(Kind::K188, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K189, depth + 1));
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
                                pending.push(Read::Start(Kind::K188, depth + 1));
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
                            14 => {
                                pending.push(Read::Finish(kind, 14, 2));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
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
                        pending.push(Read::Start(Kind::K186, depth + 1));
                        pending.push(Read::Start(Kind::K74, depth + 1));
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
                                pending.push(Read::Start(Kind::K76, depth + 1));
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
                                pending.push(Read::Start(Kind::K178, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 4));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K179, depth + 1));
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
                                pending.push(Read::Start(Kind::K119, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 1));
                                pending.push(Read::Start(Kind::K119, depth + 1));
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
                        pending.push(Read::Start(Kind::K53, depth + 1));
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
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                                pending.push(Read::Start(Kind::K53, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
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
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K52, depth + 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 3));
                                pending.push(Read::Start(Kind::K52, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                                pending.push(Read::Start(Kind::K28, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 2));
                                pending.push(Read::Start(Kind::K52, depth + 1));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K52 => {
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
                    Kind::K53 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K51, depth));
                        }
                    }
                    Kind::K54 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K56, depth + 1));
                        pending.push(Read::Start(Kind::K55, depth + 1));
                    }
                    Kind::K55 => {
                        let _ = depth;
                        values.push(Value::K55(Box::new(ModuleId(self.u32()?))));
                    }
                    Kind::K56 => {
                        let _ = depth;
                        values.push(Value::K56(Box::new(NodeId(self.u32()?))));
                    }
                    Kind::K57 => {
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
                    Kind::K58 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 7));
                        pending.push(Read::Start(Kind::K20, depth + 1));
                        pending.push(Read::Start(Kind::K57, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K62, depth + 1));
                        pending.push(Read::Start(Kind::K54, depth + 1));
                        pending.push(Read::Start(Kind::K60, depth + 1));
                        pending.push(Read::Start(Kind::K59, depth + 1));
                    }
                    Kind::K59 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K15, depth));
                        }
                    }
                    Kind::K60 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K61, depth));
                        }
                    }
                    Kind::K61 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K67, depth + 1));
                        pending.push(Read::Start(Kind::K67, depth + 1));
                        pending.push(Read::Start(Kind::K0, depth + 1));
                        pending.push(Read::Start(Kind::K82, depth + 1));
                    }
                    Kind::K62 => {
                        let _ = depth;
                        values.push(Value::K62(Box::new(self.declaration_identity()?)));
                    }
                    Kind::K63 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K64, depth + 1));
                        pending.push(Read::Start(Kind::K20, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K59, depth + 1));
                    }
                    Kind::K64 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K54, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K65 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 6));
                        pending.push(Read::Start(Kind::K67, depth + 1));
                        pending.push(Read::Start(Kind::K67, depth + 1));
                        pending.push(Read::Start(Kind::K0, depth + 1));
                        pending.push(Read::Start(Kind::K54, depth + 1));
                        pending.push(Read::Start(Kind::K66, depth + 1));
                        pending.push(Read::Start(Kind::K5, depth + 1));
                    }
                    Kind::K66 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K7, depth));
                        }
                    }
                    Kind::K67 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K68, depth));
                        }
                    }
                    Kind::K68 => {
                        let _ = depth;
                        values.push(Value::K68(Box::new(self.dyn_generic_actual()?)));
                    }
                    Kind::K69 => {
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
                    Kind::K70 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K59, depth + 1));
                        pending.push(Read::Start(Kind::K72, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K71, depth + 1));
                    }
                    Kind::K71 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K16, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K72 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K69, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K73 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K74 => {
                        let _ = depth;
                        values.push(Value::K74(Box::new(self.declaration_id()?)));
                    }
                    Kind::K75 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K0, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K76, depth + 1));
                                pending.push(Read::Start(Kind::K0, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K76 => {
                        let _ = depth;
                        values.push(Value::K76(Box::new(self.constant()?)));
                    }
                    Kind::K77 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K75, depth + 1));
                        pending.push(Read::Start(Kind::K73, depth + 1));
                        pending.push(Read::Start(Kind::K78, depth + 1));
                    }
                    Kind::K78 => {
                        let _ = depth;
                        values.push(Value::K78(Box::new(self.dyn_member()?)));
                    }
                    Kind::K79 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 7));
                        pending.push(Read::Start(Kind::K81, depth + 1));
                        pending.push(Read::Start(Kind::K67, depth + 1));
                        pending.push(Read::Start(Kind::K74, depth + 1));
                        pending.push(Read::Start(Kind::K80, depth + 1));
                        pending.push(Read::Start(Kind::K0, depth + 1));
                        pending.push(Read::Start(Kind::K66, depth + 1));
                        pending.push(Read::Start(Kind::K5, depth + 1));
                    }
                    Kind::K80 => {
                        let _ = depth;
                        values.push(Value::K80(Box::new(self.dyn_trait_application()?)));
                    }
                    Kind::K81 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K77, depth));
                        }
                    }
                    Kind::K82 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K55, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K83, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K83 => {
                        let _ = depth;
                        values.push(Value::K83(Box::new(FunctionId(self.u32()?))));
                    }
                    Kind::K84 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K55, depth + 1));
                    }
                    Kind::K85 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K55, depth + 1));
                    }
                    Kind::K86 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K93, depth + 1));
                        pending.push(Read::Start(Kind::K91, depth + 1));
                        pending.push(Read::Start(Kind::K89, depth + 1));
                        pending.push(Read::Start(Kind::K87, depth + 1));
                    }
                    Kind::K87 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K88, depth));
                        }
                    }
                    Kind::K88 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 9));
                        pending.push(Read::Start(Kind::K106, depth + 1));
                        pending.push(Read::Start(Kind::K104, depth + 1));
                        pending.push(Read::Start(Kind::K102, depth + 1));
                        pending.push(Read::Start(Kind::K100, depth + 1));
                        pending.push(Read::Start(Kind::K98, depth + 1));
                        pending.push(Read::Start(Kind::K97, depth + 1));
                        pending.push(Read::Start(Kind::K96, depth + 1));
                        pending.push(Read::Start(Kind::K62, depth + 1));
                        pending.push(Read::Start(Kind::K84, depth + 1));
                    }
                    Kind::K89 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K90, depth));
                        }
                    }
                    Kind::K90 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 9));
                        pending.push(Read::Start(Kind::K131, depth + 1));
                        pending.push(Read::Start(Kind::K129, depth + 1));
                        pending.push(Read::Start(Kind::K127, depth + 1));
                        pending.push(Read::Start(Kind::K125, depth + 1));
                        pending.push(Read::Start(Kind::K124, depth + 1));
                        pending.push(Read::Start(Kind::K123, depth + 1));
                        pending.push(Read::Start(Kind::K122, depth + 1));
                        pending.push(Read::Start(Kind::K84, depth + 1));
                        pending.push(Read::Start(Kind::K85, depth + 1));
                    }
                    Kind::K91 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K92, depth));
                        }
                    }
                    Kind::K92 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K149, depth + 1));
                        pending.push(Read::Start(Kind::K148, depth + 1));
                        pending.push(Read::Start(Kind::K145, depth + 1));
                        pending.push(Read::Start(Kind::K85, depth + 1));
                    }
                    Kind::K93 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K94, depth));
                        }
                    }
                    Kind::K94 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K84, depth + 1));
                        pending.push(Read::Start(Kind::K95, depth + 1));
                    }
                    Kind::K95 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K54, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K96 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K84, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K97 => {
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
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K98 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K99, depth));
                        }
                    }
                    Kind::K99 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K102, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K100 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K101, depth));
                        }
                    }
                    Kind::K101 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K11, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                    }
                    Kind::K102 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K103, depth));
                        }
                    }
                    Kind::K103 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K17, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K114, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 3));
                                pending.push(Read::Start(Kind::K107, depth + 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 3));
                                pending.push(Read::Start(Kind::K159, depth + 1));
                                pending.push(Read::Start(Kind::K159, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 2));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 2));
                                pending.push(Read::Start(Kind::K102, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K104 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K105, depth));
                        }
                    }
                    Kind::K105 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K121, depth + 1));
                        pending.push(Read::Start(Kind::K99, depth + 1));
                    }
                    Kind::K106 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K108, depth + 1));
                                pending.push(Read::Start(Kind::K107, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 3));
                                pending.push(Read::Start(Kind::K111, depth + 1));
                                pending.push(Read::Start(Kind::K110, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K112, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K107 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 6));
                        pending.push(Read::Start(Kind::K25, depth + 1));
                        pending.push(Read::Start(Kind::K23, depth + 1));
                        pending.push(Read::Start(Kind::K113, depth + 1));
                        pending.push(Read::Start(Kind::K180, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                    }
                    Kind::K108 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K109, depth));
                        }
                    }
                    Kind::K109 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K146, depth + 1));
                        pending.push(Read::Start(Kind::K11, depth + 1));
                        pending.push(Read::Start(Kind::K145, depth + 1));
                    }
                    Kind::K110 => {
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
                    Kind::K111 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K115, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K117, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K112 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 3));
                                pending.push(Read::Start(Kind::K114, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K113 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K1, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K158, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K147, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K114, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 1));
                                pending.push(Read::Start(Kind::K120, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 2));
                                pending.push(Read::Start(Kind::K159, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            10 => {
                                pending.push(Read::Finish(kind, 10, 3));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            11 => {
                                pending.push(Read::Finish(kind, 11, 2));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                            }
                            12 => {
                                pending.push(Read::Finish(kind, 12, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            13 => {
                                pending.push(Read::Finish(kind, 13, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            14 => {
                                pending.push(Read::Finish(kind, 14, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            15 => {
                                pending.push(Read::Finish(kind, 15, 2));
                                pending.push(Read::Start(Kind::K160, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            16 => {
                                pending.push(Read::Finish(kind, 16, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            17 => {
                                pending.push(Read::Finish(kind, 17, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            18 => {
                                pending.push(Read::Finish(kind, 18, 2));
                                pending.push(Read::Start(Kind::K160, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            19 => {
                                pending.push(Read::Finish(kind, 19, 2));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            20 => {
                                pending.push(Read::Finish(kind, 20, 2));
                                pending.push(Read::Start(Kind::K161, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            21 => {
                                pending.push(Read::Finish(kind, 21, 2));
                                pending.push(Read::Start(Kind::K107, depth + 1));
                                pending.push(Read::Start(Kind::K162, depth + 1));
                            }
                            22 => {
                                pending.push(Read::Finish(kind, 22, 3));
                                pending.push(Read::Start(Kind::K147, depth + 1));
                                pending.push(Read::Start(Kind::K147, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            23 => {
                                pending.push(Read::Finish(kind, 23, 2));
                                pending.push(Read::Start(Kind::K163, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            24 => {
                                pending.push(Read::Finish(kind, 24, 3));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K147, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            25 => {
                                pending.push(Read::Finish(kind, 25, 1));
                                pending.push(Read::Start(Kind::K120, depth + 1));
                            }
                            26 => {
                                pending.push(Read::Finish(kind, 26, 1));
                                pending.push(Read::Start(Kind::K119, depth + 1));
                            }
                            27 => {
                                pending.push(Read::Finish(kind, 27, 1));
                                pending.push(Read::Start(Kind::K119, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K114 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K147, depth + 1));
                        pending.push(Read::Start(Kind::K74, depth + 1));
                    }
                    Kind::K115 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K116, depth));
                        }
                    }
                    Kind::K116 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K113, depth + 1));
                        pending.push(Read::Start(Kind::K119, depth + 1));
                    }
                    Kind::K117 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K118, depth));
                        }
                    }
                    Kind::K118 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K120, depth + 1));
                        pending.push(Read::Start(Kind::K119, depth + 1));
                    }
                    Kind::K119 => {
                        let _ = depth;
                        values.push(Value::K119(self.text()?));
                    }
                    Kind::K120 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K113, depth));
                        }
                    }
                    Kind::K121 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K159, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K122 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K133, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K123 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 0));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K85, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K124 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K85, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K125 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K126, depth));
                        }
                    }
                    Kind::K126 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K132, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K76, depth + 1));
                                pending.push(Read::Start(Kind::K132, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K136, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                                pending.push(Read::Start(Kind::K85, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K127 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K128, depth));
                        }
                    }
                    Kind::K128 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K136, depth + 1));
                        pending.push(Read::Start(Kind::K4, depth + 1));
                    }
                    Kind::K129 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K130, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K130 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K125, depth + 1));
                        pending.push(Read::Start(Kind::K134, depth + 1));
                    }
                    Kind::K131 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K132, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K132 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 3));
                                pending.push(Read::Start(Kind::K138, depth + 1));
                                pending.push(Read::Start(Kind::K2, depth + 1));
                                pending.push(Read::Start(Kind::K1, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K137, depth + 1));
                                pending.push(Read::Start(Kind::K85, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K133 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K55, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 4));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K55, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K83, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K134 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 4));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K55, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K135, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K135 => {
                        let _ = depth;
                        values.push(Value::K135(Box::new(ClosureId(self.u32()?))));
                    }
                    Kind::K136 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 3));
                                pending.push(Read::Start(Kind::K52, depth + 1));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                                pending.push(Read::Start(Kind::K140, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                                pending.push(Read::Start(Kind::K140, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K137 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 5));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K55, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K138 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K136, depth));
                        }
                    }
                    Kind::K139 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K140 => {
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
                                pending.push(Read::Finish(kind, 2, 0));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 1));
                                pending.push(Read::Start(Kind::K143, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K144, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K141 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K142, depth));
                        }
                    }
                    Kind::K142 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K175, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 0));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 0));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 0));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K143 => {
                        let _ = depth;
                        values.push(Value::K143(Box::new(ValueId(self.u32()?))));
                    }
                    Kind::K144 => {
                        let _ = depth;
                        values.push(Value::K144(Box::new(LocalId(self.u32()?))));
                    }
                    Kind::K145 => {
                        let _ = depth;
                        values.push(Value::K145(Box::new(SemanticSiteId(self.u32()?))));
                    }
                    Kind::K146 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K147, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K147, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K147 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K121, depth));
                        }
                    }
                    Kind::K148 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 3));
                                pending.push(Read::Start(Kind::K150, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 3));
                                pending.push(Read::Start(Kind::K152, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K151, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K149 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K132, depth + 1));
                                pending.push(Read::Start(Kind::K140, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K125, depth + 1));
                                pending.push(Read::Start(Kind::K133, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K140, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K132, depth + 1));
                                pending.push(Read::Start(Kind::K140, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 2));
                                pending.push(Read::Start(Kind::K132, depth + 1));
                                pending.push(Read::Start(Kind::K153, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 3));
                                pending.push(Read::Start(Kind::K125, depth + 1));
                                pending.push(Read::Start(Kind::K134, depth + 1));
                                pending.push(Read::Start(Kind::K140, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K150 => {
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
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K151 => {
                        let _ = depth;
                        values.push(Value::K151(Box::new(BlockId(self.u32()?))));
                    }
                    Kind::K152 => {
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
                    Kind::K153 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K144, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K154 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K155 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K139, depth + 1));
                    }
                    Kind::K156 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 3));
                                pending.push(Read::Start(Kind::K52, depth + 1));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                                pending.push(Read::Start(Kind::K154, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                                pending.push(Read::Start(Kind::K154, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K157 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K158 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K164, depth + 1));
                        pending.push(Read::Start(Kind::K2, depth + 1));
                        pending.push(Read::Start(Kind::K1, depth + 1));
                    }
                    Kind::K159 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K76, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K119, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K119, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K139, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 3));
                                pending.push(Read::Start(Kind::K159, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K178, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 4));
                                pending.push(Read::Start(Kind::K159, depth + 1));
                                pending.push(Read::Start(Kind::K159, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K179, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 3));
                                pending.push(Read::Start(Kind::K159, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K114, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K160 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K172, depth + 1));
                        pending.push(Read::Start(Kind::K170, depth + 1));
                        pending.push(Read::Start(Kind::K176, depth + 1));
                        pending.push(Read::Start(Kind::K165, depth + 1));
                    }
                    Kind::K161 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K172, depth + 1));
                        pending.push(Read::Start(Kind::K170, depth + 1));
                        pending.push(Read::Start(Kind::K168, depth + 1));
                        pending.push(Read::Start(Kind::K165, depth + 1));
                    }
                    Kind::K162 => {
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
                    Kind::K163 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K183, depth + 1));
                        pending.push(Read::Start(Kind::K182, depth + 1));
                        pending.push(Read::Start(Kind::K114, depth + 1));
                    }
                    Kind::K164 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K156, depth));
                        }
                    }
                    Kind::K165 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 1));
                        pending.push(Read::Start(Kind::K166, depth + 1));
                    }
                    Kind::K166 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K167, depth));
                        }
                    }
                    Kind::K167 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K168 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K169, depth));
                        }
                    }
                    Kind::K169 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K156, depth + 1));
                        pending.push(Read::Start(Kind::K52, depth + 1));
                        pending.push(Read::Start(Kind::K174, depth + 1));
                    }
                    Kind::K170 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K171, depth));
                        }
                    }
                    Kind::K171 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 3));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K165, depth + 1));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K172 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K173, depth));
                        }
                    }
                    Kind::K173 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 4));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 3));
                                pending.push(Read::Start(Kind::K165, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K174 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K175, depth));
                        }
                    }
                    Kind::K175 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K190, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K176 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K177, depth));
                        }
                    }
                    Kind::K177 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 3));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                                pending.push(Read::Start(Kind::K52, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K165, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K178 => {
                        let _ = depth;
                        values.push(Value::K178(Box::new(self.unary()?)));
                    }
                    Kind::K179 => {
                        let _ = depth;
                        values.push(Value::K179(Box::new(self.binary()?)));
                    }
                    Kind::K180 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K181, depth));
                        }
                    }
                    Kind::K181 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K23, depth + 1));
                        pending.push(Read::Start(Kind::K113, depth + 1));
                    }
                    Kind::K182 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 0));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K183 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K184, depth));
                        }
                    }
                    Kind::K184 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K185, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K114, depth + 1));
                    }
                    Kind::K185 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K159, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K186 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K187, depth));
                        }
                    }
                    Kind::K187 => {
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
                    Kind::K188 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K14, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K189 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K16, depth));
                        }
                    }
                    Kind::K190 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
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
                            14 => DynTypePattern::Semantic {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
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
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K187(value) => *value,
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
                                    Value::K76(value) => *value,
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
                                    Value::K178(value) => *value,
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
                                    Value::K179(value) => *value,
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
                                    Value::K119(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            8 => DynConstPattern::FloatLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K119(value) => value,
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
                            Value::K30(Box::new(DynStorageSourceBindingRecipe {
                                position: match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                sources: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K51(value) => *value,
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
                        Kind::K44 => Value::K44(Box::new(match tag {
                            0 => DynResultSourceRecipe::Value,
                            1 => DynResultSourceRecipe::Create,
                            2 => DynResultSourceRecipe::Give,
                            3 => DynResultSourceRecipe::Borrow {
                                sources: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K51(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                mutable: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
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
                        Kind::K51 => Value::K51(Box::new(match tag {
                            0 => DynStorageSourceRecipe::Exact(
                                match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => DynStorageSourceRecipe::PlaceAddress {
                                place: match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => DynStorageSourceRecipe::OwnedView {
                                head: match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                leaf: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => DynStorageSourceRecipe::ReborrowedLeaf {
                                source: match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                head: match fields.next().expect("recipe field") {
                                    Value::K28(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => DynStorageSourceRecipe::Static {
                                leaf: match fields.next().expect("recipe field") {
                                    Value::K16(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K52 => Value::K52(Box::new(match tag {
                            0 => StoredBorrowMode::SharedAddress,
                            1 => StoredBorrowMode::ExclusiveAddress,
                            2 => StoredBorrowMode::ReadOnlyDescription,
                            3 => StoredBorrowMode::WritableDescription,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K53 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K54 => {
                            let _ = tag;
                            Value::K54(Box::new(DynDefinitionId {
                                module: match fields.next().expect("recipe field") {
                                    Value::K55(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K57 => Value::K57(Box::new(match tag {
                            0 => DynDefinitionScope::TraitDefault,
                            1 => DynDefinitionScope::ImplOverride,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K58 => {
                            let _ = tag;
                            Value::K58(Box::new(DynDefinitionDeclaration {
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
                                            Value::K61(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                id: match fields.next().expect("recipe field") {
                                    Value::K54(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                identity: match fields.next().expect("recipe field") {
                                    Value::K62(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                scope: match fields.next().expect("recipe field") {
                                    Value::K57(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                signature: match fields.next().expect("recipe field") {
                                    Value::K20(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K59 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K60 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K61 => {
                            let _ = tag;
                            Value::K61(Box::new(DynDefinitionSpecialization {
                                function: match fields.next().expect("recipe field") {
                                    Value::K82(value) => *value,
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
                                            Value::K68(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                method_actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K68(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K63 => {
                            let _ = tag;
                            Value::K63(Box::new(DynMethodRecipe {
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
                                        Value::K54(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            }))
                        }
                        Kind::K64 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K65 => {
                            let _ = tag;
                            Value::K65(Box::new(DynFunctionInstance {
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
                                    Value::K54(value) => *value,
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
                                            Value::K68(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                method_actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K68(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K66 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K67 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K69 => Value::K69(Box::new(match tag {
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
                        Kind::K70 => {
                            let _ = tag;
                            Value::K70(Box::new(DynAssociatedDefinition {
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
                                        Value::K69(value) => *value,
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
                        Kind::K71 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K72 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K73 => Value::K73(Box::new(match tag {
                            0 => DynAssociatedSource::TraitDefault {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => DynAssociatedSource::ImplOverride {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K75 => Value::K75(Box::new(match tag {
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
                                    Value::K76(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K77 => {
                            let _ = tag;
                            Value::K77(Box::new(DynAssociatedResolution {
                                member: match fields.next().expect("recipe field") {
                                    Value::K78(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K73(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K75(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K79 => {
                            let _ = tag;
                            Value::K79(Box::new(DynImplementationWitness {
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
                                    Value::K80(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                implementation: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K68(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                associated: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K77(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K81 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K82 => Value::K82(Box::new(match tag {
                            0 => DynFunctionTarget::Core {
                                module: match fields.next().expect("recipe field") {
                                    Value::K55(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => DynFunctionTarget::Typed(
                                match fields.next().expect("recipe field") {
                                    Value::K83(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K84 => {
                            let _ = tag;
                            Value::K84(Box::new(SemanticDefinitionId {
                                module: match fields.next().expect("recipe field") {
                                    Value::K55(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                index: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K85 => {
                            let _ = tag;
                            Value::K85(Box::new(SemanticInstanceId {
                                module: match fields.next().expect("recipe field") {
                                    Value::K55(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                index: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K86 => {
                            let _ = tag;
                            Value::K86(Box::new(SemanticModule {
                                definitions: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K88(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                instances: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K90(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                sites: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K92(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                dyn_scopes: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K94(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K87 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K88 => {
                            let _ = tag;
                            Value::K88(Box::new(SemanticDefinition {
                                id: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                identity: match fields.next().expect("recipe field") {
                                    Value::K62(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                parent: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K84(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                kind: match fields.next().expect("recipe field") {
                                    Value::K97(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                parameters: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K99(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                implicit_lifetimes: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K101(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                requirements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K103(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                environment: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K105(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K106(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K89 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K90 => {
                            let _ = tag;
                            Value::K90(Box::new(SemanticInstance {
                                id: match fields.next().expect("recipe field") {
                                    Value::K85(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                definition: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                target: match fields.next().expect("recipe field") {
                                    Value::K122(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                lifetime_instantiation: match fields.next().expect("recipe field") {
                                    Value::K123(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                parent: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K85(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K126(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                header_embedding: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K128(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                environment: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K130(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                self_type: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K132(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            }))
                        }
                        Kind::K91 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K92 => {
                            let _ = tag;
                            Value::K92(Box::new(SemanticSite {
                                instance: match fields.next().expect("recipe field") {
                                    Value::K85(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                recipe: match fields.next().expect("recipe field") {
                                    Value::K145(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                at: match fields.next().expect("recipe field") {
                                    Value::K148(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                action: match fields.next().expect("recipe field") {
                                    Value::K149(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K93 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K94 => {
                            let _ = tag;
                            Value::K94(Box::new(SemanticDynScopeBinding {
                                owner: match fields.next().expect("recipe field") {
                                    Value::K95(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                scope: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K95 => Value::K95(Box::new(match tag {
                            0 => SemanticDynOwner::Declaration(
                                match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticDynOwner::MethodSignature {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticDynOwner::MethodDefinition(
                                match fields.next().expect("recipe field") {
                                    Value::K54(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K96 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K97 => Value::K97(Box::new(match tag {
                            0 => SemanticDefinitionKind::Function,
                            1 => SemanticDefinitionKind::Method,
                            2 => SemanticDefinitionKind::Closure,
                            3 => SemanticDefinitionKind::TaskEntry,
                            4 => SemanticDefinitionKind::Nominal,
                            5 => SemanticDefinitionKind::Scope,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K98 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K99 => Value::K99(Box::new(match tag {
                            0 => SemanticFormal::Type {
                                requirements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K103(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            1 => SemanticFormal::Const {
                                carrier: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticFormal::Lifetime,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K100 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K101 => {
                            let _ = tag;
                            Value::K101(Box::new(SemanticImplicitLifetime {
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                occurrence_path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K6(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K102 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K103 => Value::K103(Box::new(match tag {
                            0 => SemanticRequirementRecipe::Structural {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                bound: match fields.next().expect("recipe field") {
                                    Value::K17(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticRequirementRecipe::Trait {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                application: match fields.next().expect("recipe field") {
                                    Value::K114(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticRequirementRecipe::Callable {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                kind: match fields.next().expect("recipe field") {
                                    Value::K19(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                signature: Box::new(match fields.next().expect("recipe field") {
                                    Value::K107(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            3 => SemanticRequirementRecipe::TypeEqual {
                                left: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                right: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticRequirementRecipe::ConstEqual {
                                carrier: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                left: match fields.next().expect("recipe field") {
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                right: match fields.next().expect("recipe field") {
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticRequirementRecipe::LifeEqual {
                                left: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                right: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            6 => SemanticRequirementRecipe::Outlives {
                                longer: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                shorter: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            7 => SemanticRequirementRecipe::ForAll {
                                lifetimes: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                requirements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K103(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K104 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K105 => {
                            let _ = tag;
                            Value::K105(Box::new(SemanticEnvironmentFormal {
                                kind: match fields.next().expect("recipe field") {
                                    Value::K99(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K106 => Value::K106(Box::new(match tag {
                            0 => SemanticDefinitionBody::Function {
                                signature: match fields.next().expect("recipe field") {
                                    Value::K107(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                sites: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K109(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            1 => SemanticDefinitionBody::Nominal {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                kind: match fields.next().expect("recipe field") {
                                    Value::K110(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K111(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticDefinitionBody::Scope(
                                match fields.next().expect("recipe field") {
                                    Value::K112(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K107 => {
                            let _ = tag;
                            Value::K107(Box::new(SemanticSignatureRecipe {
                                bound_lifetimes: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                invocation_lifetimes: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                parameters: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K181(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                result: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
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
                            }))
                        }
                        Kind::K108 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K109 => {
                            let _ = tag;
                            Value::K109(Box::new(SemanticSiteRecipe {
                                id: match fields.next().expect("recipe field") {
                                    Value::K145(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                occurrence_path: match fields.next().expect("recipe field") {
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
                                    Value::K146(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K110 => Value::K110(Box::new(match tag {
                            0 => LifeNominalKind::Record,
                            1 => LifeNominalKind::Package,
                            2 => LifeNominalKind::Resource,
                            3 => LifeNominalKind::Enum,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K111 => Value::K111(Box::new(match tag {
                            0 => SemanticNominalBody::Product {
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K116(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            1 => SemanticNominalBody::Enum {
                                variants: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K118(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K112 => Value::K112(Box::new(match tag {
                            0 => SemanticScopeRecipe::Trait {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticScopeRecipe::InherentImpl {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                self_type: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticScopeRecipe::TraitImpl {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                self_type: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                trait_application: match fields.next().expect("recipe field") {
                                    Value::K114(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K113 => Value::K113(Box::new(match tag {
                            0 => SemanticTypeRecipe::Atom(
                                match fields.next().expect("recipe field") {
                                    Value::K1(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticTypeRecipe::Exact(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K158(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            2 => SemanticTypeRecipe::Formal(
                                match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => SemanticTypeRecipe::Environment {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticTypeRecipe::SelfType {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticTypeRecipe::Nominal {
                                definition: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K121(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            6 => SemanticTypeRecipe::AssociatedType {
                                receiver: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                application: Box::new(match fields.next().expect("recipe field") {
                                    Value::K114(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            7 => SemanticTypeRecipe::Tuple(
                                match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K113(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            ),
                            8 => SemanticTypeRecipe::Array {
                                element: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                length: Box::new(match fields.next().expect("recipe field") {
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            9 => SemanticTypeRecipe::Slice(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            10 => SemanticTypeRecipe::Address {
                                mutable: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                lifetime: match fields.next().expect("recipe field") {
                                    Value::K157(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                inner: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            11 => SemanticTypeRecipe::Desc {
                                lifetime: match fields.next().expect("recipe field") {
                                    Value::K157(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                inner: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            12 => SemanticTypeRecipe::Managed(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            13 => SemanticTypeRecipe::OwnedStore(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            14 => SemanticTypeRecipe::Mutex(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            15 => SemanticTypeRecipe::MutexGuard {
                                payload: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                retained: Box::new(match fields.next().expect("recipe field") {
                                    Value::K160(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            16 => SemanticTypeRecipe::Registration(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            17 => SemanticTypeRecipe::AsyncRegistration(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            18 => SemanticTypeRecipe::Task {
                                result: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                retained: Box::new(match fields.next().expect("recipe field") {
                                    Value::K160(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            19 => SemanticTypeRecipe::ErrorUnion {
                                error: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                result: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            20 => SemanticTypeRecipe::Qualified {
                                storage: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                life: Box::new(match fields.next().expect("recipe field") {
                                    Value::K161(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            21 => SemanticTypeRecipe::Code {
                                kind: match fields.next().expect("recipe field") {
                                    Value::K162(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                signature: Box::new(match fields.next().expect("recipe field") {
                                    Value::K107(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            22 => SemanticTypeRecipe::Closure {
                                definition: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K121(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                environment: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K121(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            23 => SemanticTypeRecipe::Dyn {
                                owned: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                interface: Box::new(match fields.next().expect("recipe field") {
                                    Value::K163(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            24 => SemanticTypeRecipe::Opaque {
                                definition: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K121(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                hidden: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            25 => SemanticTypeRecipe::AnonymousSum(
                                match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K113(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            ),
                            26 => SemanticTypeRecipe::UnresolvedNamed(
                                match fields.next().expect("recipe field") {
                                    Value::K119(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            27 => SemanticTypeRecipe::Unsupported(
                                match fields.next().expect("recipe field") {
                                    Value::K119(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K114 => {
                            let _ = tag;
                            Value::K114(Box::new(SemanticTraitRecipe {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K121(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K115 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K116 => {
                            let _ = tag;
                            Value::K116(Box::new(SemanticFieldRecipe {
                                name: match fields.next().expect("recipe field") {
                                    Value::K119(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K117 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K118 => {
                            let _ = tag;
                            Value::K118(Box::new(SemanticVariantRecipe {
                                name: match fields.next().expect("recipe field") {
                                    Value::K119(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K113(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K120 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K121 => Value::K121(Box::new(match tag {
                            0 => SemanticActualRecipe::Type(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticActualRecipe::Const {
                                carrier: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticActualRecipe::Life(
                                match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K122 => Value::K122(Box::new(match tag {
                            0 => SemanticInstanceTarget::Function(
                                match fields.next().expect("recipe field") {
                                    Value::K133(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticInstanceTarget::Scope,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K123 => Value::K123(Box::new(match tag {
                            0 => SemanticLifetimeInstantiation::Rigid,
                            1 => SemanticLifetimeInstantiation::Partition {
                                rigid: match fields.next().expect("recipe field") {
                                    Value::K85(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K124 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K125 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K126 => Value::K126(Box::new(match tag {
                            0 => SemanticActual::Type(match fields.next().expect("recipe field") {
                                Value::K132(value) => *value,
                                _ => unreachable!("recipe field grammar"),
                            }),
                            1 => SemanticActual::Const {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K132(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K76(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticActual::Life(match fields.next().expect("recipe field") {
                                Value::K136(value) => *value,
                                _ => unreachable!("recipe field grammar"),
                            }),
                            3 => SemanticActual::CoreRecipe {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K85(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                recipe: Box::new(match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K127 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K128 => {
                            let _ = tag;
                            Value::K128(Box::new(SemanticHeaderBinding {
                                header: match fields.next().expect("recipe field") {
                                    Value::K4(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actual: match fields.next().expect("recipe field") {
                                    Value::K136(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K129 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K130 => {
                            let _ = tag;
                            Value::K130(Box::new(SemanticEnvironmentInstance {
                                closure: match fields.next().expect("recipe field") {
                                    Value::K134(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K126(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K131 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K132 => Value::K132(Box::new(match tag {
                            0 => SemanticTypeValue::Resolved {
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
                                            Value::K136(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            1 => SemanticTypeValue::Core {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K85(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K137(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K133 => Value::K133(Box::new(match tag {
                            0 => SemanticFunctionTarget::Core {
                                module: match fields.next().expect("recipe field") {
                                    Value::K55(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                function: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticFunctionTarget::CoreClosure {
                                module: match fields.next().expect("recipe field") {
                                    Value::K55(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                creator: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                expression: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                occurrence: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticFunctionTarget::Typed(
                                match fields.next().expect("recipe field") {
                                    Value::K83(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K134 => Value::K134(Box::new(match tag {
                            0 => SemanticClosureTarget::Core {
                                module: match fields.next().expect("recipe field") {
                                    Value::K55(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                creator: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                expression: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                occurrence: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticClosureTarget::Typed(
                                match fields.next().expect("recipe field") {
                                    Value::K135(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K136 => Value::K136(Box::new(match tag {
                            0 => SemanticClassExpr::Formal {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticClassExpr::Implicit {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticClassExpr::Bound {
                                depth: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                index: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => SemanticClassExpr::OuterOccurrence {
                                operand: match fields.next().expect("recipe field") {
                                    Value::K140(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticClassExpr::IntrinsicClass {
                                operand: match fields.next().expect("recipe field") {
                                    Value::K140(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                class: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticClassExpr::Environment {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K137 => Value::K137(Box::new(match tag {
                            0 => SemanticCoreTypeSource::OriginalRecipe(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            1 => SemanticCoreTypeSource::Expression {
                                module: match fields.next().expect("recipe field") {
                                    Value::K55(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                function: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                expression: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                occurrence: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                recipe: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K138 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K139 => Value::K139(Box::new(match tag {
                            0 => SemanticScopeRef::Ancestor(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K140 => Value::K140(Box::new(match tag {
                            0 => SemanticOperand::Parameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticOperand::Capture(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => SemanticOperand::Receiver,
                            3 => {
                                SemanticOperand::Value(match fields.next().expect("recipe field") {
                                    Value::K143(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            4 => {
                                SemanticOperand::Local(match fields.next().expect("recipe field") {
                                    Value::K144(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            5 => SemanticOperand::TypeArgument {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            6 => SemanticOperand::Environment(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            7 => SemanticOperand::CoreExpression {
                                expression: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                occurrence: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K141 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K142 => Value::K142(Box::new(match tag {
                            0 => LifeTypeProjection::Place(
                                match fields.next().expect("recipe field") {
                                    Value::K175(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => LifeTypeProjection::TypeArgument(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => LifeTypeProjection::Pointee,
                            3 => LifeTypeProjection::Element,
                            4 => LifeTypeProjection::Parameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            5 => LifeTypeProjection::Result,
                            6 => LifeTypeProjection::Port(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            7 => LifeTypeProjection::AssociatedType(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K146 => Value::K146(Box::new(match tag {
                            0 => SemanticSiteRecipeKind::Construct {
                                result: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticSiteRecipeKind::Call {
                                target: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K121(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            2 => SemanticSiteRecipeKind::CallableCall {
                                callee: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => SemanticSiteRecipeKind::TypeUse {
                                expected: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticSiteRecipeKind::DeclareLocal {
                                expected: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticSiteRecipeKind::Closure {
                                definition: match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                environment: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K121(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K147 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K148 => Value::K148(Box::new(match tag {
                            0 => SemanticSiteAddress::Core {
                                expression: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                occurrence: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                role: match fields.next().expect("recipe field") {
                                    Value::K150(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticSiteAddress::Typed {
                                block: match fields.next().expect("recipe field") {
                                    Value::K151(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                instruction: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                edge: match fields.next().expect("recipe field") {
                                    Value::K152(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K149 => Value::K149(Box::new(match tag {
                            0 => SemanticSiteAction::Construct {
                                result: match fields.next().expect("recipe field") {
                                    Value::K140(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K132(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticSiteAction::Call {
                                target: match fields.next().expect("recipe field") {
                                    Value::K133(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K126(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            2 => SemanticSiteAction::CallableCall {
                                callee: match fields.next().expect("recipe field") {
                                    Value::K140(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => SemanticSiteAction::TypeUse {
                                subject: match fields.next().expect("recipe field") {
                                    Value::K140(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                expected: match fields.next().expect("recipe field") {
                                    Value::K132(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticSiteAction::DeclareLocal {
                                local: match fields.next().expect("recipe field") {
                                    Value::K153(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                expected: match fields.next().expect("recipe field") {
                                    Value::K132(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticSiteAction::Closure {
                                result: match fields.next().expect("recipe field") {
                                    Value::K140(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                closure: match fields.next().expect("recipe field") {
                                    Value::K134(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                environment: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K126(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K150 => Value::K150(Box::new(match tag {
                            0 => SemanticEmissionRole::Construct,
                            1 => SemanticEmissionRole::Call,
                            2 => SemanticEmissionRole::Ascription,
                            3 => SemanticEmissionRole::Binding,
                            4 => SemanticEmissionRole::Closure,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K152 => Value::K152(Box::new(match tag {
                            0 => SemanticSiteEdge::Before,
                            1 => SemanticSiteEdge::After,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K153 => Value::K153(Box::new(match tag {
                            0 => SemanticLocalTarget::Core {
                                statement: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                occurrence: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticLocalTarget::Typed(
                                match fields.next().expect("recipe field") {
                                    Value::K144(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K154 => Value::K154(Box::new(match tag {
                            0 => SemanticRecipeOperand::TypeFormal(
                                match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticRecipeOperand::Environment {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticRecipeOperand::Parameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => SemanticRecipeOperand::Receiver,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K155 => {
                            let _ = tag;
                            Value::K155(Box::new(SemanticFormalRef {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K156 => Value::K156(Box::new(match tag {
                            0 => SemanticLifeRecipe::Formal(
                                match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticLifeRecipe::Implicit {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticLifeRecipe::Environment {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => SemanticLifeRecipe::Bound {
                                depth: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                index: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticLifeRecipe::Invocation(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            5 => SemanticLifeRecipe::OuterOccurrence {
                                operand: match fields.next().expect("recipe field") {
                                    Value::K154(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            6 => SemanticLifeRecipe::IntrinsicClass {
                                operand: match fields.next().expect("recipe field") {
                                    Value::K154(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                class: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K157 => Value::K157(Box::new(match tag {
                            0 => SemanticOuterLifeRecipe::Class(
                                match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticOuterLifeRecipe::Static,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K158 => {
                            let _ = tag;
                            Value::K158(Box::new(SemanticExactTypeRecipe {
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
                                            Value::K156(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K159 => Value::K159(Box::new(match tag {
                            0 => SemanticConstRecipe::Value(
                                match fields.next().expect("recipe field") {
                                    Value::K76(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticConstRecipe::IntegerLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K119(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => SemanticConstRecipe::FloatLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K119(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => SemanticConstRecipe::Formal(
                                match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            4 => SemanticConstRecipe::Environment {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticConstRecipe::Unary {
                                op: match fields.next().expect("recipe field") {
                                    Value::K178(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                operand_type: Box::new(
                                    match fields.next().expect("recipe field") {
                                        Value::K113(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    },
                                ),
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            6 => SemanticConstRecipe::Binary {
                                op: match fields.next().expect("recipe field") {
                                    Value::K179(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                operand_type: Box::new(
                                    match fields.next().expect("recipe field") {
                                        Value::K113(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    },
                                ),
                                left: Box::new(match fields.next().expect("recipe field") {
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                right: Box::new(match fields.next().expect("recipe field") {
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            7 => SemanticConstRecipe::Convert {
                                source_type: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                target_type: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            8 => SemanticConstRecipe::AssociatedConst {
                                receiver: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                application: Box::new(match fields.next().expect("recipe field") {
                                    Value::K114(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K160 => {
                            let _ = tag;
                            Value::K160(Box::new(SemanticRetainedShapeRecipe {
                                domain: match fields.next().expect("recipe field") {
                                    Value::K165(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ports: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K177(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                retained: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K171(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                nominal: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K173(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K161 => {
                            let _ = tag;
                            Value::K161(Box::new(SemanticStorageShapeRecipe {
                                domain: match fields.next().expect("recipe field") {
                                    Value::K165(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                slots: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K169(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                retained: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K171(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                nominal: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K173(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K162 => Value::K162(Box::new(match tag {
                            0 => SemanticCodeKind::FunctionPointer,
                            1 => SemanticCodeKind::BorrowedFunctionPointer,
                            2 => SemanticCodeKind::ForeignCallback,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K163 => {
                            let _ = tag;
                            Value::K163(Box::new(SemanticDynInterfaceRecipe {
                                application: match fields.next().expect("recipe field") {
                                    Value::K114(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                context: match fields.next().expect("recipe field") {
                                    Value::K182(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                associated: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K184(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K164 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K165 => {
                            let _ = tag;
                            Value::K165(Box::new(SemanticClassSequenceRecipe {
                                parts: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K167(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K166 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K167 => Value::K167(Box::new(match tag {
                            0 => SemanticClassSequencePart::One(
                                match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticClassSequencePart::AllClasses {
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K168 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K169 => {
                            let _ = tag;
                            Value::K169(Box::new(SemanticSlotRecipe {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K175(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                class: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K170 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K171 => Value::K171(Box::new(match tag {
                            0 => SemanticRetainedPart::One {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                child_class: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                parent: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticRetainedPart::All {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                parents: match fields.next().expect("recipe field") {
                                    Value::K165(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K172 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K173 => Value::K173(Box::new(match tag {
                            0 => SemanticNominalPart::One {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                argument: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                class: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticNominalPart::FromType {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                classes: match fields.next().expect("recipe field") {
                                    Value::K165(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K174 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K175 => Value::K175(Box::new(match tag {
                            0 => {
                                PlaceProjection::Field(match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            1 => PlaceProjection::Variant(
                                match fields.next().expect("recipe field") {
                                    Value::K190(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => PlaceProjection::ErrorArm(
                                match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K176 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K177 => Value::K177(Box::new(match tag {
                            0 => SemanticPortPart::One {
                                leaf: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                mode: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                class: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticPortPart::FromType {
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                classes: match fields.next().expect("recipe field") {
                                    Value::K165(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K180 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K181 => {
                            let _ = tag;
                            Value::K181(Box::new(SemanticParameterRecipe {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K23(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K182 => Value::K182(Box::new(match tag {
                            0 => SemanticDynContextRecipe::Closed,
                            1 => SemanticDynContextRecipe::Type(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K183 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K184 => {
                            let _ = tag;
                            Value::K184(Box::new(SemanticAssociatedBindingRecipe {
                                application: match fields.next().expect("recipe field") {
                                    Value::K114(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K185(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K185 => Value::K185(Box::new(match tag {
                            0 => SemanticAssociatedRecipe::Type(
                                match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticAssociatedRecipe::Const {
                                carrier: match fields.next().expect("recipe field") {
                                    Value::K113(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K186 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K187 => Value::K187(Box::new(match tag {
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
                        Kind::K188 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K189 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K190 => {
                            let _ = tag;
                            Value::K190(Box::new(VariantProjection {
                                variant: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                index: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
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
