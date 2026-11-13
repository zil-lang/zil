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
    K113(&'a Vec<SemanticOriginalClosureParameter>, usize),
    K114(&'a SemanticOriginalClosureParameter, usize),
    K115(&'a Option<u32>, usize),
    K116(&'a Vec<SemanticOriginalNameBinding>, usize),
    K117(&'a SemanticOriginalNameBinding, usize),
    K118(&'a Vec<SemanticOriginalSiteNode>, usize),
    K119(&'a SemanticOriginalSiteNode, usize),
    K120(&'a SemanticOriginalFunctionBody, usize),
    K121(&'a SemanticOriginalBlock, usize),
    K122(&'a SemanticOriginalTypeSource, usize),
    K123(&'a SemanticOriginalNode, usize),
    K124(&'a String, usize),
    K125(&'a SemanticOriginalNamespace, usize),
    K126(&'a Vec<String>, usize),
    K127(&'a SemanticOriginalNameValue, usize),
    K128(&'a SemanticActualRecipe, usize),
    K129(&'a SemanticSiteId, usize),
    K130(&'a SemanticTypeRecipe, usize),
    K131(&'a SemanticTraitRecipe, usize),
    K132(&'a SemanticBuiltinTrait, usize),
    K133(&'a Vec<SemanticFieldRecipe>, usize),
    K134(&'a SemanticFieldRecipe, usize),
    K135(&'a Vec<SemanticVariantRecipe>, usize),
    K136(&'a SemanticVariantRecipe, usize),
    K137(&'a Vec<SemanticTypeRecipe>, usize),
    K138(&'a SemanticInstanceTarget, usize),
    K139(&'a SemanticLifetimeInstantiation, usize),
    K140(&'a Option<SemanticInstanceId>, usize),
    K141(&'a Vec<SemanticActual>, usize),
    K142(&'a SemanticActual, usize),
    K143(&'a Vec<SemanticHeaderBinding>, usize),
    K144(&'a SemanticHeaderBinding, usize),
    K145(&'a Option<SemanticEnvironmentInstance>, usize),
    K146(&'a SemanticEnvironmentInstance, usize),
    K147(&'a Option<SemanticTypeValue>, usize),
    K148(&'a SemanticTypeValue, usize),
    K149(&'a SemanticFunctionTarget, usize),
    K150(&'a SemanticClosureTarget, usize),
    K151(&'a ClosureId, usize),
    K152(&'a SemanticClassExpr, usize),
    K153(&'a SemanticCoreTypeSource, usize),
    K154(&'a Vec<SemanticClassExpr>, usize),
    K155(&'a SemanticScopeRef, usize),
    K156(&'a SemanticOperand, usize),
    K157(&'a Vec<LifeTypeProjection>, usize),
    K158(&'a LifeTypeProjection, usize),
    K159(&'a ValueId, usize),
    K160(&'a LocalId, usize),
    K161(&'a SemanticSiteRecipeKind, usize),
    K162(&'a Vec<SemanticActualRecipe>, usize),
    K163(&'a SemanticSiteAddress, usize),
    K164(&'a SemanticSiteAction, usize),
    K165(&'a SemanticEmissionRole, usize),
    K166(&'a BlockId, usize),
    K167(&'a SemanticSiteEdge, usize),
    K168(&'a SemanticLocalTarget, usize),
    K169(&'a SemanticRecipeOperand, usize),
    K170(&'a SemanticFormalRef, usize),
    K171(&'a SemanticLifeRecipe, usize),
    K172(&'a SemanticOuterLifeRecipe, usize),
    K173(&'a SemanticExactTypeRecipe, usize),
    K174(&'a SemanticConstRecipe, usize),
    K175(&'a SemanticRetainedShapeRecipe, usize),
    K176(&'a SemanticStorageShapeRecipe, usize),
    K177(&'a SemanticCodeKind, usize),
    K178(&'a SemanticDynInterfaceRecipe, usize),
    K179(&'a Vec<SemanticLifeRecipe>, usize),
    K180(&'a SemanticClassSequenceRecipe, usize),
    K181(&'a Vec<SemanticClassSequencePart>, usize),
    K182(&'a SemanticClassSequencePart, usize),
    K183(&'a Vec<SemanticSlotRecipe>, usize),
    K184(&'a SemanticSlotRecipe, usize),
    K185(&'a Vec<SemanticRetainedPart>, usize),
    K186(&'a SemanticRetainedPart, usize),
    K187(&'a Vec<SemanticNominalPart>, usize),
    K188(&'a SemanticNominalPart, usize),
    K189(&'a Vec<PlaceProjection>, usize),
    K190(&'a PlaceProjection, usize),
    K191(&'a Vec<SemanticPortPart>, usize),
    K192(&'a SemanticPortPart, usize),
    K193(&'a UnaryOp, usize),
    K194(&'a BinaryOp, usize),
    K195(&'a Vec<SemanticParameterRecipe>, usize),
    K196(&'a SemanticParameterRecipe, usize),
    K197(&'a SemanticDynContextRecipe, usize),
    K198(&'a Vec<SemanticAssociatedBindingRecipe>, usize),
    K199(&'a SemanticAssociatedBindingRecipe, usize),
    K200(&'a SemanticAssociatedRecipe, usize),
    K201(&'a Option<SemanticTypeRecipe>, usize),
    K202(&'a Span, usize),
    K203(&'a SemanticOriginalPattern, usize),
    K204(&'a SemanticOriginalPatternKind, usize),
    K205(&'a Vec<SemanticOriginalPattern>, usize),
    K206(&'a Vec<(String, SemanticOriginalPattern)>, usize),
    K207(&'a (String, SemanticOriginalPattern), usize),
    K208(&'a SemanticOriginalMatchArm, usize),
    K209(&'a Vec<SemanticOriginalStatement>, usize),
    K210(&'a SemanticOriginalStatement, usize),
    K211(&'a SemanticOriginalStatementKind, usize),
    K212(&'a Option<SemanticOriginalExpr>, usize),
    K213(&'a SemanticOriginalExpr, usize),
    K214(&'a Option<SemanticOriginalBlock>, usize),
    K215(&'a Vec<SemanticOriginalMatchArm>, usize),
    K216(&'a SemanticOriginalExprKind, usize),
    K217(&'a DynInterfaceId, usize),
    K218(&'a DynVtableId, usize),
    K219(&'a DynOwnedStorage, usize),
    K220(&'a Vec<SemanticOriginalExpr>, usize),
    K221(&'a Option<Box<SemanticOriginalExpr>>, usize),
    K222(&'a Vec<(String, SemanticOriginalExpr)>, usize),
    K223(&'a (String, SemanticOriginalExpr), usize),
    K224(&'a SemanticOriginalSyntaxCategory, usize),
    K225(&'a SemanticOriginalQuoteTemplate, usize),
    K226(&'a SemanticOriginalSyntaxTree, usize),
    K227(&'a Vec<SemanticOriginalGenericArgument>, usize),
    K228(&'a SemanticOriginalGenericArgument, usize),
    K229(&'a SemanticOriginalQuoteHole, usize),
    K230(&'a SemanticOriginalQuoteHoleKind, usize),
    K231(&'a Vec<SemanticOriginalQuoteHole>, usize),
    K232(&'a Vec<SemanticOriginalQuoteNameSite>, usize),
    K233(&'a SemanticOriginalQuoteNameSite, usize),
    K234(&'a SemanticOriginalSyntaxDelimiter, usize),
    K235(&'a SemanticOriginalQuoteNameNamespace, usize),
    K236(&'a SemanticOriginalQuoteNameRole, usize),
    K237(&'a Vec<SemanticOriginalSyntaxTreeEntry>, usize),
    K238(&'a SemanticOriginalSyntaxTreeEntry, usize),
    K239(&'a Vec<DynPatternActual>, usize),
    K240(&'a DynPatternActual, usize),
    K241(&'a Option<DynLifePattern>, usize),
    K242(&'a Vec<DynTypePattern>, usize),
    K243(&'a VariantProjection, usize),
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
    K191,
    K192,
    K193,
    K194,
    K195,
    K196,
    K197,
    K198,
    K199,
    K200,
    K201,
    K202,
    K203,
    K204,
    K205,
    K206,
    K207,
    K208,
    K209,
    K210,
    K211,
    K212,
    K213,
    K214,
    K215,
    K216,
    K217,
    K218,
    K219,
    K220,
    K221,
    K222,
    K223,
    K224,
    K225,
    K226,
    K227,
    K228,
    K229,
    K230,
    K231,
    K232,
    K233,
    K234,
    K235,
    K236,
    K237,
    K238,
    K239,
    K240,
    K241,
    K242,
    K243,
}
enum Read {
    Start(Kind, usize),
    Finish(Kind, u8, usize),
}
enum Value {
    Rows(Vec<Value>),
    Tuple(Vec<Value>),
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
    K114(Box<SemanticOriginalClosureParameter>),
    K117(Box<SemanticOriginalNameBinding>),
    K119(Box<SemanticOriginalSiteNode>),
    K120(Box<SemanticOriginalFunctionBody>),
    K121(Box<SemanticOriginalBlock>),
    K122(Box<SemanticOriginalTypeSource>),
    K123(Box<SemanticOriginalNode>),
    K124(String),
    K125(Box<SemanticOriginalNamespace>),
    K127(Box<SemanticOriginalNameValue>),
    K128(Box<SemanticActualRecipe>),
    K129(Box<SemanticSiteId>),
    K130(Box<SemanticTypeRecipe>),
    K131(Box<SemanticTraitRecipe>),
    K132(Box<SemanticBuiltinTrait>),
    K134(Box<SemanticFieldRecipe>),
    K136(Box<SemanticVariantRecipe>),
    K138(Box<SemanticInstanceTarget>),
    K139(Box<SemanticLifetimeInstantiation>),
    K142(Box<SemanticActual>),
    K144(Box<SemanticHeaderBinding>),
    K146(Box<SemanticEnvironmentInstance>),
    K148(Box<SemanticTypeValue>),
    K149(Box<SemanticFunctionTarget>),
    K150(Box<SemanticClosureTarget>),
    K151(Box<ClosureId>),
    K152(Box<SemanticClassExpr>),
    K153(Box<SemanticCoreTypeSource>),
    K155(Box<SemanticScopeRef>),
    K156(Box<SemanticOperand>),
    K158(Box<LifeTypeProjection>),
    K159(Box<ValueId>),
    K160(Box<LocalId>),
    K161(Box<SemanticSiteRecipeKind>),
    K163(Box<SemanticSiteAddress>),
    K164(Box<SemanticSiteAction>),
    K165(Box<SemanticEmissionRole>),
    K166(Box<BlockId>),
    K167(Box<SemanticSiteEdge>),
    K168(Box<SemanticLocalTarget>),
    K169(Box<SemanticRecipeOperand>),
    K170(Box<SemanticFormalRef>),
    K171(Box<SemanticLifeRecipe>),
    K172(Box<SemanticOuterLifeRecipe>),
    K173(Box<SemanticExactTypeRecipe>),
    K174(Box<SemanticConstRecipe>),
    K175(Box<SemanticRetainedShapeRecipe>),
    K176(Box<SemanticStorageShapeRecipe>),
    K177(Box<SemanticCodeKind>),
    K178(Box<SemanticDynInterfaceRecipe>),
    K180(Box<SemanticClassSequenceRecipe>),
    K182(Box<SemanticClassSequencePart>),
    K184(Box<SemanticSlotRecipe>),
    K186(Box<SemanticRetainedPart>),
    K188(Box<SemanticNominalPart>),
    K190(Box<PlaceProjection>),
    K192(Box<SemanticPortPart>),
    K193(Box<UnaryOp>),
    K194(Box<BinaryOp>),
    K196(Box<SemanticParameterRecipe>),
    K197(Box<SemanticDynContextRecipe>),
    K199(Box<SemanticAssociatedBindingRecipe>),
    K200(Box<SemanticAssociatedRecipe>),
    K202(Box<Span>),
    K203(Box<SemanticOriginalPattern>),
    K204(Box<SemanticOriginalPatternKind>),
    K208(Box<SemanticOriginalMatchArm>),
    K210(Box<SemanticOriginalStatement>),
    K211(Box<SemanticOriginalStatementKind>),
    K213(Box<SemanticOriginalExpr>),
    K216(Box<SemanticOriginalExprKind>),
    K217(Box<DynInterfaceId>),
    K218(Box<DynVtableId>),
    K219(Box<DynOwnedStorage>),
    K224(Box<SemanticOriginalSyntaxCategory>),
    K225(Box<SemanticOriginalQuoteTemplate>),
    K226(Box<SemanticOriginalSyntaxTree>),
    K228(Box<SemanticOriginalGenericArgument>),
    K229(Box<SemanticOriginalQuoteHole>),
    K230(Box<SemanticOriginalQuoteHoleKind>),
    K233(Box<SemanticOriginalQuoteNameSite>),
    K234(Box<SemanticOriginalSyntaxDelimiter>),
    K235(Box<SemanticOriginalQuoteNameNamespace>),
    K236(Box<SemanticOriginalQuoteNameRole>),
    K238(Box<SemanticOriginalSyntaxTreeEntry>),
    K240(Box<DynPatternActual>),
    K243(Box<VariantProjection>),
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
                            pending.push(Write::K241(field_lifetime, depth + 1));
                            pending.push(Write::K33(field_mutable, depth + 1));
                        }
                        DynTypePattern::Tuple(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K242(field_value_0, depth + 1));
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
                            pending.push(Write::K241(field_lifetime, depth + 1));
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
                            pending.push(Write::K130(field_ty, depth + 1));
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
                    pending.push(Write::K239(&value.arguments, depth + 1));
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
                            pending.push(Write::K193(field_op, depth + 1));
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
                            pending.push(Write::K194(field_op, depth + 1));
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
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        DynConstPattern::FloatLiteral(field_value_0) => {
                            self.u8(8)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
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
                    pending.push(Write::K147(&value.self_type, depth + 1));
                    pending.push(Write::K145(&value.environment, depth + 1));
                    pending.push(Write::K143(&value.header_embedding, depth + 1));
                    pending.push(Write::K141(&value.actuals, depth + 1));
                    pending.push(Write::K140(&value.parent, depth + 1));
                    pending.push(Write::K139(&value.lifetime_instantiation, depth + 1));
                    pending.push(Write::K138(&value.target, depth + 1));
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
                    pending.push(Write::K164(&value.action, depth + 1));
                    pending.push(Write::K163(&value.at, depth + 1));
                    pending.push(Write::K129(&value.recipe, depth + 1));
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
                            pending.push(Write::K130(field_carrier, depth + 1));
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
                            pending.push(Write::K130(field_ty, depth + 1));
                        }
                        SemanticRequirementRecipe::Trait {
                            ty: field_ty,
                            application: field_application,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K131(field_application, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
                        }
                        SemanticRequirementRecipe::Callable {
                            ty: field_ty,
                            kind: field_kind,
                            signature: field_signature,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K107(field_signature, depth + 1));
                            pending.push(Write::K19(field_kind, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
                        }
                        SemanticRequirementRecipe::TypeEqual {
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K130(field_right, depth + 1));
                            pending.push(Write::K130(field_left, depth + 1));
                        }
                        SemanticRequirementRecipe::ConstEqual {
                            carrier: field_carrier,
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K174(field_right, depth + 1));
                            pending.push(Write::K174(field_left, depth + 1));
                            pending.push(Write::K130(field_carrier, depth + 1));
                        }
                        SemanticRequirementRecipe::LifeEqual {
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K171(field_right, depth + 1));
                            pending.push(Write::K171(field_left, depth + 1));
                        }
                        SemanticRequirementRecipe::Outlives {
                            longer: field_longer,
                            shorter: field_shorter,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K171(field_shorter, depth + 1));
                            pending.push(Write::K171(field_longer, depth + 1));
                        }
                        SemanticRequirementRecipe::ForAll {
                            lifetimes: field_lifetimes,
                            requirements: field_requirements,
                        } => {
                            self.u8(7)?;
                            pending.push(Write::K102(field_requirements, depth + 1));
                            pending.push(Write::K6(field_lifetimes, depth + 1));
                        }
                        SemanticRequirementRecipe::Builtin {
                            ty: field_ty,
                            trait_kind: field_trait_kind,
                        } => {
                            self.u8(8)?;
                            pending.push(Write::K132(field_trait_kind, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
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
                    pending.push(Write::K128(&value.source, depth + 1));
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
                        SemanticDefinitionBody::FunctionWithBody {
                            signature: field_signature,
                            sites: field_sites,
                            parameters: field_parameters,
                            receiver_parameter: field_receiver_parameter,
                            names: field_names,
                            site_nodes: field_site_nodes,
                            body: field_body,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K120(field_body, depth + 1));
                            pending.push(Write::K118(field_site_nodes, depth + 1));
                            pending.push(Write::K116(field_names, depth + 1));
                            pending.push(Write::K115(field_receiver_parameter, depth + 1));
                            pending.push(Write::K113(field_parameters, depth + 1));
                            pending.push(Write::K108(field_sites, depth + 1));
                            pending.push(Write::K107(field_signature, depth + 1));
                        }
                    }
                }
                Write::K107(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K25(&value.call_kind, depth + 1));
                    pending.push(Write::K23(&value.result_mode, depth + 1));
                    pending.push(Write::K130(&value.result, depth + 1));
                    pending.push(Write::K195(&value.parameters, depth + 1));
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
                    pending.push(Write::K161(&value.kind, depth + 1));
                    pending.push(Write::K11(&value.occurrence_path, depth + 1));
                    pending.push(Write::K129(&value.id, depth + 1));
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
                            pending.push(Write::K133(field_fields, depth + 1));
                        }
                        SemanticNominalBody::Enum {
                            variants: field_variants,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K135(field_variants, depth + 1));
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
                            pending.push(Write::K130(field_self_type, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                        }
                        SemanticScopeRecipe::TraitImpl {
                            declaration: field_declaration,
                            self_type: field_self_type,
                            trait_application: field_trait_application,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K131(field_trait_application, depth + 1));
                            pending.push(Write::K130(field_self_type, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                        }
                        SemanticScopeRecipe::BuiltinImpl {
                            self_type: field_self_type,
                            trait_kind: field_trait_kind,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K132(field_trait_kind, depth + 1));
                            pending.push(Write::K130(field_self_type, depth + 1));
                        }
                    }
                }
                Write::K113(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K114(item, depth));
                    }
                }
                Write::K114(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K202(&value.span, depth + 1));
                    pending.push(Write::K23(&value.mode, depth + 1));
                    pending.push(Write::K201(&value.ty, depth + 1));
                    pending.push(Write::K124(&value.name, depth + 1));
                }
                Write::K115(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K6(item, depth));
                    }
                }
                Write::K116(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K117(item, depth));
                    }
                }
                Write::K117(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K127(&value.value, depth + 1));
                    pending.push(Write::K125(&value.namespace, depth + 1));
                    pending.push(Write::K126(&value.path, depth + 1));
                }
                Write::K118(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K119(item, depth));
                    }
                }
                Write::K119(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K123(&value.node, depth + 1));
                    pending.push(Write::K129(&value.site, depth + 1));
                }
                Write::K120(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalFunctionBody::Defined(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K121(field_value_0, depth + 1));
                        }
                        SemanticOriginalFunctionBody::External => {
                            self.u8(1)?;
                        }
                        SemanticOriginalFunctionBody::Declaration => {
                            self.u8(2)?;
                        }
                    }
                }
                Write::K121(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K202(&value.span, depth + 1));
                    pending.push(Write::K209(&value.statements, depth + 1));
                }
                Write::K122(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalTypeSource::Expression(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K56(field_value_0, depth + 1));
                        }
                        SemanticOriginalTypeSource::Binding(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K56(field_value_0, depth + 1));
                        }
                        SemanticOriginalTypeSource::Parameter(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        SemanticOriginalTypeSource::Result => {
                            self.u8(3)?;
                        }
                        SemanticOriginalTypeSource::CallConstCarrier {
                            call: field_call,
                            formal: field_formal,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_formal, depth + 1));
                            pending.push(Write::K56(field_call, depth + 1));
                        }
                        SemanticOriginalTypeSource::BindingProjection {
                            statement: field_statement,
                            pattern: field_pattern,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K11(field_pattern, depth + 1));
                            pending.push(Write::K56(field_statement, depth + 1));
                        }
                        SemanticOriginalTypeSource::NameAt {
                            at: field_at,
                            name: field_name,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K124(field_name, depth + 1));
                            pending.push(Write::K123(field_at, depth + 1));
                        }
                    }
                }
                Write::K123(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalNode::Expression(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K56(field_value_0, depth + 1));
                        }
                        SemanticOriginalNode::Statement(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K56(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K124(value, depth) => {
                    let _ = depth;
                    self.text(value)?;
                }
                Write::K125(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalNamespace::Value => {
                            self.u8(0)?;
                        }
                        SemanticOriginalNamespace::Type => {
                            self.u8(1)?;
                        }
                        SemanticOriginalNamespace::Lifetime => {
                            self.u8(2)?;
                        }
                    }
                }
                Write::K126(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K124(item, depth));
                    }
                }
                Write::K127(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalNameValue::Recipe(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K128(field_value_0, depth + 1));
                        }
                        SemanticOriginalNameValue::Definition(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K84(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K128(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticActualRecipe::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticActualRecipe::Const {
                            carrier: field_carrier,
                            value: field_value,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K174(field_value, depth + 1));
                            pending.push(Write::K130(field_carrier, depth + 1));
                        }
                        SemanticActualRecipe::Life(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K171(field_value_0, depth + 1));
                        }
                        SemanticActualRecipe::Infer => {
                            self.u8(3)?;
                        }
                    }
                }
                Write::K129(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K130(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticTypeRecipe::Atom(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K1(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Exact(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K173(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Formal(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K170(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
                        }
                        SemanticTypeRecipe::SelfType { scope: field_scope } => {
                            self.u8(4)?;
                            pending.push(Write::K155(field_scope, depth + 1));
                        }
                        SemanticTypeRecipe::Nominal {
                            definition: field_definition,
                            arguments: field_arguments,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K162(field_arguments, depth + 1));
                            pending.push(Write::K84(field_definition, depth + 1));
                        }
                        SemanticTypeRecipe::AssociatedType {
                            receiver: field_receiver,
                            application: field_application,
                            member: field_member,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K131(field_application, depth + 1));
                            pending.push(Write::K130(field_receiver, depth + 1));
                        }
                        SemanticTypeRecipe::Tuple(field_value_0) => {
                            self.u8(7)?;
                            pending.push(Write::K137(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Array {
                            element: field_element,
                            length: field_length,
                        } => {
                            self.u8(8)?;
                            pending.push(Write::K174(field_length, depth + 1));
                            pending.push(Write::K130(field_element, depth + 1));
                        }
                        SemanticTypeRecipe::Slice(field_value_0) => {
                            self.u8(9)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Address {
                            mutable: field_mutable,
                            lifetime: field_lifetime,
                            inner: field_inner,
                        } => {
                            self.u8(10)?;
                            pending.push(Write::K130(field_inner, depth + 1));
                            pending.push(Write::K172(field_lifetime, depth + 1));
                            pending.push(Write::K33(field_mutable, depth + 1));
                        }
                        SemanticTypeRecipe::Desc {
                            lifetime: field_lifetime,
                            inner: field_inner,
                        } => {
                            self.u8(11)?;
                            pending.push(Write::K130(field_inner, depth + 1));
                            pending.push(Write::K172(field_lifetime, depth + 1));
                        }
                        SemanticTypeRecipe::Managed(field_value_0) => {
                            self.u8(12)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::OwnedStore(field_value_0) => {
                            self.u8(13)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Mutex(field_value_0) => {
                            self.u8(14)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::MutexGuard {
                            payload: field_payload,
                            retained: field_retained,
                        } => {
                            self.u8(15)?;
                            pending.push(Write::K175(field_retained, depth + 1));
                            pending.push(Write::K130(field_payload, depth + 1));
                        }
                        SemanticTypeRecipe::Registration(field_value_0) => {
                            self.u8(16)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::AsyncRegistration(field_value_0) => {
                            self.u8(17)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Task {
                            result: field_result,
                            retained: field_retained,
                        } => {
                            self.u8(18)?;
                            pending.push(Write::K175(field_retained, depth + 1));
                            pending.push(Write::K130(field_result, depth + 1));
                        }
                        SemanticTypeRecipe::ErrorUnion {
                            error: field_error,
                            result: field_result,
                        } => {
                            self.u8(19)?;
                            pending.push(Write::K130(field_result, depth + 1));
                            pending.push(Write::K130(field_error, depth + 1));
                        }
                        SemanticTypeRecipe::Qualified {
                            storage: field_storage,
                            life: field_life,
                        } => {
                            self.u8(20)?;
                            pending.push(Write::K176(field_life, depth + 1));
                            pending.push(Write::K130(field_storage, depth + 1));
                        }
                        SemanticTypeRecipe::Code {
                            kind: field_kind,
                            signature: field_signature,
                        } => {
                            self.u8(21)?;
                            pending.push(Write::K107(field_signature, depth + 1));
                            pending.push(Write::K177(field_kind, depth + 1));
                        }
                        SemanticTypeRecipe::Closure {
                            definition: field_definition,
                            arguments: field_arguments,
                            environment: field_environment,
                        } => {
                            self.u8(22)?;
                            pending.push(Write::K162(field_environment, depth + 1));
                            pending.push(Write::K162(field_arguments, depth + 1));
                            pending.push(Write::K84(field_definition, depth + 1));
                        }
                        SemanticTypeRecipe::Dyn {
                            owned: field_owned,
                            interface: field_interface,
                        } => {
                            self.u8(23)?;
                            pending.push(Write::K178(field_interface, depth + 1));
                            pending.push(Write::K33(field_owned, depth + 1));
                        }
                        SemanticTypeRecipe::Opaque {
                            definition: field_definition,
                            arguments: field_arguments,
                            hidden: field_hidden,
                        } => {
                            self.u8(24)?;
                            pending.push(Write::K130(field_hidden, depth + 1));
                            pending.push(Write::K162(field_arguments, depth + 1));
                            pending.push(Write::K84(field_definition, depth + 1));
                        }
                        SemanticTypeRecipe::AnonymousSum(field_value_0) => {
                            self.u8(25)?;
                            pending.push(Write::K137(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::UnresolvedNamed(field_value_0) => {
                            self.u8(26)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::Unsupported(field_value_0) => {
                            self.u8(27)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticTypeRecipe::OriginalType {
                            scope: field_scope,
                            source: field_source,
                        } => {
                            self.u8(28)?;
                            pending.push(Write::K122(field_source, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
                        }
                    }
                }
                Write::K131(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K162(&value.arguments, depth + 1));
                    pending.push(Write::K74(&value.declaration, depth + 1));
                }
                Write::K132(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticBuiltinTrait::Drop => {
                            self.u8(0)?;
                        }
                        SemanticBuiltinTrait::Copy => {
                            self.u8(1)?;
                        }
                        SemanticBuiltinTrait::Value => {
                            self.u8(2)?;
                        }
                        SemanticBuiltinTrait::SyncDrop => {
                            self.u8(3)?;
                        }
                        SemanticBuiltinTrait::StoreElement => {
                            self.u8(4)?;
                        }
                        SemanticBuiltinTrait::Transfer => {
                            self.u8(5)?;
                        }
                        SemanticBuiltinTrait::Share => {
                            self.u8(6)?;
                        }
                        SemanticBuiltinTrait::ThreadCallable => {
                            self.u8(7)?;
                        }
                        SemanticBuiltinTrait::ClosedEnvironment => {
                            self.u8(8)?;
                        }
                        SemanticBuiltinTrait::Send => {
                            self.u8(9)?;
                        }
                        SemanticBuiltinTrait::Sync => {
                            self.u8(10)?;
                        }
                    }
                }
                Write::K133(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K134(item, depth));
                    }
                }
                Write::K134(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K130(&value.ty, depth + 1));
                    pending.push(Write::K124(&value.name, depth + 1));
                }
                Write::K135(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K136(item, depth));
                    }
                }
                Write::K136(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K137(&value.fields, depth + 1));
                    pending.push(Write::K124(&value.name, depth + 1));
                }
                Write::K137(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K130(item, depth));
                    }
                }
                Write::K138(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticInstanceTarget::Function(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K149(field_value_0, depth + 1));
                        }
                        SemanticInstanceTarget::Scope => {
                            self.u8(1)?;
                        }
                    }
                }
                Write::K139(value, depth) => {
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
                Write::K140(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K85(item, depth));
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
                        SemanticActual::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K148(field_value_0, depth + 1));
                        }
                        SemanticActual::Const {
                            ty: field_ty,
                            value: field_value,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K76(field_value, depth + 1));
                            pending.push(Write::K148(field_ty, depth + 1));
                        }
                        SemanticActual::Life(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K152(field_value_0, depth + 1));
                        }
                        SemanticActual::CoreRecipe {
                            scope: field_scope,
                            recipe: field_recipe,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K128(field_recipe, depth + 1));
                            pending.push(Write::K85(field_scope, depth + 1));
                        }
                        SemanticActual::CoreInferred {
                            scope: field_scope,
                            call: field_call,
                            formal: field_formal,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_formal, depth + 1));
                            pending.push(Write::K129(field_call, depth + 1));
                            pending.push(Write::K85(field_scope, depth + 1));
                        }
                    }
                }
                Write::K143(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K144(item, depth));
                    }
                }
                Write::K144(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K152(&value.actual, depth + 1));
                    pending.push(Write::K4(&value.header, depth + 1));
                }
                Write::K145(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K146(item, depth));
                    }
                }
                Write::K146(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K141(&value.actuals, depth + 1));
                    pending.push(Write::K150(&value.closure, depth + 1));
                }
                Write::K147(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K148(item, depth));
                    }
                }
                Write::K148(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticTypeValue::Resolved {
                            ty: field_ty,
                            life: field_life,
                            classes: field_classes,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K154(field_classes, depth + 1));
                            pending.push(Write::K2(field_life, depth + 1));
                            pending.push(Write::K1(field_ty, depth + 1));
                        }
                        SemanticTypeValue::Core {
                            scope: field_scope,
                            source: field_source,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K153(field_source, depth + 1));
                            pending.push(Write::K85(field_scope, depth + 1));
                        }
                    }
                }
                Write::K149(value, depth) => {
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
                Write::K150(value, depth) => {
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
                            pending.push(Write::K151(field_value_0, depth + 1));
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
                        SemanticClassExpr::Formal {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
                        }
                        SemanticClassExpr::Implicit {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
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
                            pending.push(Write::K157(field_path, depth + 1));
                            pending.push(Write::K156(field_operand, depth + 1));
                        }
                        SemanticClassExpr::IntrinsicClass {
                            operand: field_operand,
                            path: field_path,
                            class: field_class,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_class, depth + 1));
                            pending.push(Write::K157(field_path, depth + 1));
                            pending.push(Write::K156(field_operand, depth + 1));
                        }
                        SemanticClassExpr::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
                        }
                    }
                }
                Write::K153(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticCoreTypeSource::OriginalRecipe(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticCoreTypeSource::Expression {
                            module: field_module,
                            function: field_function,
                            expression: field_expression,
                            occurrence: field_occurrence,
                            recipe: field_recipe,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K130(field_recipe, depth + 1));
                            pending.push(Write::K6(field_occurrence, depth + 1));
                            pending.push(Write::K56(field_expression, depth + 1));
                            pending.push(Write::K56(field_function, depth + 1));
                            pending.push(Write::K55(field_module, depth + 1));
                        }
                        SemanticCoreTypeSource::OriginalExpression {
                            module: field_module,
                            function: field_function,
                            expression: field_expression,
                            occurrence: field_occurrence,
                            source: field_source,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K122(field_source, depth + 1));
                            pending.push(Write::K6(field_occurrence, depth + 1));
                            pending.push(Write::K56(field_expression, depth + 1));
                            pending.push(Write::K56(field_function, depth + 1));
                            pending.push(Write::K55(field_module, depth + 1));
                        }
                    }
                }
                Write::K154(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K152(item, depth));
                    }
                }
                Write::K155(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticScopeRef::Ancestor(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K156(value, depth) => {
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
                            pending.push(Write::K159(field_value_0, depth + 1));
                        }
                        SemanticOperand::Local(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K160(field_value_0, depth + 1));
                        }
                        SemanticOperand::TypeArgument {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
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
                Write::K157(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K158(item, depth));
                    }
                }
                Write::K158(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        LifeTypeProjection::Place(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K190(field_value_0, depth + 1));
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
                Write::K159(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K160(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K161(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticSiteRecipeKind::Construct {
                            result: field_result,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K130(field_result, depth + 1));
                        }
                        SemanticSiteRecipeKind::Call {
                            target: field_target,
                            actuals: field_actuals,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K162(field_actuals, depth + 1));
                            pending.push(Write::K84(field_target, depth + 1));
                        }
                        SemanticSiteRecipeKind::CallableCall {
                            callee: field_callee,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K130(field_callee, depth + 1));
                        }
                        SemanticSiteRecipeKind::TypeUse {
                            expected: field_expected,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K130(field_expected, depth + 1));
                        }
                        SemanticSiteRecipeKind::DeclareLocal {
                            expected: field_expected,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K130(field_expected, depth + 1));
                        }
                        SemanticSiteRecipeKind::Closure {
                            definition: field_definition,
                            environment: field_environment,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K162(field_environment, depth + 1));
                            pending.push(Write::K84(field_definition, depth + 1));
                        }
                        SemanticSiteRecipeKind::OriginalCall => {
                            self.u8(6)?;
                        }
                    }
                }
                Write::K162(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K128(item, depth));
                    }
                }
                Write::K163(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticSiteAddress::Core {
                            expression: field_expression,
                            occurrence: field_occurrence,
                            role: field_role,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K165(field_role, depth + 1));
                            pending.push(Write::K6(field_occurrence, depth + 1));
                            pending.push(Write::K56(field_expression, depth + 1));
                        }
                        SemanticSiteAddress::Typed {
                            block: field_block,
                            instruction: field_instruction,
                            edge: field_edge,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K167(field_edge, depth + 1));
                            pending.push(Write::K6(field_instruction, depth + 1));
                            pending.push(Write::K166(field_block, depth + 1));
                        }
                    }
                }
                Write::K164(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticSiteAction::Construct {
                            result: field_result,
                            value: field_value,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K148(field_value, depth + 1));
                            pending.push(Write::K156(field_result, depth + 1));
                        }
                        SemanticSiteAction::Call {
                            target: field_target,
                            actuals: field_actuals,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K141(field_actuals, depth + 1));
                            pending.push(Write::K149(field_target, depth + 1));
                        }
                        SemanticSiteAction::CallableCall {
                            callee: field_callee,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K156(field_callee, depth + 1));
                        }
                        SemanticSiteAction::TypeUse {
                            subject: field_subject,
                            expected: field_expected,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K148(field_expected, depth + 1));
                            pending.push(Write::K156(field_subject, depth + 1));
                        }
                        SemanticSiteAction::DeclareLocal {
                            local: field_local,
                            expected: field_expected,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K148(field_expected, depth + 1));
                            pending.push(Write::K168(field_local, depth + 1));
                        }
                        SemanticSiteAction::Closure {
                            result: field_result,
                            closure: field_closure,
                            environment: field_environment,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K141(field_environment, depth + 1));
                            pending.push(Write::K150(field_closure, depth + 1));
                            pending.push(Write::K156(field_result, depth + 1));
                        }
                        SemanticSiteAction::CoreBodyCall => {
                            self.u8(6)?;
                        }
                        SemanticSiteAction::BodyCall {
                            actuals: field_actuals,
                        } => {
                            self.u8(7)?;
                            pending.push(Write::K141(field_actuals, depth + 1));
                        }
                    }
                }
                Write::K165(value, depth) => {
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
                Write::K166(value, depth) => {
                    let _ = depth;
                    self.u32((value).0)?;
                }
                Write::K167(value, depth) => {
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
                Write::K168(value, depth) => {
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
                            pending.push(Write::K160(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K169(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticRecipeOperand::TypeFormal(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K170(field_value_0, depth + 1));
                        }
                        SemanticRecipeOperand::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
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
                Write::K170(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K6(&value.ordinal, depth + 1));
                    pending.push(Write::K155(&value.scope, depth + 1));
                }
                Write::K171(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticLifeRecipe::Formal(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K170(field_value_0, depth + 1));
                        }
                        SemanticLifeRecipe::Implicit {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
                        }
                        SemanticLifeRecipe::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
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
                            pending.push(Write::K157(field_path, depth + 1));
                            pending.push(Write::K169(field_operand, depth + 1));
                        }
                        SemanticLifeRecipe::IntrinsicClass {
                            operand: field_operand,
                            path: field_path,
                            class: field_class,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K6(field_class, depth + 1));
                            pending.push(Write::K157(field_path, depth + 1));
                            pending.push(Write::K169(field_operand, depth + 1));
                        }
                    }
                }
                Write::K172(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOuterLifeRecipe::Class(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K171(field_value_0, depth + 1));
                        }
                        SemanticOuterLifeRecipe::Static => {
                            self.u8(1)?;
                        }
                    }
                }
                Write::K173(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K179(&value.classes, depth + 1));
                    pending.push(Write::K2(&value.life, depth + 1));
                    pending.push(Write::K1(&value.ty, depth + 1));
                }
                Write::K174(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticConstRecipe::Value(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K76(field_value_0, depth + 1));
                        }
                        SemanticConstRecipe::IntegerLiteral(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticConstRecipe::FloatLiteral(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticConstRecipe::Formal(field_value_0) => {
                            self.u8(3)?;
                            pending.push(Write::K170(field_value_0, depth + 1));
                        }
                        SemanticConstRecipe::Environment {
                            scope: field_scope,
                            ordinal: field_ordinal,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K6(field_ordinal, depth + 1));
                            pending.push(Write::K155(field_scope, depth + 1));
                        }
                        SemanticConstRecipe::Unary {
                            op: field_op,
                            operand_type: field_operand_type,
                            value: field_value,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K174(field_value, depth + 1));
                            pending.push(Write::K130(field_operand_type, depth + 1));
                            pending.push(Write::K193(field_op, depth + 1));
                        }
                        SemanticConstRecipe::Binary {
                            op: field_op,
                            operand_type: field_operand_type,
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K174(field_right, depth + 1));
                            pending.push(Write::K174(field_left, depth + 1));
                            pending.push(Write::K130(field_operand_type, depth + 1));
                            pending.push(Write::K194(field_op, depth + 1));
                        }
                        SemanticConstRecipe::Convert {
                            source_type: field_source_type,
                            target_type: field_target_type,
                            value: field_value,
                        } => {
                            self.u8(7)?;
                            pending.push(Write::K174(field_value, depth + 1));
                            pending.push(Write::K130(field_target_type, depth + 1));
                            pending.push(Write::K130(field_source_type, depth + 1));
                        }
                        SemanticConstRecipe::AssociatedConst {
                            receiver: field_receiver,
                            application: field_application,
                            member: field_member,
                        } => {
                            self.u8(8)?;
                            pending.push(Write::K6(field_member, depth + 1));
                            pending.push(Write::K131(field_application, depth + 1));
                            pending.push(Write::K130(field_receiver, depth + 1));
                        }
                    }
                }
                Write::K175(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K187(&value.nominal, depth + 1));
                    pending.push(Write::K185(&value.retained, depth + 1));
                    pending.push(Write::K191(&value.ports, depth + 1));
                    pending.push(Write::K180(&value.domain, depth + 1));
                }
                Write::K176(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K187(&value.nominal, depth + 1));
                    pending.push(Write::K185(&value.retained, depth + 1));
                    pending.push(Write::K183(&value.slots, depth + 1));
                    pending.push(Write::K180(&value.domain, depth + 1));
                }
                Write::K177(value, depth) => {
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
                Write::K178(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K198(&value.associated, depth + 1));
                    pending.push(Write::K197(&value.context, depth + 1));
                    pending.push(Write::K131(&value.application, depth + 1));
                }
                Write::K179(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K171(item, depth));
                    }
                }
                Write::K180(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K181(&value.parts, depth + 1));
                }
                Write::K181(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K182(item, depth));
                    }
                }
                Write::K182(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticClassSequencePart::One(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K171(field_value_0, depth + 1));
                        }
                        SemanticClassSequencePart::AllClasses {
                            ty: field_ty,
                            path: field_path,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K157(field_path, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
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
                    pending.push(Write::K171(&value.class, depth + 1));
                    pending.push(Write::K52(&value.mode, depth + 1));
                    pending.push(Write::K189(&value.path, depth + 1));
                }
                Write::K185(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K186(item, depth));
                    }
                }
                Write::K186(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticRetainedPart::One {
                            path: field_path,
                            child_class: field_child_class,
                            parent: field_parent,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K171(field_parent, depth + 1));
                            pending.push(Write::K6(field_child_class, depth + 1));
                            pending.push(Write::K157(field_path, depth + 1));
                        }
                        SemanticRetainedPart::All {
                            path: field_path,
                            parents: field_parents,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K180(field_parents, depth + 1));
                            pending.push(Write::K157(field_path, depth + 1));
                        }
                    }
                }
                Write::K187(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K188(item, depth));
                    }
                }
                Write::K188(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticNominalPart::One {
                            path: field_path,
                            declaration: field_declaration,
                            argument: field_argument,
                            class: field_class,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K171(field_class, depth + 1));
                            pending.push(Write::K6(field_argument, depth + 1));
                            pending.push(Write::K74(field_declaration, depth + 1));
                            pending.push(Write::K157(field_path, depth + 1));
                        }
                        SemanticNominalPart::FromType {
                            path: field_path,
                            ty: field_ty,
                            classes: field_classes,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K180(field_classes, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
                            pending.push(Write::K157(field_path, depth + 1));
                        }
                    }
                }
                Write::K189(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K190(item, depth));
                    }
                }
                Write::K190(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        PlaceProjection::Field(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K6(field_value_0, depth + 1));
                        }
                        PlaceProjection::Variant(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K243(field_value_0, depth + 1));
                        }
                        PlaceProjection::ErrorArm(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K33(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K191(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K192(item, depth));
                    }
                }
                Write::K192(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticPortPart::One {
                            leaf: field_leaf,
                            mode: field_mode,
                            class: field_class,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K171(field_class, depth + 1));
                            pending.push(Write::K52(field_mode, depth + 1));
                            pending.push(Write::K130(field_leaf, depth + 1));
                        }
                        SemanticPortPart::FromType {
                            ty: field_ty,
                            classes: field_classes,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K180(field_classes, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
                        }
                    }
                }
                Write::K193(value, depth) => {
                    let _ = depth;
                    self.unary(*value)?;
                }
                Write::K194(value, depth) => {
                    let _ = depth;
                    self.binary(*value)?;
                }
                Write::K195(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K196(item, depth));
                    }
                }
                Write::K196(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K23(&value.mode, depth + 1));
                    pending.push(Write::K130(&value.ty, depth + 1));
                }
                Write::K197(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticDynContextRecipe::Closed => {
                            self.u8(0)?;
                        }
                        SemanticDynContextRecipe::Type(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticDynContextRecipe::Shape(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K175(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K198(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K199(item, depth));
                    }
                }
                Write::K199(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K200(&value.value, depth + 1));
                    pending.push(Write::K6(&value.member, depth + 1));
                    pending.push(Write::K131(&value.application, depth + 1));
                }
                Write::K200(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticAssociatedRecipe::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticAssociatedRecipe::Const {
                            carrier: field_carrier,
                            value: field_value,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K174(field_value, depth + 1));
                            pending.push(Write::K130(field_carrier, depth + 1));
                        }
                    }
                }
                Write::K201(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K130(item, depth));
                    }
                }
                Write::K202(value, depth) => {
                    let _ = depth;
                    self.span(*value)?;
                }
                Write::K203(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K202(&value.span, depth + 1));
                    pending.push(Write::K204(&value.kind, depth + 1));
                }
                Write::K204(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalPatternKind::Name(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalPatternKind::Wildcard => {
                            self.u8(1)?;
                        }
                        SemanticOriginalPatternKind::Tuple(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K205(field_value_0, depth + 1));
                        }
                        SemanticOriginalPatternKind::Record {
                            ty: field_ty,
                            fields: field_fields,
                        } => {
                            self.u8(3)?;
                            pending.push(Write::K206(field_fields, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
                        }
                        SemanticOriginalPatternKind::Reference {
                            mutable: field_mutable,
                            pattern: field_pattern,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K203(field_pattern, depth + 1));
                            pending.push(Write::K33(field_mutable, depth + 1));
                        }
                        SemanticOriginalPatternKind::ErrorArm {
                            is_error: field_is_error,
                            pattern: field_pattern,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K203(field_pattern, depth + 1));
                            pending.push(Write::K33(field_is_error, depth + 1));
                        }
                        SemanticOriginalPatternKind::Variant {
                            ty: field_ty,
                            variant: field_variant,
                            fields: field_fields,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K205(field_fields, depth + 1));
                            pending.push(Write::K124(field_variant, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
                        }
                        SemanticOriginalPatternKind::Integer(field_value_0) => {
                            self.u8(7)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalPatternKind::Bool(field_value_0) => {
                            self.u8(8)?;
                            pending.push(Write::K33(field_value_0, depth + 1));
                        }
                        SemanticOriginalPatternKind::Constructor {
                            definition: field_definition,
                            variant: field_variant,
                            fields: field_fields,
                        } => {
                            self.u8(9)?;
                            pending.push(Write::K205(field_fields, depth + 1));
                            pending.push(Write::K124(field_variant, depth + 1));
                            pending.push(Write::K96(field_definition, depth + 1));
                        }
                        SemanticOriginalPatternKind::RecordConstructor {
                            definition: field_definition,
                            fields: field_fields,
                        } => {
                            self.u8(10)?;
                            pending.push(Write::K206(field_fields, depth + 1));
                            pending.push(Write::K96(field_definition, depth + 1));
                        }
                        SemanticOriginalPatternKind::Float(field_value_0) => {
                            self.u8(11)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalPatternKind::String(field_value_0) => {
                            self.u8(12)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalPatternKind::Character(field_value_0) => {
                            self.u8(13)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K205(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K203(item, depth));
                    }
                }
                Write::K206(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K207(item, depth));
                    }
                }
                Write::K207(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K203(&value.1, depth + 1));
                    pending.push(Write::K124(&value.0, depth + 1));
                }
                Write::K208(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K202(&value.span, depth + 1));
                    pending.push(Write::K121(&value.body, depth + 1));
                    pending.push(Write::K203(&value.pattern, depth + 1));
                }
                Write::K209(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K210(item, depth));
                    }
                }
                Write::K210(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K202(&value.span, depth + 1));
                    pending.push(Write::K211(&value.kind, depth + 1));
                    pending.push(Write::K56(&value.id, depth + 1));
                }
                Write::K211(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalStatementKind::Binding {
                            name: field_name,
                            mutable: field_mutable,
                            ty: field_ty,
                            initializer: field_initializer,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K212(field_initializer, depth + 1));
                            pending.push(Write::K201(field_ty, depth + 1));
                            pending.push(Write::K33(field_mutable, depth + 1));
                            pending.push(Write::K124(field_name, depth + 1));
                        }
                        SemanticOriginalStatementKind::TupleBinding {
                            pattern: field_pattern,
                            mutable: field_mutable,
                            ty: field_ty,
                            initializer: field_initializer,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K213(field_initializer, depth + 1));
                            pending.push(Write::K201(field_ty, depth + 1));
                            pending.push(Write::K33(field_mutable, depth + 1));
                            pending.push(Write::K203(field_pattern, depth + 1));
                        }
                        SemanticOriginalStatementKind::Assign {
                            target: field_target,
                            value: field_value,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K213(field_value, depth + 1));
                            pending.push(Write::K213(field_target, depth + 1));
                        }
                        SemanticOriginalStatementKind::Expression(field_value_0) => {
                            self.u8(3)?;
                            pending.push(Write::K213(field_value_0, depth + 1));
                        }
                        SemanticOriginalStatementKind::Return(field_value_0) => {
                            self.u8(4)?;
                            pending.push(Write::K212(field_value_0, depth + 1));
                        }
                        SemanticOriginalStatementKind::If {
                            condition: field_condition,
                            then_body: field_then_body,
                            else_body: field_else_body,
                        } => {
                            self.u8(5)?;
                            pending.push(Write::K214(field_else_body, depth + 1));
                            pending.push(Write::K121(field_then_body, depth + 1));
                            pending.push(Write::K213(field_condition, depth + 1));
                        }
                        SemanticOriginalStatementKind::IfLet {
                            pattern: field_pattern,
                            value: field_value,
                            then_body: field_then_body,
                            else_body: field_else_body,
                        } => {
                            self.u8(6)?;
                            pending.push(Write::K214(field_else_body, depth + 1));
                            pending.push(Write::K121(field_then_body, depth + 1));
                            pending.push(Write::K213(field_value, depth + 1));
                            pending.push(Write::K203(field_pattern, depth + 1));
                        }
                        SemanticOriginalStatementKind::While {
                            condition: field_condition,
                            body: field_body,
                        } => {
                            self.u8(7)?;
                            pending.push(Write::K121(field_body, depth + 1));
                            pending.push(Write::K213(field_condition, depth + 1));
                        }
                        SemanticOriginalStatementKind::WhileLet {
                            pattern: field_pattern,
                            value: field_value,
                            body: field_body,
                        } => {
                            self.u8(8)?;
                            pending.push(Write::K121(field_body, depth + 1));
                            pending.push(Write::K213(field_value, depth + 1));
                            pending.push(Write::K203(field_pattern, depth + 1));
                        }
                        SemanticOriginalStatementKind::ForRange {
                            name: field_name,
                            start: field_start,
                            end: field_end,
                            body: field_body,
                        } => {
                            self.u8(9)?;
                            pending.push(Write::K121(field_body, depth + 1));
                            pending.push(Write::K213(field_end, depth + 1));
                            pending.push(Write::K213(field_start, depth + 1));
                            pending.push(Write::K124(field_name, depth + 1));
                        }
                        SemanticOriginalStatementKind::Match {
                            value: field_value,
                            arms: field_arms,
                        } => {
                            self.u8(10)?;
                            pending.push(Write::K215(field_arms, depth + 1));
                            pending.push(Write::K213(field_value, depth + 1));
                        }
                        SemanticOriginalStatementKind::Block(field_value_0) => {
                            self.u8(11)?;
                            pending.push(Write::K121(field_value_0, depth + 1));
                        }
                        SemanticOriginalStatementKind::Defer(field_value_0) => {
                            self.u8(12)?;
                            pending.push(Write::K121(field_value_0, depth + 1));
                        }
                        SemanticOriginalStatementKind::Unsafe(field_value_0) => {
                            self.u8(13)?;
                            pending.push(Write::K121(field_value_0, depth + 1));
                        }
                        SemanticOriginalStatementKind::Break => {
                            self.u8(14)?;
                        }
                        SemanticOriginalStatementKind::Continue => {
                            self.u8(15)?;
                        }
                        SemanticOriginalStatementKind::Unsupported(field_value_0) => {
                            self.u8(16)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalStatementKind::ForEach {
                            pattern: field_pattern,
                            iterator: field_iterator,
                            body: field_body,
                        } => {
                            self.u8(17)?;
                            pending.push(Write::K121(field_body, depth + 1));
                            pending.push(Write::K213(field_iterator, depth + 1));
                            pending.push(Write::K203(field_pattern, depth + 1));
                        }
                        SemanticOriginalStatementKind::CompoundAssign {
                            target: field_target,
                            op: field_op,
                            value: field_value,
                        } => {
                            self.u8(18)?;
                            pending.push(Write::K213(field_value, depth + 1));
                            pending.push(Write::K194(field_op, depth + 1));
                            pending.push(Write::K213(field_target, depth + 1));
                        }
                        SemanticOriginalStatementKind::Yield(field_value_0) => {
                            self.u8(19)?;
                            pending.push(Write::K213(field_value_0, depth + 1));
                        }
                        SemanticOriginalStatementKind::Loop(field_value_0) => {
                            self.u8(20)?;
                            pending.push(Write::K121(field_value_0, depth + 1));
                        }
                        SemanticOriginalStatementKind::With {
                            name: field_name,
                            value: field_value,
                            body: field_body,
                        } => {
                            self.u8(21)?;
                            pending.push(Write::K121(field_body, depth + 1));
                            pending.push(Write::K213(field_value, depth + 1));
                            pending.push(Write::K124(field_name, depth + 1));
                        }
                        SemanticOriginalStatementKind::CompileTime(field_value_0) => {
                            self.u8(22)?;
                            pending.push(Write::K210(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K212(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K213(item, depth));
                    }
                }
                Write::K213(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K202(&value.span, depth + 1));
                    pending.push(Write::K216(&value.kind, depth + 1));
                    pending.push(Write::K56(&value.id, depth + 1));
                }
                Write::K214(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K121(item, depth));
                    }
                }
                Write::K215(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K208(item, depth));
                    }
                }
                Write::K216(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalExprKind::MakeDynBorrow {
                            interface: field_interface,
                            vtable: field_vtable,
                            source: field_source,
                            mutable: field_mutable,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K33(field_mutable, depth + 1));
                            pending.push(Write::K213(field_source, depth + 1));
                            pending.push(Write::K218(field_vtable, depth + 1));
                            pending.push(Write::K217(field_interface, depth + 1));
                        }
                        SemanticOriginalExprKind::MakeDynOwned {
                            interface: field_interface,
                            vtable: field_vtable,
                            source: field_source,
                            route: field_route,
                        } => {
                            self.u8(1)?;
                            pending.push(Write::K219(field_route, depth + 1));
                            pending.push(Write::K213(field_source, depth + 1));
                            pending.push(Write::K218(field_vtable, depth + 1));
                            pending.push(Write::K217(field_interface, depth + 1));
                        }
                        SemanticOriginalExprKind::CallDynSlot {
                            callee: field_callee,
                            interface: field_interface,
                            slot: field_slot,
                            arguments: field_arguments,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K220(field_arguments, depth + 1));
                            pending.push(Write::K6(field_slot, depth + 1));
                            pending.push(Write::K217(field_interface, depth + 1));
                            pending.push(Write::K213(field_callee, depth + 1));
                        }
                        SemanticOriginalExprKind::Literal(field_value_0) => {
                            self.u8(3)?;
                            pending.push(Write::K76(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Ascribed {
                            ty: field_ty,
                            value: field_value,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K213(field_value, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
                        }
                        SemanticOriginalExprKind::Integer(field_value_0) => {
                            self.u8(5)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Float(field_value_0) => {
                            self.u8(6)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::NumericConvert {
                            value: field_value,
                            target: field_target,
                        } => {
                            self.u8(7)?;
                            pending.push(Write::K130(field_target, depth + 1));
                            pending.push(Write::K213(field_value, depth + 1));
                        }
                        SemanticOriginalExprKind::IntegerConvert {
                            value: field_value,
                            target: field_target,
                        } => {
                            self.u8(8)?;
                            pending.push(Write::K130(field_target, depth + 1));
                            pending.push(Write::K213(field_value, depth + 1));
                        }
                        SemanticOriginalExprKind::Bool(field_value_0) => {
                            self.u8(9)?;
                            pending.push(Write::K33(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::String(field_value_0) => {
                            self.u8(10)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Unit => {
                            self.u8(11)?;
                        }
                        SemanticOriginalExprKind::Name(field_value_0) => {
                            self.u8(12)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Unary {
                            op: field_op,
                            operand: field_operand,
                        } => {
                            self.u8(13)?;
                            pending.push(Write::K213(field_operand, depth + 1));
                            pending.push(Write::K193(field_op, depth + 1));
                        }
                        SemanticOriginalExprKind::Binary {
                            op: field_op,
                            left: field_left,
                            right: field_right,
                        } => {
                            self.u8(14)?;
                            pending.push(Write::K213(field_right, depth + 1));
                            pending.push(Write::K213(field_left, depth + 1));
                            pending.push(Write::K194(field_op, depth + 1));
                        }
                        SemanticOriginalExprKind::TypeApply {
                            callee: field_callee,
                            type_arguments: field_type_arguments,
                        } => {
                            self.u8(15)?;
                            pending.push(Write::K137(field_type_arguments, depth + 1));
                            pending.push(Write::K213(field_callee, depth + 1));
                        }
                        SemanticOriginalExprKind::Call {
                            callee: field_callee,
                            arguments: field_arguments,
                        } => {
                            self.u8(16)?;
                            pending.push(Write::K220(field_arguments, depth + 1));
                            pending.push(Write::K213(field_callee, depth + 1));
                        }
                        SemanticOriginalExprKind::Tuple(field_value_0) => {
                            self.u8(17)?;
                            pending.push(Write::K220(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Array(field_value_0) => {
                            self.u8(18)?;
                            pending.push(Write::K220(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Index {
                            object: field_object,
                            index: field_index,
                        } => {
                            self.u8(19)?;
                            pending.push(Write::K213(field_index, depth + 1));
                            pending.push(Write::K213(field_object, depth + 1));
                        }
                        SemanticOriginalExprKind::Slice {
                            object: field_object,
                            start: field_start,
                            end: field_end,
                        } => {
                            self.u8(20)?;
                            pending.push(Write::K221(field_end, depth + 1));
                            pending.push(Write::K221(field_start, depth + 1));
                            pending.push(Write::K213(field_object, depth + 1));
                        }
                        SemanticOriginalExprKind::Member {
                            object: field_object,
                            member: field_member,
                            owner: field_owner,
                        } => {
                            self.u8(21)?;
                            pending.push(Write::K201(field_owner, depth + 1));
                            pending.push(Write::K124(field_member, depth + 1));
                            pending.push(Write::K213(field_object, depth + 1));
                        }
                        SemanticOriginalExprKind::ReceiverMember(field_value_0) => {
                            self.u8(22)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Record {
                            ty: field_ty,
                            fields: field_fields,
                        } => {
                            self.u8(23)?;
                            pending.push(Write::K222(field_fields, depth + 1));
                            pending.push(Write::K130(field_ty, depth + 1));
                        }
                        SemanticOriginalExprKind::RecordUpdate {
                            ty: field_ty,
                            base: field_base,
                            fields: field_fields,
                        } => {
                            self.u8(24)?;
                            pending.push(Write::K222(field_fields, depth + 1));
                            pending.push(Write::K213(field_base, depth + 1));
                            pending.push(Write::K201(field_ty, depth + 1));
                        }
                        SemanticOriginalExprKind::Sequence {
                            bindings: field_bindings,
                            value: field_value,
                        } => {
                            self.u8(25)?;
                            pending.push(Write::K213(field_value, depth + 1));
                            pending.push(Write::K222(field_bindings, depth + 1));
                        }
                        SemanticOriginalExprKind::CompileTime(field_value_0) => {
                            self.u8(26)?;
                            pending.push(Write::K213(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Try(field_value_0) => {
                            self.u8(27)?;
                            pending.push(Write::K213(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Closure {
                            move_capture: field_move_capture,
                            parameters: field_parameters,
                            return_type: field_return_type,
                            body: field_body,
                        } => {
                            self.u8(28)?;
                            pending.push(Write::K121(field_body, depth + 1));
                            pending.push(Write::K201(field_return_type, depth + 1));
                            pending.push(Write::K113(field_parameters, depth + 1));
                            pending.push(Write::K33(field_move_capture, depth + 1));
                        }
                        SemanticOriginalExprKind::If {
                            condition: field_condition,
                            then_value: field_then_value,
                            else_value: field_else_value,
                        } => {
                            self.u8(29)?;
                            pending.push(Write::K213(field_else_value, depth + 1));
                            pending.push(Write::K213(field_then_value, depth + 1));
                            pending.push(Write::K213(field_condition, depth + 1));
                        }
                        SemanticOriginalExprKind::Unsupported(field_value_0) => {
                            self.u8(30)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Spread(field_value_0) => {
                            self.u8(31)?;
                            pending.push(Write::K213(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Group(field_value_0) => {
                            self.u8(32)?;
                            pending.push(Write::K213(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Character(field_value_0) => {
                            self.u8(33)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::Range {
                            start: field_start,
                            end: field_end,
                        } => {
                            self.u8(34)?;
                            pending.push(Write::K213(field_end, depth + 1));
                            pending.push(Write::K213(field_start, depth + 1));
                        }
                        SemanticOriginalExprKind::Pipeline {
                            input: field_input,
                            target: field_target,
                        } => {
                            self.u8(35)?;
                            pending.push(Write::K213(field_target, depth + 1));
                            pending.push(Write::K213(field_input, depth + 1));
                        }
                        SemanticOriginalExprKind::RecordConstructor {
                            definition: field_definition,
                            fields: field_fields,
                        } => {
                            self.u8(36)?;
                            pending.push(Write::K222(field_fields, depth + 1));
                            pending.push(Write::K96(field_definition, depth + 1));
                        }
                        SemanticOriginalExprKind::Splice(field_value_0) => {
                            self.u8(37)?;
                            pending.push(Write::K213(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::CompileTimeBlock(field_value_0) => {
                            self.u8(38)?;
                            pending.push(Write::K121(field_value_0, depth + 1));
                        }
                        SemanticOriginalExprKind::SyntaxQuote {
                            category: field_category,
                            template: field_template,
                        } => {
                            self.u8(39)?;
                            pending.push(Write::K225(field_template, depth + 1));
                            pending.push(Write::K224(field_category, depth + 1));
                        }
                        SemanticOriginalExprKind::SyntaxIsland {
                            path: field_path,
                            tree: field_tree,
                        } => {
                            self.u8(40)?;
                            pending.push(Write::K226(field_tree, depth + 1));
                            pending.push(Write::K126(field_path, depth + 1));
                        }
                        SemanticOriginalExprKind::GenericApply {
                            callee: field_callee,
                            arguments: field_arguments,
                        } => {
                            self.u8(41)?;
                            pending.push(Write::K227(field_arguments, depth + 1));
                            pending.push(Write::K213(field_callee, depth + 1));
                        }
                    }
                }
                Write::K217(value, depth) => {
                    let _ = depth;
                    self.dyn_interface_id(value)?;
                }
                Write::K218(value, depth) => {
                    let _ = depth;
                    self.dyn_vtable_id(value)?;
                }
                Write::K219(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        DynOwnedStorage::Heap => {
                            self.u8(0)?;
                        }
                    }
                }
                Write::K220(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K213(item, depth));
                    }
                }
                Write::K221(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K213(item, depth));
                    }
                }
                Write::K222(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K223(item, depth));
                    }
                }
                Write::K223(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K213(&value.1, depth + 1));
                    pending.push(Write::K124(&value.0, depth + 1));
                }
                Write::K224(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalSyntaxCategory::Expr => {
                            self.u8(0)?;
                        }
                        SemanticOriginalSyntaxCategory::Type => {
                            self.u8(1)?;
                        }
                        SemanticOriginalSyntaxCategory::StmtList => {
                            self.u8(2)?;
                        }
                        SemanticOriginalSyntaxCategory::ItemList => {
                            self.u8(3)?;
                        }
                    }
                }
                Write::K225(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K232(&value.name_sites, depth + 1));
                    pending.push(Write::K231(&value.holes, depth + 1));
                    pending.push(Write::K226(&value.tree, depth + 1));
                }
                Write::K226(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K202(&value.span, depth + 1));
                    pending.push(Write::K237(&value.children, depth + 1));
                    pending.push(Write::K234(&value.delimiter, depth + 1));
                }
                Write::K227(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K228(item, depth));
                    }
                }
                Write::K228(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalGenericArgument::Type(field_value_0) => {
                            self.u8(0)?;
                            pending.push(Write::K130(field_value_0, depth + 1));
                        }
                        SemanticOriginalGenericArgument::Const(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K213(field_value_0, depth + 1));
                        }
                        SemanticOriginalGenericArgument::Lifetime(field_value_0) => {
                            self.u8(2)?;
                            pending.push(Write::K124(field_value_0, depth + 1));
                        }
                    }
                }
                Write::K229(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K202(&value.span, depth + 1));
                    pending.push(Write::K213(&value.value, depth + 1));
                    pending.push(Write::K230(&value.kind, depth + 1));
                }
                Write::K230(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalQuoteHoleKind::Expr => {
                            self.u8(0)?;
                        }
                        SemanticOriginalQuoteHoleKind::Type => {
                            self.u8(1)?;
                        }
                        SemanticOriginalQuoteHoleKind::StmtList => {
                            self.u8(2)?;
                        }
                        SemanticOriginalQuoteHoleKind::ItemList => {
                            self.u8(3)?;
                        }
                        SemanticOriginalQuoteHoleKind::Ident {
                            namespace: field_namespace,
                            role: field_role,
                        } => {
                            self.u8(4)?;
                            pending.push(Write::K236(field_role, depth + 1));
                            pending.push(Write::K235(field_namespace, depth + 1));
                        }
                    }
                }
                Write::K231(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K229(item, depth));
                    }
                }
                Write::K232(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K233(item, depth));
                    }
                }
                Write::K233(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    pending.push(Write::K6(&value.token_index, depth + 1));
                    pending.push(Write::K202(&value.span, depth + 1));
                }
                Write::K234(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalSyntaxDelimiter::Parenthesis => {
                            self.u8(0)?;
                        }
                        SemanticOriginalSyntaxDelimiter::Bracket => {
                            self.u8(1)?;
                        }
                        SemanticOriginalSyntaxDelimiter::Brace => {
                            self.u8(2)?;
                        }
                    }
                }
                Write::K235(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalQuoteNameNamespace::Value => {
                            self.u8(0)?;
                        }
                        SemanticOriginalQuoteNameNamespace::Type => {
                            self.u8(1)?;
                        }
                        SemanticOriginalQuoteNameNamespace::Member => {
                            self.u8(2)?;
                        }
                        SemanticOriginalQuoteNameNamespace::Module => {
                            self.u8(3)?;
                        }
                        SemanticOriginalQuoteNameNamespace::Label => {
                            self.u8(4)?;
                        }
                    }
                }
                Write::K236(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalQuoteNameRole::Binding => {
                            self.u8(0)?;
                        }
                        SemanticOriginalQuoteNameRole::Reference => {
                            self.u8(1)?;
                        }
                    }
                }
                Write::K237(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K238(item, depth));
                    }
                }
                Write::K238(value, depth) => {
                    self.dyn_recipe_node(depth)?;
                    match value {
                        SemanticOriginalSyntaxTreeEntry::Token {
                            text: field_text,
                            span: field_span,
                        } => {
                            self.u8(0)?;
                            pending.push(Write::K202(field_span, depth + 1));
                            pending.push(Write::K124(field_text, depth + 1));
                        }
                        SemanticOriginalSyntaxTreeEntry::Tree(field_value_0) => {
                            self.u8(1)?;
                            pending.push(Write::K226(field_value_0, depth + 1));
                        }
                        SemanticOriginalSyntaxTreeEntry::Hole {
                            index: field_index,
                            span: field_span,
                        } => {
                            self.u8(2)?;
                            pending.push(Write::K202(field_span, depth + 1));
                            pending.push(Write::K6(field_index, depth + 1));
                        }
                    }
                }
                Write::K239(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K240(item, depth));
                    }
                }
                Write::K240(value, depth) => {
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
                Write::K241(value, depth) => {
                    self.boolean(value.is_some())?;
                    if let Some(item) = value {
                        pending.push(Write::K14(item, depth));
                    }
                }
                Write::K242(value, depth) => {
                    self.count(value.len(), self.limits.max_nodes)?;
                    for item in value.iter().rev() {
                        pending.push(Write::K16(item, depth));
                    }
                }
                Write::K243(value, depth) => {
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
                                pending.push(Read::Start(Kind::K241, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K242, depth + 1));
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
                                pending.push(Read::Start(Kind::K241, depth + 1));
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
                                pending.push(Read::Start(Kind::K130, depth + 1));
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
                        pending.push(Read::Start(Kind::K239, depth + 1));
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
                                pending.push(Read::Start(Kind::K193, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 4));
                                pending.push(Read::Start(Kind::K16, depth + 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K21, depth + 1));
                                pending.push(Read::Start(Kind::K194, depth + 1));
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
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
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
                        pending.push(Read::Start(Kind::K147, depth + 1));
                        pending.push(Read::Start(Kind::K145, depth + 1));
                        pending.push(Read::Start(Kind::K143, depth + 1));
                        pending.push(Read::Start(Kind::K141, depth + 1));
                        pending.push(Read::Start(Kind::K140, depth + 1));
                        pending.push(Read::Start(Kind::K139, depth + 1));
                        pending.push(Read::Start(Kind::K138, depth + 1));
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
                        pending.push(Read::Start(Kind::K164, depth + 1));
                        pending.push(Read::Start(Kind::K163, depth + 1));
                        pending.push(Read::Start(Kind::K129, depth + 1));
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
                                pending.push(Read::Start(Kind::K130, depth + 1));
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
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K131, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 3));
                                pending.push(Read::Start(Kind::K107, depth + 1));
                                pending.push(Read::Start(Kind::K19, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 3));
                                pending.push(Read::Start(Kind::K174, depth + 1));
                                pending.push(Read::Start(Kind::K174, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 2));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 2));
                                pending.push(Read::Start(Kind::K102, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 2));
                                pending.push(Read::Start(Kind::K132, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
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
                        pending.push(Read::Start(Kind::K128, depth + 1));
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
                            3 => {
                                pending.push(Read::Finish(kind, 3, 7));
                                pending.push(Read::Start(Kind::K120, depth + 1));
                                pending.push(Read::Start(Kind::K118, depth + 1));
                                pending.push(Read::Start(Kind::K116, depth + 1));
                                pending.push(Read::Start(Kind::K115, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K108, depth + 1));
                                pending.push(Read::Start(Kind::K107, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K107 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 6));
                        pending.push(Read::Start(Kind::K25, depth + 1));
                        pending.push(Read::Start(Kind::K23, depth + 1));
                        pending.push(Read::Start(Kind::K130, depth + 1));
                        pending.push(Read::Start(Kind::K195, depth + 1));
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
                        pending.push(Read::Start(Kind::K161, depth + 1));
                        pending.push(Read::Start(Kind::K11, depth + 1));
                        pending.push(Read::Start(Kind::K129, depth + 1));
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
                                pending.push(Read::Start(Kind::K133, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K135, depth + 1));
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
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 3));
                                pending.push(Read::Start(Kind::K131, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K132, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K113 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K114, depth));
                        }
                    }
                    Kind::K114 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K202, depth + 1));
                        pending.push(Read::Start(Kind::K23, depth + 1));
                        pending.push(Read::Start(Kind::K201, depth + 1));
                        pending.push(Read::Start(Kind::K124, depth + 1));
                    }
                    Kind::K115 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K6, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K116 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K117, depth));
                        }
                    }
                    Kind::K117 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K127, depth + 1));
                        pending.push(Read::Start(Kind::K125, depth + 1));
                        pending.push(Read::Start(Kind::K126, depth + 1));
                    }
                    Kind::K118 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K119, depth));
                        }
                    }
                    Kind::K119 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K123, depth + 1));
                        pending.push(Read::Start(Kind::K129, depth + 1));
                    }
                    Kind::K120 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K121, depth + 1));
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
                    Kind::K121 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K202, depth + 1));
                        pending.push(Read::Start(Kind::K209, depth + 1));
                    }
                    Kind::K122 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 0));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K11, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 2));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                                pending.push(Read::Start(Kind::K123, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K123 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K124 => {
                        let _ = depth;
                        values.push(Value::K124(self.text()?));
                    }
                    Kind::K125 => {
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
                    Kind::K126 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K124, depth));
                        }
                    }
                    Kind::K127 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K128, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K128 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K174, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K129 => {
                        let _ = depth;
                        values.push(Value::K129(Box::new(SemanticSiteId(self.u32()?))));
                    }
                    Kind::K130 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K1, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K173, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K170, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K162, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K131, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 1));
                                pending.push(Read::Start(Kind::K137, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 2));
                                pending.push(Read::Start(Kind::K174, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            10 => {
                                pending.push(Read::Finish(kind, 10, 3));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K172, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            11 => {
                                pending.push(Read::Finish(kind, 11, 2));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K172, depth + 1));
                            }
                            12 => {
                                pending.push(Read::Finish(kind, 12, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            13 => {
                                pending.push(Read::Finish(kind, 13, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            14 => {
                                pending.push(Read::Finish(kind, 14, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            15 => {
                                pending.push(Read::Finish(kind, 15, 2));
                                pending.push(Read::Start(Kind::K175, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            16 => {
                                pending.push(Read::Finish(kind, 16, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            17 => {
                                pending.push(Read::Finish(kind, 17, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            18 => {
                                pending.push(Read::Finish(kind, 18, 2));
                                pending.push(Read::Start(Kind::K175, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            19 => {
                                pending.push(Read::Finish(kind, 19, 2));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            20 => {
                                pending.push(Read::Finish(kind, 20, 2));
                                pending.push(Read::Start(Kind::K176, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            21 => {
                                pending.push(Read::Finish(kind, 21, 2));
                                pending.push(Read::Start(Kind::K107, depth + 1));
                                pending.push(Read::Start(Kind::K177, depth + 1));
                            }
                            22 => {
                                pending.push(Read::Finish(kind, 22, 3));
                                pending.push(Read::Start(Kind::K162, depth + 1));
                                pending.push(Read::Start(Kind::K162, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            23 => {
                                pending.push(Read::Finish(kind, 23, 2));
                                pending.push(Read::Start(Kind::K178, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            24 => {
                                pending.push(Read::Finish(kind, 24, 3));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K162, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            25 => {
                                pending.push(Read::Finish(kind, 25, 1));
                                pending.push(Read::Start(Kind::K137, depth + 1));
                            }
                            26 => {
                                pending.push(Read::Finish(kind, 26, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            27 => {
                                pending.push(Read::Finish(kind, 27, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            28 => {
                                pending.push(Read::Finish(kind, 28, 2));
                                pending.push(Read::Start(Kind::K122, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K131 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K162, depth + 1));
                        pending.push(Read::Start(Kind::K74, depth + 1));
                    }
                    Kind::K132 => {
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
                                pending.push(Read::Finish(kind, 8, 0));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 0));
                            }
                            10 => {
                                pending.push(Read::Finish(kind, 10, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K133 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K134, depth));
                        }
                    }
                    Kind::K134 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K130, depth + 1));
                        pending.push(Read::Start(Kind::K124, depth + 1));
                    }
                    Kind::K135 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K136, depth));
                        }
                    }
                    Kind::K136 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K137, depth + 1));
                        pending.push(Read::Start(Kind::K124, depth + 1));
                    }
                    Kind::K137 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K130, depth));
                        }
                    }
                    Kind::K138 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K149, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K139 => {
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
                    Kind::K140 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K85, depth));
                        } else {
                            values.push(Value::Optional(None));
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
                                pending.push(Read::Start(Kind::K148, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K76, depth + 1));
                                pending.push(Read::Start(Kind::K148, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K152, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K128, depth + 1));
                                pending.push(Read::Start(Kind::K85, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K129, depth + 1));
                                pending.push(Read::Start(Kind::K85, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K143 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K144, depth));
                        }
                    }
                    Kind::K144 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K152, depth + 1));
                        pending.push(Read::Start(Kind::K4, depth + 1));
                    }
                    Kind::K145 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K146, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K146 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K141, depth + 1));
                        pending.push(Read::Start(Kind::K150, depth + 1));
                    }
                    Kind::K147 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K148, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K148 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 3));
                                pending.push(Read::Start(Kind::K154, depth + 1));
                                pending.push(Read::Start(Kind::K2, depth + 1));
                                pending.push(Read::Start(Kind::K1, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K153, depth + 1));
                                pending.push(Read::Start(Kind::K85, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K149 => {
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
                    Kind::K150 => {
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
                                pending.push(Read::Start(Kind::K151, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K151 => {
                        let _ = depth;
                        values.push(Value::K151(Box::new(ClosureId(self.u32()?))));
                    }
                    Kind::K152 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 3));
                                pending.push(Read::Start(Kind::K52, depth + 1));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K153 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 5));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K55, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 5));
                                pending.push(Read::Start(Kind::K122, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                                pending.push(Read::Start(Kind::K55, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K154 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K152, depth));
                        }
                    }
                    Kind::K155 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K156 => {
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
                                pending.push(Read::Start(Kind::K159, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K160, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
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
                    Kind::K157 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K158, depth));
                        }
                    }
                    Kind::K158 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K190, depth + 1));
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
                    Kind::K159 => {
                        let _ = depth;
                        values.push(Value::K159(Box::new(ValueId(self.u32()?))));
                    }
                    Kind::K160 => {
                        let _ = depth;
                        values.push(Value::K160(Box::new(LocalId(self.u32()?))));
                    }
                    Kind::K161 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K162, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K162, depth + 1));
                                pending.push(Read::Start(Kind::K84, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K162 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K128, depth));
                        }
                    }
                    Kind::K163 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 3));
                                pending.push(Read::Start(Kind::K165, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 3));
                                pending.push(Read::Start(Kind::K167, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K166, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K164 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K148, depth + 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                                pending.push(Read::Start(Kind::K149, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K148, depth + 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 2));
                                pending.push(Read::Start(Kind::K148, depth + 1));
                                pending.push(Read::Start(Kind::K168, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 3));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                                pending.push(Read::Start(Kind::K150, depth + 1));
                                pending.push(Read::Start(Kind::K156, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 0));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 1));
                                pending.push(Read::Start(Kind::K141, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K165 => {
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
                    Kind::K166 => {
                        let _ = depth;
                        values.push(Value::K166(Box::new(BlockId(self.u32()?))));
                    }
                    Kind::K167 => {
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
                    Kind::K168 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K56, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K160, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K169 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K170, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
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
                    Kind::K170 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K155, depth + 1));
                    }
                    Kind::K171 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K170, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
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
                                pending.push(Read::Start(Kind::K157, depth + 1));
                                pending.push(Read::Start(Kind::K169, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                                pending.push(Read::Start(Kind::K169, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K172 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K173 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K179, depth + 1));
                        pending.push(Read::Start(Kind::K2, depth + 1));
                        pending.push(Read::Start(Kind::K1, depth + 1));
                    }
                    Kind::K174 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K76, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 1));
                                pending.push(Read::Start(Kind::K170, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 2));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K155, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 3));
                                pending.push(Read::Start(Kind::K174, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K193, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 4));
                                pending.push(Read::Start(Kind::K174, depth + 1));
                                pending.push(Read::Start(Kind::K174, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K194, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 3));
                                pending.push(Read::Start(Kind::K174, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 3));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K131, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K175 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K187, depth + 1));
                        pending.push(Read::Start(Kind::K185, depth + 1));
                        pending.push(Read::Start(Kind::K191, depth + 1));
                        pending.push(Read::Start(Kind::K180, depth + 1));
                    }
                    Kind::K176 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 4));
                        pending.push(Read::Start(Kind::K187, depth + 1));
                        pending.push(Read::Start(Kind::K185, depth + 1));
                        pending.push(Read::Start(Kind::K183, depth + 1));
                        pending.push(Read::Start(Kind::K180, depth + 1));
                    }
                    Kind::K177 => {
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
                    Kind::K178 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K198, depth + 1));
                        pending.push(Read::Start(Kind::K197, depth + 1));
                        pending.push(Read::Start(Kind::K131, depth + 1));
                    }
                    Kind::K179 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K171, depth));
                        }
                    }
                    Kind::K180 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 1));
                        pending.push(Read::Start(Kind::K181, depth + 1));
                    }
                    Kind::K181 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K182, depth));
                        }
                    }
                    Kind::K182 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
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
                        pending.push(Read::Start(Kind::K171, depth + 1));
                        pending.push(Read::Start(Kind::K52, depth + 1));
                        pending.push(Read::Start(Kind::K189, depth + 1));
                    }
                    Kind::K185 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K186, depth));
                        }
                    }
                    Kind::K186 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 3));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K180, depth + 1));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K187 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K188, depth));
                        }
                    }
                    Kind::K188 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 4));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K74, depth + 1));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 3));
                                pending.push(Read::Start(Kind::K180, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K157, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K189 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K190, depth));
                        }
                    }
                    Kind::K190 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K243, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K191 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K192, depth));
                        }
                    }
                    Kind::K192 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 3));
                                pending.push(Read::Start(Kind::K171, depth + 1));
                                pending.push(Read::Start(Kind::K52, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K180, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K193 => {
                        let _ = depth;
                        values.push(Value::K193(Box::new(self.unary()?)));
                    }
                    Kind::K194 => {
                        let _ = depth;
                        values.push(Value::K194(Box::new(self.binary()?)));
                    }
                    Kind::K195 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K196, depth));
                        }
                    }
                    Kind::K196 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K23, depth + 1));
                        pending.push(Read::Start(Kind::K130, depth + 1));
                    }
                    Kind::K197 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 0));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K175, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K198 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K199, depth));
                        }
                    }
                    Kind::K199 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K200, depth + 1));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K131, depth + 1));
                    }
                    Kind::K200 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 2));
                                pending.push(Read::Start(Kind::K174, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K201 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K130, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K202 => {
                        let _ = depth;
                        values.push(Value::K202(Box::new(self.span()?)));
                    }
                    Kind::K203 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K202, depth + 1));
                        pending.push(Read::Start(Kind::K204, depth + 1));
                    }
                    Kind::K204 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 0));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K205, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 2));
                                pending.push(Read::Start(Kind::K206, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 2));
                                pending.push(Read::Start(Kind::K203, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 2));
                                pending.push(Read::Start(Kind::K203, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 3));
                                pending.push(Read::Start(Kind::K205, depth + 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 3));
                                pending.push(Read::Start(Kind::K205, depth + 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                                pending.push(Read::Start(Kind::K96, depth + 1));
                            }
                            10 => {
                                pending.push(Read::Finish(kind, 10, 2));
                                pending.push(Read::Start(Kind::K206, depth + 1));
                                pending.push(Read::Start(Kind::K96, depth + 1));
                            }
                            11 => {
                                pending.push(Read::Finish(kind, 11, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            12 => {
                                pending.push(Read::Finish(kind, 12, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            13 => {
                                pending.push(Read::Finish(kind, 13, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K205 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K203, depth));
                        }
                    }
                    Kind::K206 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K207, depth));
                        }
                    }
                    Kind::K207 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K203, depth + 1));
                        pending.push(Read::Start(Kind::K124, depth + 1));
                    }
                    Kind::K208 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K202, depth + 1));
                        pending.push(Read::Start(Kind::K121, depth + 1));
                        pending.push(Read::Start(Kind::K203, depth + 1));
                    }
                    Kind::K209 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K210, depth));
                        }
                    }
                    Kind::K210 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K202, depth + 1));
                        pending.push(Read::Start(Kind::K211, depth + 1));
                        pending.push(Read::Start(Kind::K56, depth + 1));
                    }
                    Kind::K211 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 4));
                                pending.push(Read::Start(Kind::K212, depth + 1));
                                pending.push(Read::Start(Kind::K201, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 4));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K201, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                                pending.push(Read::Start(Kind::K203, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 2));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 1));
                                pending.push(Read::Start(Kind::K212, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 3));
                                pending.push(Read::Start(Kind::K214, depth + 1));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 4));
                                pending.push(Read::Start(Kind::K214, depth + 1));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K203, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 2));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 3));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K203, depth + 1));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 4));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            10 => {
                                pending.push(Read::Finish(kind, 10, 2));
                                pending.push(Read::Start(Kind::K215, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            11 => {
                                pending.push(Read::Finish(kind, 11, 1));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                            }
                            12 => {
                                pending.push(Read::Finish(kind, 12, 1));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                            }
                            13 => {
                                pending.push(Read::Finish(kind, 13, 1));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                            }
                            14 => {
                                pending.push(Read::Finish(kind, 14, 0));
                            }
                            15 => {
                                pending.push(Read::Finish(kind, 15, 0));
                            }
                            16 => {
                                pending.push(Read::Finish(kind, 16, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            17 => {
                                pending.push(Read::Finish(kind, 17, 3));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K203, depth + 1));
                            }
                            18 => {
                                pending.push(Read::Finish(kind, 18, 3));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K194, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            19 => {
                                pending.push(Read::Finish(kind, 19, 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            20 => {
                                pending.push(Read::Finish(kind, 20, 1));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                            }
                            21 => {
                                pending.push(Read::Finish(kind, 21, 3));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            22 => {
                                pending.push(Read::Finish(kind, 22, 1));
                                pending.push(Read::Start(Kind::K210, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K212 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K213, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K213 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K202, depth + 1));
                        pending.push(Read::Start(Kind::K216, depth + 1));
                        pending.push(Read::Start(Kind::K56, depth + 1));
                    }
                    Kind::K214 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K121, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K215 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K208, depth));
                        }
                    }
                    Kind::K216 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 4));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K218, depth + 1));
                                pending.push(Read::Start(Kind::K217, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 4));
                                pending.push(Read::Start(Kind::K219, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K218, depth + 1));
                                pending.push(Read::Start(Kind::K217, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 4));
                                pending.push(Read::Start(Kind::K220, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                                pending.push(Read::Start(Kind::K217, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            3 => {
                                pending.push(Read::Finish(kind, 3, 1));
                                pending.push(Read::Start(Kind::K76, depth + 1));
                            }
                            4 => {
                                pending.push(Read::Finish(kind, 4, 2));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            5 => {
                                pending.push(Read::Finish(kind, 5, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            6 => {
                                pending.push(Read::Finish(kind, 6, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            7 => {
                                pending.push(Read::Finish(kind, 7, 2));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            8 => {
                                pending.push(Read::Finish(kind, 8, 2));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            9 => {
                                pending.push(Read::Finish(kind, 9, 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            10 => {
                                pending.push(Read::Finish(kind, 10, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            11 => {
                                pending.push(Read::Finish(kind, 11, 0));
                            }
                            12 => {
                                pending.push(Read::Finish(kind, 12, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            13 => {
                                pending.push(Read::Finish(kind, 13, 2));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K193, depth + 1));
                            }
                            14 => {
                                pending.push(Read::Finish(kind, 14, 3));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K194, depth + 1));
                            }
                            15 => {
                                pending.push(Read::Finish(kind, 15, 2));
                                pending.push(Read::Start(Kind::K137, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            16 => {
                                pending.push(Read::Finish(kind, 16, 2));
                                pending.push(Read::Start(Kind::K220, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            17 => {
                                pending.push(Read::Finish(kind, 17, 1));
                                pending.push(Read::Start(Kind::K220, depth + 1));
                            }
                            18 => {
                                pending.push(Read::Finish(kind, 18, 1));
                                pending.push(Read::Start(Kind::K220, depth + 1));
                            }
                            19 => {
                                pending.push(Read::Finish(kind, 19, 2));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            20 => {
                                pending.push(Read::Finish(kind, 20, 3));
                                pending.push(Read::Start(Kind::K221, depth + 1));
                                pending.push(Read::Start(Kind::K221, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            21 => {
                                pending.push(Read::Finish(kind, 21, 3));
                                pending.push(Read::Start(Kind::K201, depth + 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            22 => {
                                pending.push(Read::Finish(kind, 22, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            23 => {
                                pending.push(Read::Finish(kind, 23, 2));
                                pending.push(Read::Start(Kind::K222, depth + 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            24 => {
                                pending.push(Read::Finish(kind, 24, 3));
                                pending.push(Read::Start(Kind::K222, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K201, depth + 1));
                            }
                            25 => {
                                pending.push(Read::Finish(kind, 25, 2));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K222, depth + 1));
                            }
                            26 => {
                                pending.push(Read::Finish(kind, 26, 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            27 => {
                                pending.push(Read::Finish(kind, 27, 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            28 => {
                                pending.push(Read::Finish(kind, 28, 4));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                                pending.push(Read::Start(Kind::K201, depth + 1));
                                pending.push(Read::Start(Kind::K113, depth + 1));
                                pending.push(Read::Start(Kind::K33, depth + 1));
                            }
                            29 => {
                                pending.push(Read::Finish(kind, 29, 3));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            30 => {
                                pending.push(Read::Finish(kind, 30, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            31 => {
                                pending.push(Read::Finish(kind, 31, 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            32 => {
                                pending.push(Read::Finish(kind, 32, 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            33 => {
                                pending.push(Read::Finish(kind, 33, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            34 => {
                                pending.push(Read::Finish(kind, 34, 2));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            35 => {
                                pending.push(Read::Finish(kind, 35, 2));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            36 => {
                                pending.push(Read::Finish(kind, 36, 2));
                                pending.push(Read::Start(Kind::K222, depth + 1));
                                pending.push(Read::Start(Kind::K96, depth + 1));
                            }
                            37 => {
                                pending.push(Read::Finish(kind, 37, 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            38 => {
                                pending.push(Read::Finish(kind, 38, 1));
                                pending.push(Read::Start(Kind::K121, depth + 1));
                            }
                            39 => {
                                pending.push(Read::Finish(kind, 39, 2));
                                pending.push(Read::Start(Kind::K225, depth + 1));
                                pending.push(Read::Start(Kind::K224, depth + 1));
                            }
                            40 => {
                                pending.push(Read::Finish(kind, 40, 2));
                                pending.push(Read::Start(Kind::K226, depth + 1));
                                pending.push(Read::Start(Kind::K126, depth + 1));
                            }
                            41 => {
                                pending.push(Read::Finish(kind, 41, 2));
                                pending.push(Read::Start(Kind::K227, depth + 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K217 => {
                        let _ = depth;
                        values.push(Value::K217(Box::new(self.dyn_interface_id()?)));
                    }
                    Kind::K218 => {
                        let _ = depth;
                        values.push(Value::K218(Box::new(self.dyn_vtable_id()?)));
                    }
                    Kind::K219 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 0));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K220 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K213, depth));
                        }
                    }
                    Kind::K221 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K213, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K222 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K223, depth));
                        }
                    }
                    Kind::K223 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K213, depth + 1));
                        pending.push(Read::Start(Kind::K124, depth + 1));
                    }
                    Kind::K224 => {
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
                    Kind::K225 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K232, depth + 1));
                        pending.push(Read::Start(Kind::K231, depth + 1));
                        pending.push(Read::Start(Kind::K226, depth + 1));
                    }
                    Kind::K226 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K202, depth + 1));
                        pending.push(Read::Start(Kind::K237, depth + 1));
                        pending.push(Read::Start(Kind::K234, depth + 1));
                    }
                    Kind::K227 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K228, depth));
                        }
                    }
                    Kind::K228 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 1));
                                pending.push(Read::Start(Kind::K130, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K213, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K229 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 3));
                        pending.push(Read::Start(Kind::K202, depth + 1));
                        pending.push(Read::Start(Kind::K213, depth + 1));
                        pending.push(Read::Start(Kind::K230, depth + 1));
                    }
                    Kind::K230 => {
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
                                pending.push(Read::Finish(kind, 4, 2));
                                pending.push(Read::Start(Kind::K236, depth + 1));
                                pending.push(Read::Start(Kind::K235, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K231 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K229, depth));
                        }
                    }
                    Kind::K232 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K233, depth));
                        }
                    }
                    Kind::K233 => {
                        self.dyn_recipe_node(depth)?;
                        pending.push(Read::Finish(kind, 0, 2));
                        pending.push(Read::Start(Kind::K6, depth + 1));
                        pending.push(Read::Start(Kind::K202, depth + 1));
                    }
                    Kind::K234 => {
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
                    Kind::K235 => {
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
                    Kind::K236 => {
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
                    Kind::K237 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K238, depth));
                        }
                    }
                    Kind::K238 => {
                        self.dyn_recipe_node(depth)?;
                        match self.u8()? {
                            0 => {
                                pending.push(Read::Finish(kind, 0, 2));
                                pending.push(Read::Start(Kind::K202, depth + 1));
                                pending.push(Read::Start(Kind::K124, depth + 1));
                            }
                            1 => {
                                pending.push(Read::Finish(kind, 1, 1));
                                pending.push(Read::Start(Kind::K226, depth + 1));
                            }
                            2 => {
                                pending.push(Read::Finish(kind, 2, 2));
                                pending.push(Read::Start(Kind::K202, depth + 1));
                                pending.push(Read::Start(Kind::K6, depth + 1));
                            }
                            _ => return Err(malformed("E-WIRE-TAG", "unknown dynamic recipe tag")),
                        }
                    }
                    Kind::K239 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K240, depth));
                        }
                    }
                    Kind::K240 => {
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
                    Kind::K241 => {
                        if self.boolean()? {
                            pending.push(Read::Finish(kind, 0, 1));
                            pending.push(Read::Start(Kind::K14, depth));
                        } else {
                            values.push(Value::Optional(None));
                        }
                    }
                    Kind::K242 => {
                        let count = self.count(self.limits.max_nodes, 1)?;
                        pending.push(Read::Finish(kind, 0, count));
                        for _ in 0..count {
                            pending.push(Read::Start(Kind::K16, depth));
                        }
                    }
                    Kind::K243 => {
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
                                    Value::K130(value) => *value,
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
                                            Value::K240(value) => *value,
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
                                    Value::K193(value) => *value,
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
                                    Value::K194(value) => *value,
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
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            8 => DynConstPattern::FloatLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
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
                                    Value::K138(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                lifetime_instantiation: match fields.next().expect("recipe field") {
                                    Value::K139(value) => *value,
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
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                header_embedding: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K144(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                environment: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K146(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                self_type: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K148(value) => *value,
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
                                    Value::K129(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                at: match fields.next().expect("recipe field") {
                                    Value::K163(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                action: match fields.next().expect("recipe field") {
                                    Value::K164(value) => *value,
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
                                    Value::K130(value) => *value,
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
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                bound: match fields.next().expect("recipe field") {
                                    Value::K17(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticRequirementRecipe::Trait {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                application: match fields.next().expect("recipe field") {
                                    Value::K131(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticRequirementRecipe::Callable {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
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
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                right: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticRequirementRecipe::ConstEqual {
                                carrier: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                left: match fields.next().expect("recipe field") {
                                    Value::K174(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                right: match fields.next().expect("recipe field") {
                                    Value::K174(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticRequirementRecipe::LifeEqual {
                                left: match fields.next().expect("recipe field") {
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                right: match fields.next().expect("recipe field") {
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            6 => SemanticRequirementRecipe::Outlives {
                                longer: match fields.next().expect("recipe field") {
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                shorter: match fields.next().expect("recipe field") {
                                    Value::K171(value) => *value,
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
                            8 => SemanticRequirementRecipe::Builtin {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                trait_kind: match fields.next().expect("recipe field") {
                                    Value::K132(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
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
                                    Value::K128(value) => *value,
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
                            3 => SemanticDefinitionBody::FunctionWithBody {
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
                                parameters: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K114(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                receiver_parameter: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K6(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                names: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K117(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                site_nodes: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K119(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K120(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
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
                                            Value::K196(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                result: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
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
                                    Value::K129(value) => *value,
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
                                    Value::K161(value) => *value,
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
                                            Value::K134(value) => *value,
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
                                            Value::K136(value) => *value,
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
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticScopeRecipe::TraitImpl {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                self_type: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                trait_application: match fields.next().expect("recipe field") {
                                    Value::K131(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => SemanticScopeRecipe::BuiltinImpl {
                                self_type: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                trait_kind: match fields.next().expect("recipe field") {
                                    Value::K132(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K113 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K114 => {
                            let _ = tag;
                            Value::K114(Box::new(SemanticOriginalClosureParameter {
                                name: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K130(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K23(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K115 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K116 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K117 => {
                            let _ = tag;
                            Value::K117(Box::new(SemanticOriginalNameBinding {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K124(value) => value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                namespace: match fields.next().expect("recipe field") {
                                    Value::K125(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K127(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K118 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K119 => {
                            let _ = tag;
                            Value::K119(Box::new(SemanticOriginalSiteNode {
                                site: match fields.next().expect("recipe field") {
                                    Value::K129(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                node: match fields.next().expect("recipe field") {
                                    Value::K123(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K120 => Value::K120(Box::new(match tag {
                            0 => SemanticOriginalFunctionBody::Defined(
                                match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticOriginalFunctionBody::External,
                            2 => SemanticOriginalFunctionBody::Declaration,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K121 => {
                            let _ = tag;
                            Value::K121(Box::new(SemanticOriginalBlock {
                                statements: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K210(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K122 => Value::K122(Box::new(match tag {
                            0 => SemanticOriginalTypeSource::Expression(
                                match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticOriginalTypeSource::Binding(
                                match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => SemanticOriginalTypeSource::Parameter(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => SemanticOriginalTypeSource::Result,
                            4 => SemanticOriginalTypeSource::CallConstCarrier {
                                call: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                formal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticOriginalTypeSource::BindingProjection {
                                statement: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                pattern: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K6(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            6 => SemanticOriginalTypeSource::NameAt {
                                at: match fields.next().expect("recipe field") {
                                    Value::K123(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                name: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K123 => Value::K123(Box::new(match tag {
                            0 => SemanticOriginalNode::Expression(
                                match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticOriginalNode::Statement(
                                match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K125 => Value::K125(Box::new(match tag {
                            0 => SemanticOriginalNamespace::Value,
                            1 => SemanticOriginalNamespace::Type,
                            2 => SemanticOriginalNamespace::Lifetime,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K126 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K127 => Value::K127(Box::new(match tag {
                            0 => SemanticOriginalNameValue::Recipe(
                                match fields.next().expect("recipe field") {
                                    Value::K128(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticOriginalNameValue::Definition(
                                match fields.next().expect("recipe field") {
                                    Value::K84(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K128 => Value::K128(Box::new(match tag {
                            0 => SemanticActualRecipe::Type(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticActualRecipe::Const {
                                carrier: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K174(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticActualRecipe::Life(
                                match fields.next().expect("recipe field") {
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => SemanticActualRecipe::Infer,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K130 => Value::K130(Box::new(match tag {
                            0 => SemanticTypeRecipe::Atom(
                                match fields.next().expect("recipe field") {
                                    Value::K1(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticTypeRecipe::Exact(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K173(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            2 => SemanticTypeRecipe::Formal(
                                match fields.next().expect("recipe field") {
                                    Value::K170(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => SemanticTypeRecipe::Environment {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticTypeRecipe::SelfType {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
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
                                            Value::K128(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            6 => SemanticTypeRecipe::AssociatedType {
                                receiver: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                application: Box::new(match fields.next().expect("recipe field") {
                                    Value::K131(value) => *value,
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
                                            Value::K130(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            ),
                            8 => SemanticTypeRecipe::Array {
                                element: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                length: Box::new(match fields.next().expect("recipe field") {
                                    Value::K174(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            9 => SemanticTypeRecipe::Slice(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            10 => SemanticTypeRecipe::Address {
                                mutable: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                lifetime: match fields.next().expect("recipe field") {
                                    Value::K172(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                inner: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            11 => SemanticTypeRecipe::Desc {
                                lifetime: match fields.next().expect("recipe field") {
                                    Value::K172(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                inner: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            12 => SemanticTypeRecipe::Managed(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            13 => SemanticTypeRecipe::OwnedStore(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            14 => SemanticTypeRecipe::Mutex(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            15 => SemanticTypeRecipe::MutexGuard {
                                payload: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                retained: Box::new(match fields.next().expect("recipe field") {
                                    Value::K175(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            16 => SemanticTypeRecipe::Registration(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            17 => SemanticTypeRecipe::AsyncRegistration(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            18 => SemanticTypeRecipe::Task {
                                result: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                retained: Box::new(match fields.next().expect("recipe field") {
                                    Value::K175(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            19 => SemanticTypeRecipe::ErrorUnion {
                                error: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                result: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            20 => SemanticTypeRecipe::Qualified {
                                storage: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                life: Box::new(match fields.next().expect("recipe field") {
                                    Value::K176(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            21 => SemanticTypeRecipe::Code {
                                kind: match fields.next().expect("recipe field") {
                                    Value::K177(value) => *value,
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
                                            Value::K128(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                environment: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K128(value) => *value,
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
                                    Value::K178(value) => *value,
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
                                            Value::K128(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                hidden: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            25 => SemanticTypeRecipe::AnonymousSum(
                                match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K130(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            ),
                            26 => SemanticTypeRecipe::UnresolvedNamed(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            27 => SemanticTypeRecipe::Unsupported(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            28 => SemanticTypeRecipe::OriginalType {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: match fields.next().expect("recipe field") {
                                    Value::K122(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K131 => {
                            let _ = tag;
                            Value::K131(Box::new(SemanticTraitRecipe {
                                declaration: match fields.next().expect("recipe field") {
                                    Value::K74(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K128(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K132 => Value::K132(Box::new(match tag {
                            0 => SemanticBuiltinTrait::Drop,
                            1 => SemanticBuiltinTrait::Copy,
                            2 => SemanticBuiltinTrait::Value,
                            3 => SemanticBuiltinTrait::SyncDrop,
                            4 => SemanticBuiltinTrait::StoreElement,
                            5 => SemanticBuiltinTrait::Transfer,
                            6 => SemanticBuiltinTrait::Share,
                            7 => SemanticBuiltinTrait::ThreadCallable,
                            8 => SemanticBuiltinTrait::ClosedEnvironment,
                            9 => SemanticBuiltinTrait::Send,
                            10 => SemanticBuiltinTrait::Sync,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K133 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K134 => {
                            let _ = tag;
                            Value::K134(Box::new(SemanticFieldRecipe {
                                name: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K135 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K136 => {
                            let _ = tag;
                            Value::K136(Box::new(SemanticVariantRecipe {
                                name: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K130(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K137 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K138 => Value::K138(Box::new(match tag {
                            0 => SemanticInstanceTarget::Function(
                                match fields.next().expect("recipe field") {
                                    Value::K149(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticInstanceTarget::Scope,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K139 => Value::K139(Box::new(match tag {
                            0 => SemanticLifetimeInstantiation::Rigid,
                            1 => SemanticLifetimeInstantiation::Partition {
                                rigid: match fields.next().expect("recipe field") {
                                    Value::K85(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K140 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K141 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K142 => Value::K142(Box::new(match tag {
                            0 => SemanticActual::Type(match fields.next().expect("recipe field") {
                                Value::K148(value) => *value,
                                _ => unreachable!("recipe field grammar"),
                            }),
                            1 => SemanticActual::Const {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K148(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K76(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticActual::Life(match fields.next().expect("recipe field") {
                                Value::K152(value) => *value,
                                _ => unreachable!("recipe field grammar"),
                            }),
                            3 => SemanticActual::CoreRecipe {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K85(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                recipe: Box::new(match fields.next().expect("recipe field") {
                                    Value::K128(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            4 => SemanticActual::CoreInferred {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K85(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                call: match fields.next().expect("recipe field") {
                                    Value::K129(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                formal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K143 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K144 => {
                            let _ = tag;
                            Value::K144(Box::new(SemanticHeaderBinding {
                                header: match fields.next().expect("recipe field") {
                                    Value::K4(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actual: match fields.next().expect("recipe field") {
                                    Value::K152(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K145 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K146 => {
                            let _ = tag;
                            Value::K146(Box::new(SemanticEnvironmentInstance {
                                closure: match fields.next().expect("recipe field") {
                                    Value::K150(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actuals: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K142(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K147 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K148 => Value::K148(Box::new(match tag {
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
                                            Value::K152(value) => *value,
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
                                    Value::K153(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K149 => Value::K149(Box::new(match tag {
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
                        Kind::K150 => Value::K150(Box::new(match tag {
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
                                    Value::K151(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K152 => Value::K152(Box::new(match tag {
                            0 => SemanticClassExpr::Formal {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticClassExpr::Implicit {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
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
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K158(value) => *value,
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
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K158(value) => *value,
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
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K153 => Value::K153(Box::new(match tag {
                            0 => SemanticCoreTypeSource::OriginalRecipe(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
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
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            2 => SemanticCoreTypeSource::OriginalExpression {
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
                                source: match fields.next().expect("recipe field") {
                                    Value::K122(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K154 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K155 => Value::K155(Box::new(match tag {
                            0 => SemanticScopeRef::Ancestor(
                                match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K156 => Value::K156(Box::new(match tag {
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
                                    Value::K159(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            4 => {
                                SemanticOperand::Local(match fields.next().expect("recipe field") {
                                    Value::K160(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            5 => SemanticOperand::TypeArgument {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
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
                        Kind::K157 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K158 => Value::K158(Box::new(match tag {
                            0 => LifeTypeProjection::Place(
                                match fields.next().expect("recipe field") {
                                    Value::K190(value) => *value,
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
                        Kind::K161 => Value::K161(Box::new(match tag {
                            0 => SemanticSiteRecipeKind::Construct {
                                result: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
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
                                            Value::K128(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            2 => SemanticSiteRecipeKind::CallableCall {
                                callee: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => SemanticSiteRecipeKind::TypeUse {
                                expected: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticSiteRecipeKind::DeclareLocal {
                                expected: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
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
                                            Value::K128(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            6 => SemanticSiteRecipeKind::OriginalCall,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K162 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K163 => Value::K163(Box::new(match tag {
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
                                    Value::K165(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticSiteAddress::Typed {
                                block: match fields.next().expect("recipe field") {
                                    Value::K166(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                instruction: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                edge: match fields.next().expect("recipe field") {
                                    Value::K167(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K164 => Value::K164(Box::new(match tag {
                            0 => SemanticSiteAction::Construct {
                                result: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K148(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticSiteAction::Call {
                                target: match fields.next().expect("recipe field") {
                                    Value::K149(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                actuals: match fields.next().expect("recipe field") {
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
                            2 => SemanticSiteAction::CallableCall {
                                callee: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => SemanticSiteAction::TypeUse {
                                subject: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                expected: match fields.next().expect("recipe field") {
                                    Value::K148(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            4 => SemanticSiteAction::DeclareLocal {
                                local: match fields.next().expect("recipe field") {
                                    Value::K168(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                expected: match fields.next().expect("recipe field") {
                                    Value::K148(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticSiteAction::Closure {
                                result: match fields.next().expect("recipe field") {
                                    Value::K156(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                closure: match fields.next().expect("recipe field") {
                                    Value::K150(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                environment: match fields.next().expect("recipe field") {
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
                            6 => SemanticSiteAction::CoreBodyCall,
                            7 => SemanticSiteAction::BodyCall {
                                actuals: match fields.next().expect("recipe field") {
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
                        Kind::K165 => Value::K165(Box::new(match tag {
                            0 => SemanticEmissionRole::Construct,
                            1 => SemanticEmissionRole::Call,
                            2 => SemanticEmissionRole::Ascription,
                            3 => SemanticEmissionRole::Binding,
                            4 => SemanticEmissionRole::Closure,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K167 => Value::K167(Box::new(match tag {
                            0 => SemanticSiteEdge::Before,
                            1 => SemanticSiteEdge::After,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K168 => Value::K168(Box::new(match tag {
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
                                    Value::K160(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K169 => Value::K169(Box::new(match tag {
                            0 => SemanticRecipeOperand::TypeFormal(
                                match fields.next().expect("recipe field") {
                                    Value::K170(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticRecipeOperand::Environment {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
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
                        Kind::K170 => {
                            let _ = tag;
                            Value::K170(Box::new(SemanticFormalRef {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K171 => Value::K171(Box::new(match tag {
                            0 => SemanticLifeRecipe::Formal(
                                match fields.next().expect("recipe field") {
                                    Value::K170(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticLifeRecipe::Implicit {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticLifeRecipe::Environment {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
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
                                    Value::K169(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K158(value) => *value,
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
                                    Value::K169(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K158(value) => *value,
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
                        Kind::K172 => Value::K172(Box::new(match tag {
                            0 => SemanticOuterLifeRecipe::Class(
                                match fields.next().expect("recipe field") {
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticOuterLifeRecipe::Static,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K173 => {
                            let _ = tag;
                            Value::K173(Box::new(SemanticExactTypeRecipe {
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
                                            Value::K171(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K174 => Value::K174(Box::new(match tag {
                            0 => SemanticConstRecipe::Value(
                                match fields.next().expect("recipe field") {
                                    Value::K76(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticConstRecipe::IntegerLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => SemanticConstRecipe::FloatLiteral(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            3 => SemanticConstRecipe::Formal(
                                match fields.next().expect("recipe field") {
                                    Value::K170(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            4 => SemanticConstRecipe::Environment {
                                scope: match fields.next().expect("recipe field") {
                                    Value::K155(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ordinal: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            5 => SemanticConstRecipe::Unary {
                                op: match fields.next().expect("recipe field") {
                                    Value::K193(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                operand_type: Box::new(
                                    match fields.next().expect("recipe field") {
                                        Value::K130(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    },
                                ),
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K174(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            6 => SemanticConstRecipe::Binary {
                                op: match fields.next().expect("recipe field") {
                                    Value::K194(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                operand_type: Box::new(
                                    match fields.next().expect("recipe field") {
                                        Value::K130(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    },
                                ),
                                left: Box::new(match fields.next().expect("recipe field") {
                                    Value::K174(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                right: Box::new(match fields.next().expect("recipe field") {
                                    Value::K174(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            7 => SemanticConstRecipe::Convert {
                                source_type: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                target_type: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K174(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            8 => SemanticConstRecipe::AssociatedConst {
                                receiver: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                application: Box::new(match fields.next().expect("recipe field") {
                                    Value::K131(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K175 => {
                            let _ = tag;
                            Value::K175(Box::new(SemanticRetainedShapeRecipe {
                                domain: match fields.next().expect("recipe field") {
                                    Value::K180(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ports: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K192(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                retained: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K186(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                nominal: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K188(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K176 => {
                            let _ = tag;
                            Value::K176(Box::new(SemanticStorageShapeRecipe {
                                domain: match fields.next().expect("recipe field") {
                                    Value::K180(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                slots: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K184(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                retained: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K186(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                nominal: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K188(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K177 => Value::K177(Box::new(match tag {
                            0 => SemanticCodeKind::FunctionPointer,
                            1 => SemanticCodeKind::BorrowedFunctionPointer,
                            2 => SemanticCodeKind::ForeignCallback,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K178 => {
                            let _ = tag;
                            Value::K178(Box::new(SemanticDynInterfaceRecipe {
                                application: match fields.next().expect("recipe field") {
                                    Value::K131(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                context: match fields.next().expect("recipe field") {
                                    Value::K197(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                associated: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K199(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K179 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K180 => {
                            let _ = tag;
                            Value::K180(Box::new(SemanticClassSequenceRecipe {
                                parts: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K182(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K181 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K182 => Value::K182(Box::new(match tag {
                            0 => SemanticClassSequencePart::One(
                                match fields.next().expect("recipe field") {
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticClassSequencePart::AllClasses {
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K158(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K183 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K184 => {
                            let _ = tag;
                            Value::K184(Box::new(SemanticSlotRecipe {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K190(value) => *value,
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
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K185 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K186 => Value::K186(Box::new(match tag {
                            0 => SemanticRetainedPart::One {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K158(value) => *value,
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
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticRetainedPart::All {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K158(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                parents: match fields.next().expect("recipe field") {
                                    Value::K180(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K187 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K188 => Value::K188(Box::new(match tag {
                            0 => SemanticNominalPart::One {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K158(value) => *value,
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
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticNominalPart::FromType {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K158(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                classes: match fields.next().expect("recipe field") {
                                    Value::K180(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K189 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K190 => Value::K190(Box::new(match tag {
                            0 => {
                                PlaceProjection::Field(match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                })
                            }
                            1 => PlaceProjection::Variant(
                                match fields.next().expect("recipe field") {
                                    Value::K243(value) => *value,
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
                        Kind::K191 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K192 => Value::K192(Box::new(match tag {
                            0 => SemanticPortPart::One {
                                leaf: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                mode: match fields.next().expect("recipe field") {
                                    Value::K52(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                class: match fields.next().expect("recipe field") {
                                    Value::K171(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticPortPart::FromType {
                                ty: Box::new(match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                classes: match fields.next().expect("recipe field") {
                                    Value::K180(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K195 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K196 => {
                            let _ = tag;
                            Value::K196(Box::new(SemanticParameterRecipe {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                mode: match fields.next().expect("recipe field") {
                                    Value::K23(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K197 => Value::K197(Box::new(match tag {
                            0 => SemanticDynContextRecipe::Closed,
                            1 => SemanticDynContextRecipe::Type(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            2 => SemanticDynContextRecipe::Shape(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K175(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K198 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K199 => {
                            let _ = tag;
                            Value::K199(Box::new(SemanticAssociatedBindingRecipe {
                                application: match fields.next().expect("recipe field") {
                                    Value::K131(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                member: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K200(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K200 => Value::K200(Box::new(match tag {
                            0 => SemanticAssociatedRecipe::Type(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticAssociatedRecipe::Const {
                                carrier: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K174(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K201 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K203 => {
                            let _ = tag;
                            Value::K203(Box::new(SemanticOriginalPattern {
                                kind: match fields.next().expect("recipe field") {
                                    Value::K204(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K204 => Value::K204(Box::new(match tag {
                            0 => SemanticOriginalPatternKind::Name(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticOriginalPatternKind::Wildcard,
                            2 => SemanticOriginalPatternKind::Tuple(
                                match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K203(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            ),
                            3 => SemanticOriginalPatternKind::Record {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::Tuple(rows) => {
                                                let mut fields = rows.into_iter();
                                                (
                                                    match fields.next().expect("tuple field") {
                                                        Value::K124(value) => value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                    match fields.next().expect("tuple field") {
                                                        Value::K203(value) => *value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                )
                                            }
                                            _ => unreachable!("recipe tuple grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            4 => SemanticOriginalPatternKind::Reference {
                                mutable: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                pattern: Box::new(match fields.next().expect("recipe field") {
                                    Value::K203(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            5 => SemanticOriginalPatternKind::ErrorArm {
                                is_error: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                pattern: Box::new(match fields.next().expect("recipe field") {
                                    Value::K203(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            6 => SemanticOriginalPatternKind::Variant {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                variant: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K203(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            7 => SemanticOriginalPatternKind::Integer(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            8 => SemanticOriginalPatternKind::Bool(
                                match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            9 => SemanticOriginalPatternKind::Constructor {
                                definition: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K84(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                variant: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K203(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            10 => SemanticOriginalPatternKind::RecordConstructor {
                                definition: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K84(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::Tuple(rows) => {
                                                let mut fields = rows.into_iter();
                                                (
                                                    match fields.next().expect("tuple field") {
                                                        Value::K124(value) => value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                    match fields.next().expect("tuple field") {
                                                        Value::K203(value) => *value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                )
                                            }
                                            _ => unreachable!("recipe tuple grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            11 => SemanticOriginalPatternKind::Float(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            12 => SemanticOriginalPatternKind::String(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            13 => SemanticOriginalPatternKind::Character(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K205 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K206 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K207 => {
                            let _ = tag;
                            Value::Tuple(fields.collect())
                        }
                        Kind::K208 => {
                            let _ = tag;
                            Value::K208(Box::new(SemanticOriginalMatchArm {
                                pattern: match fields.next().expect("recipe field") {
                                    Value::K203(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K209 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K210 => {
                            let _ = tag;
                            Value::K210(Box::new(SemanticOriginalStatement {
                                id: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                kind: match fields.next().expect("recipe field") {
                                    Value::K211(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K211 => Value::K211(Box::new(match tag {
                            0 => SemanticOriginalStatementKind::Binding {
                                name: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                mutable: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K130(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                initializer: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K213(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            },
                            1 => SemanticOriginalStatementKind::TupleBinding {
                                pattern: match fields.next().expect("recipe field") {
                                    Value::K203(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                mutable: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                ty: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K130(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                initializer: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticOriginalStatementKind::Assign {
                                target: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            3 => SemanticOriginalStatementKind::Expression(
                                match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            4 => SemanticOriginalStatementKind::Return(
                                match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K213(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            ),
                            5 => SemanticOriginalStatementKind::If {
                                condition: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                then_body: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                else_body: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K121(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            },
                            6 => SemanticOriginalStatementKind::IfLet {
                                pattern: match fields.next().expect("recipe field") {
                                    Value::K203(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                then_body: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                else_body: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K121(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            },
                            7 => SemanticOriginalStatementKind::While {
                                condition: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            8 => SemanticOriginalStatementKind::WhileLet {
                                pattern: match fields.next().expect("recipe field") {
                                    Value::K203(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            9 => SemanticOriginalStatementKind::ForRange {
                                name: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                start: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                end: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            10 => SemanticOriginalStatementKind::Match {
                                value: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arms: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K208(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            11 => SemanticOriginalStatementKind::Block(
                                match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            12 => SemanticOriginalStatementKind::Defer(
                                match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            13 => SemanticOriginalStatementKind::Unsafe(
                                match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            14 => SemanticOriginalStatementKind::Break,
                            15 => SemanticOriginalStatementKind::Continue,
                            16 => SemanticOriginalStatementKind::Unsupported(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            17 => SemanticOriginalStatementKind::ForEach {
                                pattern: match fields.next().expect("recipe field") {
                                    Value::K203(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                iterator: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            18 => SemanticOriginalStatementKind::CompoundAssign {
                                target: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                op: match fields.next().expect("recipe field") {
                                    Value::K194(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            19 => SemanticOriginalStatementKind::Yield(
                                match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            20 => SemanticOriginalStatementKind::Loop(
                                match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            21 => SemanticOriginalStatementKind::With {
                                name: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            22 => SemanticOriginalStatementKind::CompileTime(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K210(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K212 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K213 => {
                            let _ = tag;
                            Value::K213(Box::new(SemanticOriginalExpr {
                                id: match fields.next().expect("recipe field") {
                                    Value::K56(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                kind: match fields.next().expect("recipe field") {
                                    Value::K216(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K214 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K215 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K216 => Value::K216(Box::new(match tag {
                            0 => SemanticOriginalExprKind::MakeDynBorrow {
                                interface: match fields.next().expect("recipe field") {
                                    Value::K217(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                vtable: match fields.next().expect("recipe field") {
                                    Value::K218(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                mutable: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticOriginalExprKind::MakeDynOwned {
                                interface: match fields.next().expect("recipe field") {
                                    Value::K217(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                vtable: match fields.next().expect("recipe field") {
                                    Value::K218(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                source: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                route: match fields.next().expect("recipe field") {
                                    Value::K219(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            2 => SemanticOriginalExprKind::CallDynSlot {
                                callee: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                interface: match fields.next().expect("recipe field") {
                                    Value::K217(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                slot: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K213(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            3 => SemanticOriginalExprKind::Literal(
                                match fields.next().expect("recipe field") {
                                    Value::K76(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            4 => SemanticOriginalExprKind::Ascribed {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            5 => SemanticOriginalExprKind::Integer(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            6 => SemanticOriginalExprKind::Float(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            7 => SemanticOriginalExprKind::NumericConvert {
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                target: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            8 => SemanticOriginalExprKind::IntegerConvert {
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                target: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            9 => SemanticOriginalExprKind::Bool(
                                match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            10 => SemanticOriginalExprKind::String(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            11 => SemanticOriginalExprKind::Unit,
                            12 => SemanticOriginalExprKind::Name(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            13 => SemanticOriginalExprKind::Unary {
                                op: match fields.next().expect("recipe field") {
                                    Value::K193(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                operand: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            14 => SemanticOriginalExprKind::Binary {
                                op: match fields.next().expect("recipe field") {
                                    Value::K194(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                left: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                right: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            15 => SemanticOriginalExprKind::TypeApply {
                                callee: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                type_arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K130(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            16 => SemanticOriginalExprKind::Call {
                                callee: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K213(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            17 => SemanticOriginalExprKind::Tuple(
                                match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K213(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            ),
                            18 => SemanticOriginalExprKind::Array(
                                match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K213(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            ),
                            19 => SemanticOriginalExprKind::Index {
                                object: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                index: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            20 => SemanticOriginalExprKind::Slice {
                                object: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                start: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| {
                                        Box::new(match *item {
                                            Value::K213(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                end: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| {
                                        Box::new(match *item {
                                            Value::K213(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            },
                            21 => SemanticOriginalExprKind::Member {
                                object: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                member: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                owner: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K130(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                            },
                            22 => SemanticOriginalExprKind::ReceiverMember(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            23 => SemanticOriginalExprKind::Record {
                                ty: match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::Tuple(rows) => {
                                                let mut fields = rows.into_iter();
                                                (
                                                    match fields.next().expect("tuple field") {
                                                        Value::K124(value) => value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                    match fields.next().expect("tuple field") {
                                                        Value::K213(value) => *value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                )
                                            }
                                            _ => unreachable!("recipe tuple grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            24 => SemanticOriginalExprKind::RecordUpdate {
                                ty: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K130(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                base: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::Tuple(rows) => {
                                                let mut fields = rows.into_iter();
                                                (
                                                    match fields.next().expect("tuple field") {
                                                        Value::K124(value) => value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                    match fields.next().expect("tuple field") {
                                                        Value::K213(value) => *value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                )
                                            }
                                            _ => unreachable!("recipe tuple grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            25 => SemanticOriginalExprKind::Sequence {
                                bindings: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::Tuple(rows) => {
                                                let mut fields = rows.into_iter();
                                                (
                                                    match fields.next().expect("tuple field") {
                                                        Value::K124(value) => value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                    match fields.next().expect("tuple field") {
                                                        Value::K213(value) => *value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                )
                                            }
                                            _ => unreachable!("recipe tuple grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            26 => SemanticOriginalExprKind::CompileTime(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            27 => SemanticOriginalExprKind::Try(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            28 => SemanticOriginalExprKind::Closure {
                                move_capture: match fields.next().expect("recipe field") {
                                    Value::K33(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                parameters: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K114(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                return_type: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K130(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                body: match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            29 => SemanticOriginalExprKind::If {
                                condition: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                then_value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                else_value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            30 => SemanticOriginalExprKind::Unsupported(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            31 => SemanticOriginalExprKind::Spread(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            32 => SemanticOriginalExprKind::Group(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            33 => SemanticOriginalExprKind::Character(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            34 => SemanticOriginalExprKind::Range {
                                start: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                end: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            35 => SemanticOriginalExprKind::Pipeline {
                                input: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                target: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                            },
                            36 => SemanticOriginalExprKind::RecordConstructor {
                                definition: match fields.next().expect("recipe field") {
                                    Value::Optional(value) => value.map(|item| match *item {
                                        Value::K84(value) => *value,
                                        _ => unreachable!("recipe field grammar"),
                                    }),
                                    _ => unreachable!("recipe optional grammar"),
                                },
                                fields: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::Tuple(rows) => {
                                                let mut fields = rows.into_iter();
                                                (
                                                    match fields.next().expect("tuple field") {
                                                        Value::K124(value) => value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                    match fields.next().expect("tuple field") {
                                                        Value::K213(value) => *value,
                                                        _ => unreachable!("recipe field grammar"),
                                                    },
                                                )
                                            }
                                            _ => unreachable!("recipe tuple grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            37 => SemanticOriginalExprKind::Splice(Box::new(
                                match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            )),
                            38 => SemanticOriginalExprKind::CompileTimeBlock(
                                match fields.next().expect("recipe field") {
                                    Value::K121(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            39 => SemanticOriginalExprKind::SyntaxQuote {
                                category: match fields.next().expect("recipe field") {
                                    Value::K224(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                template: match fields.next().expect("recipe field") {
                                    Value::K225(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            40 => SemanticOriginalExprKind::SyntaxIsland {
                                path: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K124(value) => value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                tree: match fields.next().expect("recipe field") {
                                    Value::K226(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            41 => SemanticOriginalExprKind::GenericApply {
                                callee: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                arguments: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K228(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K219 => Value::K219(Box::new(match tag {
                            0 => DynOwnedStorage::Heap,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K220 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K221 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K222 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K223 => {
                            let _ = tag;
                            Value::Tuple(fields.collect())
                        }
                        Kind::K224 => Value::K224(Box::new(match tag {
                            0 => SemanticOriginalSyntaxCategory::Expr,
                            1 => SemanticOriginalSyntaxCategory::Type,
                            2 => SemanticOriginalSyntaxCategory::StmtList,
                            3 => SemanticOriginalSyntaxCategory::ItemList,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K225 => {
                            let _ = tag;
                            Value::K225(Box::new(SemanticOriginalQuoteTemplate {
                                tree: match fields.next().expect("recipe field") {
                                    Value::K226(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                holes: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K229(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                name_sites: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K233(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                            }))
                        }
                        Kind::K226 => {
                            let _ = tag;
                            Value::K226(Box::new(SemanticOriginalSyntaxTree {
                                delimiter: match fields.next().expect("recipe field") {
                                    Value::K234(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                children: match fields.next().expect("recipe field") {
                                    Value::Rows(rows) => rows
                                        .into_iter()
                                        .map(|item| match item {
                                            Value::K238(value) => *value,
                                            _ => unreachable!("recipe field grammar"),
                                        })
                                        .collect(),
                                    _ => unreachable!("recipe row grammar"),
                                },
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K227 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K228 => Value::K228(Box::new(match tag {
                            0 => SemanticOriginalGenericArgument::Type(
                                match fields.next().expect("recipe field") {
                                    Value::K130(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            1 => SemanticOriginalGenericArgument::Const(
                                match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => SemanticOriginalGenericArgument::Lifetime(
                                match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K229 => {
                            let _ = tag;
                            Value::K229(Box::new(SemanticOriginalQuoteHole {
                                kind: match fields.next().expect("recipe field") {
                                    Value::K230(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                value: Box::new(match fields.next().expect("recipe field") {
                                    Value::K213(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                }),
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K230 => Value::K230(Box::new(match tag {
                            0 => SemanticOriginalQuoteHoleKind::Expr,
                            1 => SemanticOriginalQuoteHoleKind::Type,
                            2 => SemanticOriginalQuoteHoleKind::StmtList,
                            3 => SemanticOriginalQuoteHoleKind::ItemList,
                            4 => SemanticOriginalQuoteHoleKind::Ident {
                                namespace: match fields.next().expect("recipe field") {
                                    Value::K235(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                role: match fields.next().expect("recipe field") {
                                    Value::K236(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K231 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K232 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K233 => {
                            let _ = tag;
                            Value::K233(Box::new(SemanticOriginalQuoteNameSite {
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                token_index: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            }))
                        }
                        Kind::K234 => Value::K234(Box::new(match tag {
                            0 => SemanticOriginalSyntaxDelimiter::Parenthesis,
                            1 => SemanticOriginalSyntaxDelimiter::Bracket,
                            2 => SemanticOriginalSyntaxDelimiter::Brace,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K235 => Value::K235(Box::new(match tag {
                            0 => SemanticOriginalQuoteNameNamespace::Value,
                            1 => SemanticOriginalQuoteNameNamespace::Type,
                            2 => SemanticOriginalQuoteNameNamespace::Member,
                            3 => SemanticOriginalQuoteNameNamespace::Module,
                            4 => SemanticOriginalQuoteNameNamespace::Label,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K236 => Value::K236(Box::new(match tag {
                            0 => SemanticOriginalQuoteNameRole::Binding,
                            1 => SemanticOriginalQuoteNameRole::Reference,
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K237 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K238 => Value::K238(Box::new(match tag {
                            0 => SemanticOriginalSyntaxTreeEntry::Token {
                                text: match fields.next().expect("recipe field") {
                                    Value::K124(value) => value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            1 => SemanticOriginalSyntaxTreeEntry::Tree(
                                match fields.next().expect("recipe field") {
                                    Value::K226(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            ),
                            2 => SemanticOriginalSyntaxTreeEntry::Hole {
                                index: match fields.next().expect("recipe field") {
                                    Value::K6(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                                span: match fields.next().expect("recipe field") {
                                    Value::K202(value) => *value,
                                    _ => unreachable!("recipe field grammar"),
                                },
                            },
                            _ => unreachable!("validated recipe tag"),
                        })),
                        Kind::K239 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K240 => Value::K240(Box::new(match tag {
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
                        Kind::K241 => {
                            let _ = tag;
                            Value::Optional(Some(Box::new(fields.next().expect("optional field"))))
                        }
                        Kind::K242 => {
                            let _ = tag;
                            Value::Rows(fields.collect())
                        }
                        Kind::K243 => {
                            let _ = tag;
                            Value::K243(Box::new(VariantProjection {
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
