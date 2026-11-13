//! Immutable Core-stage snapshot identity. This is a version-bound content
//! digest and request binding, never nominal identity or semantic authority.
use super::*;
use std::{
    fmt::{self, Write},
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{Fingerprint, FingerprintBuilder, ModuleId, NodeId};
use zil_ir::ct_artifact::CtStageIdentity;
static EPOCH: AtomicU64 = AtomicU64::new(1);
fn exhausted() -> Vec<Diagnostic> {
    vec![Diagnostic::limit(
        Phase::Ct,
        "frozen Core snapshot budget exhausted",
        None,
    )]
}
struct Meter<'a> {
    limits: &'a Limits,
    nodes: &'a mut usize,
    work: &'a mut usize,
    stack_bytes: usize,
    tree_bytes: usize,
    used: Option<BTreeSet<NodeId>>,
}
impl Meter<'_> {
    fn node(&mut self, depth: usize) -> CompileResult<()> {
        if depth > self.limits.max_call_depth.min(128) {
            return Err(exhausted());
        }
        *self.nodes = self
            .nodes
            .checked_add(1)
            .filter(|n| *n <= self.limits.max_nodes)
            .ok_or_else(exhausted)?;
        *self.work = self.work.checked_sub(1).ok_or_else(exhausted)?;
        Ok(())
    }
    fn rows(&mut self, n: usize, _width: usize) -> CompileResult<()> {
        *self.work = self.work.checked_sub(n).ok_or_else(exhausted)?;
        Ok(())
    }
    fn remember(&mut self, node: NodeId) -> CompileResult<()> {
        let Some(used) = &mut self.used else {
            return Ok(());
        };
        *self.work = self
            .work
            .checked_sub(used.len().checked_ilog2().unwrap_or(0) as usize + 1)
            .ok_or_else(exhausted)?;
        if !used.contains(&node) {
            self.tree_bytes = self
                .tree_bytes
                .checked_add(16 * std::mem::size_of::<NodeId>())
                .ok_or_else(exhausted)?;
            if self.tree_bytes.saturating_add(self.stack_bytes) > self.limits.max_source_bytes {
                return Err(exhausted());
            }
            used.insert(node);
        }
        Ok(())
    }
}
enum Walk<'a> {
    W0(&'a CoreProgram),
    W1(&'a ModuleId),
    W2(&'a Fingerprint),
    W3(&'a Vec<ProgramModule>),
    W4(&'a ProgramModule),
    W5(&'a CoreModule),
    W6(&'a Vec<CoreImport>),
    W7(&'a CoreImport),
    W8(&'a Vec<NodeId>),
    W9(&'a NodeId),
    W10(&'a String),
    W11(&'a Span),
    W12(&'a Option<SemanticModule>),
    W13(&'a SemanticModule),
    W14(&'a Vec<DynDeclaration>),
    W15(&'a DynDeclaration),
    W16(&'a Vec<DynInterface>),
    W17(&'a DynInterface),
    W18(&'a Vec<CoreDynVtable>),
    W19(&'a CoreDynVtable),
    W20(&'a Vec<LifeNominalDeclaration>),
    W21(&'a LifeNominalDeclaration),
    W22(&'a Vec<LifeNominalInstance>),
    W23(&'a LifeNominalInstance),
    W24(&'a Vec<CoreClosureLifeContext>),
    W25(&'a CoreClosureLifeContext),
    W26(&'a u32),
    W27(&'a Vec<CoreFunction>),
    W28(&'a CoreFunction),
    W29(&'a Vec<CoreAggregate>),
    W30(&'a CoreAggregate),
    W31(&'a Vec<CoreAggregateTemplate>),
    W32(&'a CoreAggregateTemplate),
    W33(&'a Vec<CoreAggregateApplication>),
    W34(&'a CoreAggregateApplication),
    W35(&'a Vec<CoreEnum>),
    W36(&'a CoreEnum),
    W37(&'a Vec<UnsupportedFeature>),
    W38(&'a UnsupportedFeature),
    W39(&'a Option<Span>),
    W40(&'a Vec<(String, Vec<Type>)>),
    W41(&'a (String, Vec<Type>)),
    W42(&'a Vec<Type>),
    W43(&'a Type),
    W44(&'a DynInterfaceId),
    W45(&'a StorageLifeShape),
    W46(&'a SyncCellKind),
    W47(&'a RetainedLifeShape),
    W48(&'a Vec<(String, Type)>),
    W49(&'a (String, Type)),
    W50(&'a bool),
    W51(&'a StorageLifeContract),
    W52(&'a ClosureId),
    W53(&'a Vec<StorageLifeBinding>),
    W54(&'a StorageLifeBinding),
    W55(&'a Vec<StoragePosition>),
    W56(&'a StoragePosition),
    W57(&'a Vec<SignatureLifeClassBinding>),
    W58(&'a SignatureLifeClassBinding),
    W59(&'a SignatureLifeClass),
    W60(&'a StorageBinder),
    W61(&'a StorageBase),
    W62(&'a Vec<LifeTypeProjection>),
    W63(&'a LifeTypeProjection),
    W64(&'a PlaceProjection),
    W65(&'a VariantProjection),
    W66(&'a Vec<ContractProjection>),
    W67(&'a ContractProjection),
    W68(&'a Vec<RetainedBorrowPort>),
    W69(&'a RetainedBorrowPort),
    W70(&'a Vec<StorageRetainedBinding>),
    W71(&'a StorageRetainedBinding),
    W72(&'a Vec<NominalLifeArgument>),
    W73(&'a NominalLifeArgument),
    W74(&'a LifeDeclarationId),
    W75(&'a StoredBorrowMode),
    W76(&'a Vec<StorageLifeSlot>),
    W77(&'a StorageLifeSlot),
    W78(&'a Vec<PlaceProjection>),
    W79(&'a Option<CoreAggregateOwner>),
    W80(&'a CoreAggregateOwner),
    W81(&'a Vec<CoreAggregateArgument>),
    W82(&'a CoreAggregateArgument),
    W83(&'a Vec<CoreTypeParameter>),
    W84(&'a CoreTypeParameter),
    W85(&'a Vec<CoreAggregateFieldRecipe>),
    W86(&'a CoreAggregateFieldRecipe),
    W87(&'a CoreAggregateTypeRecipe),
    W88(&'a Vec<CoreTaskPortRecipe>),
    W89(&'a CoreTaskPortRecipe),
    W90(&'a Vec<CoreAggregateTypeRecipe>),
    W91(&'a CoreTypeBound),
    W92(&'a AggregateKind),
    W93(&'a Option<String>),
    W94(&'a Vec<CoreField>),
    W95(&'a CoreField),
    W96(&'a Option<DynFunctionInstance>),
    W97(&'a DynFunctionInstance),
    W98(&'a Option<Vec<StorageSourceBinding>>),
    W99(&'a Vec<StorageSourceBinding>),
    W100(&'a StorageSourceBinding),
    W101(&'a Option<CoreFunctionSourceScope>),
    W102(&'a CoreFunctionSourceScope),
    W103(&'a Option<StorageLifeContract>),
    W104(&'a Option<CallKind>),
    W105(&'a CallKind),
    W106(&'a TaskResumeBound),
    W107(&'a Option<CoreLifeContract>),
    W108(&'a CoreLifeContract),
    W109(&'a Vec<CoreStateParameter>),
    W110(&'a CoreStateParameter),
    W111(&'a Vec<FunctionProtocolContract>),
    W112(&'a FunctionProtocolContract),
    W113(&'a Option<CoreCallableReturn>),
    W114(&'a CoreCallableReturn),
    W115(&'a Vec<CoreCallableParameter>),
    W116(&'a CoreCallableParameter),
    W117(&'a Option<ForeignDeclaration>),
    W118(&'a ForeignDeclaration),
    W119(&'a Vec<CoreParameter>),
    W120(&'a CoreParameter),
    W121(&'a PassingMode),
    W122(&'a CoreBlock),
    W123(&'a Option<ReceiverShape>),
    W124(&'a ReceiverShape),
    W125(&'a Vec<CoreStatement>),
    W126(&'a CoreStatement),
    W127(&'a CoreStatementKind),
    W128(&'a Option<Type>),
    W129(&'a Option<CoreExpr>),
    W130(&'a CoreExpr),
    W131(&'a CorePattern),
    W132(&'a Option<CoreBlock>),
    W133(&'a Vec<CoreMatchArm>),
    W134(&'a CoreMatchArm),
    W135(&'a BinaryOp),
    W136(&'a CoreExprKind),
    W137(&'a DynVtableId),
    W138(&'a DynOwnedStorage),
    W139(&'a Vec<CoreExpr>),
    W140(&'a Constant),
    W141(&'a UnaryOp),
    W142(&'a Option<Box<CoreExpr>>),
    W143(&'a Vec<(String, CoreExpr)>),
    W144(&'a (String, CoreExpr)),
    W145(&'a Vec<CoreClosureParameter>),
    W146(&'a CoreClosureParameter),
    W147(&'a Option<SemanticDefinitionId>),
    W148(&'a SemanticDefinitionId),
    W149(&'a CoreSyntaxCategory),
    W150(&'a CoreQuoteTemplate),
    W151(&'a Vec<String>),
    W152(&'a CoreSyntaxTree),
    W153(&'a Vec<CoreGenericArgument>),
    W154(&'a CoreGenericArgument),
    W155(&'a CoreSyntaxDelimiter),
    W156(&'a Vec<CoreSyntaxTreeEntry>),
    W157(&'a CoreSyntaxTreeEntry),
    W158(&'a Vec<CoreQuoteHole>),
    W159(&'a CoreQuoteHole),
    W160(&'a Vec<CoreQuoteNameSite>),
    W161(&'a CoreQuoteNameSite),
    W162(&'a CoreQuoteHoleKind),
    W163(&'a CoreQuoteNameNamespace),
    W164(&'a CoreQuoteNameRole),
    W165(&'a i32),
    W166(&'a i64),
    W167(&'a u64),
    W168(&'a u8),
    W169(&'a u16),
    W170(&'a i8),
    W171(&'a i16),
    W172(&'a Vec<u16>),
    W173(&'a CorePatternKind),
    W174(&'a Vec<CorePattern>),
    W175(&'a Vec<(String, CorePattern)>),
    W176(&'a (String, CorePattern)),
    W177(&'a Vec<u32>),
    W178(&'a Option<ScopedCallbackRole>),
    W179(&'a ScopedCallbackRole),
    W180(&'a Option<CoreCallableLifeContext>),
    W181(&'a CoreCallableLifeContext),
    W182(&'a CallableKind),
    W183(&'a LifeDeclarationIdentity),
    W184(&'a Vec<LifeParameterKind>),
    W185(&'a LifeParameterKind),
    W186(&'a Vec<CoreClosureNamedFixed>),
    W187(&'a CoreClosureNamedFixed),
    W188(&'a Vec<CoreClosureInputAnchor>),
    W189(&'a CoreClosureInputAnchor),
    W190(&'a Option<u32>),
    W191(&'a CoreClosureHeaderPosition),
    W192(&'a Option<CoreCallableReturnLifeContext>),
    W193(&'a CoreCallableReturnLifeContext),
    W194(&'a Vec<CoreCallableReturnFixed>),
    W195(&'a CoreCallableReturnFixed),
    W196(&'a ProtocolKind),
    W197(&'a Vec<ProtocolState>),
    W198(&'a ProtocolState),
    W199(&'a Vec<ProtocolTransition>),
    W200(&'a ProtocolTransition),
    W201(&'a ProtocolExit),
    W202(&'a Vec<CoreStateBinding>),
    W203(&'a CoreStateBinding),
    W204(&'a Vec<Option<u32>>),
    W205(&'a Vec<StorageSource>),
    W206(&'a StorageSource),
    W207(&'a DynInstantiationScope),
    W208(&'a Vec<DynLifeEmbedding>),
    W209(&'a DynLifeEmbedding),
    W210(&'a DynDefinitionId),
    W211(&'a DynTypeValue),
    W212(&'a Vec<DynGenericActual>),
    W213(&'a DynGenericActual),
    W214(&'a Vec<StorageBinder>),
    W215(&'a Vec<LifeNominalActual>),
    W216(&'a LifeNominalActual),
    W217(&'a LifeNominalKind),
    W218(&'a Option<StorageLifeShape>),
    W219(&'a Option<Vec<DynImplementationWitness>>),
    W220(&'a Vec<DynImplementationWitness>),
    W221(&'a DynImplementationWitness),
    W222(&'a Vec<DynRetainedPortBinding>),
    W223(&'a DynRetainedPortBinding),
    W224(&'a Vec<CoreDynEntry>),
    W225(&'a CoreDynEntry),
    W226(&'a Option<CoreDynEntry>),
    W227(&'a Option<DynOwnedStorage>),
    W228(&'a DynTraitApplication),
    W229(&'a Vec<DynAssociatedResolution>),
    W230(&'a DynAssociatedResolution),
    W231(&'a DynMember),
    W232(&'a DynAssociatedSource),
    W233(&'a DynAssociatedValue),
    W234(&'a Option<DynCleanupContract>),
    W235(&'a DynCleanupContract),
    W236(&'a Vec<DynTraitApplication>),
    W237(&'a Vec<DynAssociatedType>),
    W238(&'a DynAssociatedType),
    W239(&'a Vec<DynAssociatedConstant>),
    W240(&'a DynAssociatedConstant),
    W241(&'a Vec<DynSlot>),
    W242(&'a DynSlot),
    W243(&'a DynReceiver),
    W244(&'a Vec<PassingMode>),
    W245(&'a DynContract),
    W246(&'a Option<Vec<DynRequirement>>),
    W247(&'a Vec<DynRequirement>),
    W248(&'a DynRequirement),
    W249(&'a Option<Vec<DynGuarantee>>),
    W250(&'a Vec<DynGuarantee>),
    W251(&'a DynGuarantee),
    W252(&'a Option<Vec<StoragePosition>>),
    W253(&'a Option<DynCallAccess>),
    W254(&'a DynCallAccess),
    W255(&'a Option<DynResultSource>),
    W256(&'a DynResultSource),
    W257(&'a Option<Vec<DynEffect>>),
    W258(&'a Vec<DynEffect>),
    W259(&'a DynEffect),
    W260(&'a Option<DynResumeBound>),
    W261(&'a DynResumeBound),
    W262(&'a Option<DynThreadRequirements>),
    W263(&'a DynThreadRequirements),
    W264(&'a Option<Vec<DynImplicitLifeDeclaration>>),
    W265(&'a Vec<DynImplicitLifeDeclaration>),
    W266(&'a DynImplicitLifeDeclaration),
    W267(&'a Option<Vec<DynSelfPattern>>),
    W268(&'a Vec<DynSelfPattern>),
    W269(&'a DynSelfPattern),
    W270(&'a Option<Vec<DynObligation>>),
    W271(&'a Vec<DynObligation>),
    W272(&'a DynObligation),
    W273(&'a Option<Vec<DynAssociatedDefinition>>),
    W274(&'a Vec<DynAssociatedDefinition>),
    W275(&'a DynAssociatedDefinition),
    W276(&'a Option<Vec<DynMethodRecipe>>),
    W277(&'a Vec<DynMethodRecipe>),
    W278(&'a DynMethodRecipe),
    W279(&'a Option<Vec<DynDefinitionDeclaration>>),
    W280(&'a Vec<DynDefinitionDeclaration>),
    W281(&'a DynDefinitionDeclaration),
    W282(&'a Option<DynTypePattern>),
    W283(&'a DynTypePattern),
    W284(&'a Option<DynSelfPattern>),
    W285(&'a DynDeclarationKind),
    W286(&'a Vec<DynMemberDeclaration>),
    W287(&'a DynMemberDeclaration),
    W288(&'a DynMemberKind),
    W289(&'a Vec<DynPatternActual>),
    W290(&'a DynPatternActual),
    W291(&'a DynConstPattern),
    W292(&'a DynLifePattern),
    W293(&'a Option<DynLifePattern>),
    W294(&'a Vec<DynTypePattern>),
    W295(&'a SemanticTypeRecipe),
    W296(&'a SemanticExactTypeRecipe),
    W297(&'a SemanticFormalRef),
    W298(&'a SemanticScopeRef),
    W299(&'a Vec<SemanticActualRecipe>),
    W300(&'a SemanticActualRecipe),
    W301(&'a SemanticTraitRecipe),
    W302(&'a Vec<SemanticTypeRecipe>),
    W303(&'a SemanticConstRecipe),
    W304(&'a SemanticOuterLifeRecipe),
    W305(&'a SemanticRetainedShapeRecipe),
    W306(&'a SemanticStorageShapeRecipe),
    W307(&'a SemanticCodeKind),
    W308(&'a SemanticSignatureRecipe),
    W309(&'a SemanticDynInterfaceRecipe),
    W310(&'a SemanticOriginalTypeSource),
    W311(&'a SemanticOriginalNode),
    W312(&'a SemanticLifeRecipe),
    W313(&'a SemanticRecipeOperand),
    W314(&'a SemanticDynContextRecipe),
    W315(&'a Vec<SemanticAssociatedBindingRecipe>),
    W316(&'a SemanticAssociatedBindingRecipe),
    W317(&'a SemanticAssociatedRecipe),
    W318(&'a SemanticClassSequenceRecipe),
    W319(&'a Vec<SemanticPortPart>),
    W320(&'a SemanticPortPart),
    W321(&'a Vec<SemanticRetainedPart>),
    W322(&'a SemanticRetainedPart),
    W323(&'a Vec<SemanticNominalPart>),
    W324(&'a SemanticNominalPart),
    W325(&'a Vec<SemanticClassSequencePart>),
    W326(&'a SemanticClassSequencePart),
    W327(&'a Vec<SemanticParameterRecipe>),
    W328(&'a SemanticParameterRecipe),
    W329(&'a Vec<SemanticSlotRecipe>),
    W330(&'a SemanticSlotRecipe),
    W331(&'a Vec<SemanticLifeRecipe>),
    W332(&'a Vec<DynDefinitionSpecialization>),
    W333(&'a DynDefinitionSpecialization),
    W334(&'a DynDefinitionScope),
    W335(&'a DynSignatureRecipe),
    W336(&'a Vec<DynParameterRecipe>),
    W337(&'a DynParameterRecipe),
    W338(&'a DynContractRecipe),
    W339(&'a Option<Vec<DynRequirementRecipe>>),
    W340(&'a Vec<DynRequirementRecipe>),
    W341(&'a DynRequirementRecipe),
    W342(&'a Option<Vec<DynGuaranteeRecipe>>),
    W343(&'a Vec<DynGuaranteeRecipe>),
    W344(&'a DynGuaranteeRecipe),
    W345(&'a Option<DynResultSourceRecipe>),
    W346(&'a DynResultSourceRecipe),
    W347(&'a Option<DynCleanupRecipe>),
    W348(&'a DynCleanupRecipe),
    W349(&'a Option<Vec<DynEffectRecipe>>),
    W350(&'a Vec<DynEffectRecipe>),
    W351(&'a DynEffectRecipe),
    W352(&'a Vec<DynStorageSourceRecipe>),
    W353(&'a DynStorageSourceRecipe),
    W354(&'a DynStorageSourceBindingRecipe),
    W355(&'a DynFunctionTarget),
    W356(&'a FunctionId),
    W357(&'a DynStructuralBound),
    W358(&'a Option<DynDefinitionId>),
    W359(&'a Option<DynAssociatedValueRecipe>),
    W360(&'a DynAssociatedValueRecipe),
    W361(&'a Vec<DynImplicitLifeOccurrence>),
    W362(&'a DynImplicitLifeOccurrence),
    W363(&'a DynImplicitLifeRoot),
    W364(&'a DynImplicitLifeKind),
    W365(&'a Vec<SemanticDefinition>),
    W366(&'a SemanticDefinition),
    W367(&'a Vec<SemanticInstance>),
    W368(&'a SemanticInstance),
    W369(&'a Vec<SemanticSite>),
    W370(&'a SemanticSite),
    W371(&'a Vec<SemanticDynScopeBinding>),
    W372(&'a SemanticDynScopeBinding),
    W373(&'a SemanticDynOwner),
    W374(&'a SemanticInstanceId),
    W375(&'a SemanticSiteId),
    W376(&'a SemanticSiteAddress),
    W377(&'a SemanticSiteAction),
    W378(&'a SemanticOperand),
    W379(&'a SemanticTypeValue),
    W380(&'a SemanticFunctionTarget),
    W381(&'a Vec<SemanticActual>),
    W382(&'a SemanticActual),
    W383(&'a SemanticLocalTarget),
    W384(&'a SemanticClosureTarget),
    W385(&'a SemanticClassExpr),
    W386(&'a ValueId),
    W387(&'a LocalId),
    W388(&'a Vec<SemanticClassExpr>),
    W389(&'a SemanticCoreTypeSource),
    W390(&'a SemanticEmissionRole),
    W391(&'a BlockId),
    W392(&'a SemanticSiteEdge),
    W393(&'a SemanticInstanceTarget),
    W394(&'a SemanticLifetimeInstantiation),
    W395(&'a Option<SemanticInstanceId>),
    W396(&'a Vec<SemanticHeaderBinding>),
    W397(&'a SemanticHeaderBinding),
    W398(&'a Option<SemanticEnvironmentInstance>),
    W399(&'a SemanticEnvironmentInstance),
    W400(&'a Option<SemanticTypeValue>),
    W401(&'a SemanticDefinitionKind),
    W402(&'a Vec<SemanticFormal>),
    W403(&'a SemanticFormal),
    W404(&'a Vec<SemanticImplicitLifetime>),
    W405(&'a SemanticImplicitLifetime),
    W406(&'a Vec<SemanticRequirementRecipe>),
    W407(&'a SemanticRequirementRecipe),
    W408(&'a Vec<SemanticEnvironmentFormal>),
    W409(&'a SemanticEnvironmentFormal),
    W410(&'a SemanticDefinitionBody),
    W411(&'a Vec<SemanticSiteRecipe>),
    W412(&'a SemanticSiteRecipe),
    W413(&'a SemanticNominalBody),
    W414(&'a SemanticScopeRecipe),
    W415(&'a Vec<SemanticOriginalClosureParameter>),
    W416(&'a SemanticOriginalClosureParameter),
    W417(&'a Vec<SemanticOriginalNameBinding>),
    W418(&'a SemanticOriginalNameBinding),
    W419(&'a Vec<SemanticOriginalSiteNode>),
    W420(&'a SemanticOriginalSiteNode),
    W421(&'a SemanticOriginalFunctionBody),
    W422(&'a SemanticOriginalBlock),
    W423(&'a Vec<SemanticOriginalStatement>),
    W424(&'a SemanticOriginalStatement),
    W425(&'a SemanticOriginalStatementKind),
    W426(&'a Option<SemanticTypeRecipe>),
    W427(&'a Option<SemanticOriginalExpr>),
    W428(&'a SemanticOriginalExpr),
    W429(&'a SemanticOriginalPattern),
    W430(&'a Option<SemanticOriginalBlock>),
    W431(&'a Vec<SemanticOriginalMatchArm>),
    W432(&'a SemanticOriginalMatchArm),
    W433(&'a SemanticOriginalExprKind),
    W434(&'a Vec<SemanticOriginalExpr>),
    W435(&'a Option<Box<SemanticOriginalExpr>>),
    W436(&'a Vec<(String, SemanticOriginalExpr)>),
    W437(&'a (String, SemanticOriginalExpr)),
    W438(&'a SemanticOriginalSyntaxCategory),
    W439(&'a SemanticOriginalQuoteTemplate),
    W440(&'a SemanticOriginalSyntaxTree),
    W441(&'a Vec<SemanticOriginalGenericArgument>),
    W442(&'a SemanticOriginalGenericArgument),
    W443(&'a SemanticOriginalSyntaxDelimiter),
    W444(&'a Vec<SemanticOriginalSyntaxTreeEntry>),
    W445(&'a SemanticOriginalSyntaxTreeEntry),
    W446(&'a Vec<SemanticOriginalQuoteHole>),
    W447(&'a SemanticOriginalQuoteHole),
    W448(&'a Vec<SemanticOriginalQuoteNameSite>),
    W449(&'a SemanticOriginalQuoteNameSite),
    W450(&'a SemanticOriginalQuoteHoleKind),
    W451(&'a SemanticOriginalQuoteNameNamespace),
    W452(&'a SemanticOriginalQuoteNameRole),
    W453(&'a SemanticOriginalPatternKind),
    W454(&'a Vec<SemanticOriginalPattern>),
    W455(&'a Vec<(String, SemanticOriginalPattern)>),
    W456(&'a (String, SemanticOriginalPattern)),
    W457(&'a SemanticOriginalNamespace),
    W458(&'a SemanticOriginalNameValue),
    W459(&'a SemanticSiteRecipeKind),
    W460(&'a SemanticBuiltinTrait),
    W461(&'a Vec<SemanticFieldRecipe>),
    W462(&'a SemanticFieldRecipe),
    W463(&'a Vec<SemanticVariantRecipe>),
    W464(&'a SemanticVariantRecipe),
}
fn prepare<'a>(
    pending: &mut Vec<(Walk<'a>, usize)>,
    meter: &mut Meter<'_>,
    count: usize,
    width: usize,
) -> CompileResult<()> {
    meter.rows(count, width)?;
    let needed = pending.len().checked_add(count).ok_or_else(exhausted)?;
    if needed > pending.capacity() {
        let capacity = needed.max(pending.capacity().saturating_mul(2)).max(4);
        let bytes = capacity
            .checked_mul(std::mem::size_of::<(Walk<'_>, usize)>())
            .ok_or_else(exhausted)?;
        if meter
            .tree_bytes
            .saturating_add(meter.stack_bytes)
            .saturating_add(bytes)
            > meter.limits.max_source_bytes
        {
            return Err(exhausted());
        }
        *meter.work = meter
            .work
            .checked_sub(pending.len())
            .ok_or_else(exhausted)?;
        pending
            .try_reserve_exact(capacity - pending.len())
            .map_err(|_| exhausted())?;
        meter.stack_bytes = pending
            .capacity()
            .checked_mul(std::mem::size_of::<(Walk<'_>, usize)>())
            .ok_or_else(exhausted)?;
    }
    Ok(())
}
fn preflight(value: &CoreProgram, meter: &mut Meter<'_>) -> CompileResult<()> {
    let mut pending = Vec::new();
    prepare(
        &mut pending,
        meter,
        1,
        std::mem::size_of::<(Walk<'_>, usize)>(),
    )?;
    pending.push((Walk::W0(value), 0));
    while let Some((next, depth)) = pending.pop() {
        match next {
            Walk::W0(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W3(&value.modules), depth + 1));
                pending.push((Walk::W2(&value.source_fingerprint), depth + 1));
                pending.push((Walk::W1(&value.root), depth + 1));
            }
            Walk::W1(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W2(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W3(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W4(child), depth));
                }
            }
            Walk::W4(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W8(&value.exports), depth + 1));
                pending.push((Walk::W6(&value.imports), depth + 1));
                pending.push((Walk::W5(&value.core), depth + 1));
            }
            Walk::W5(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    16,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W37(&value.unsupported), depth + 1));
                pending.push((Walk::W35(&value.enums), depth + 1));
                pending.push((Walk::W33(&value.aggregate_applications), depth + 1));
                pending.push((Walk::W31(&value.aggregate_templates), depth + 1));
                pending.push((Walk::W29(&value.aggregates), depth + 1));
                pending.push((Walk::W27(&value.functions), depth + 1));
                pending.push((Walk::W2(&value.source_fingerprint), depth + 1));
                pending.push((Walk::W1(&value.module_id), depth + 1));
                pending.push((Walk::W26(&value.schema_version), depth + 1));
                pending.push((Walk::W24(&value.closure_life_contexts), depth + 1));
                pending.push((Walk::W22(&value.life_instances), depth + 1));
                pending.push((Walk::W20(&value.life_declarations), depth + 1));
                pending.push((Walk::W18(&value.dyn_vtables), depth + 1));
                pending.push((Walk::W16(&value.dyn_interfaces), depth + 1));
                pending.push((Walk::W14(&value.dyn_declarations), depth + 1));
                pending.push((Walk::W12(&value.semantic), depth + 1));
            }
            Walk::W6(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W7(child), depth));
                }
            }
            Walk::W7(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W1(&value.module), depth + 1));
                pending.push((Walk::W10(&value.alias), depth + 1));
            }
            Walk::W8(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W9(child), depth));
                }
            }
            Walk::W9(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W10(value) => {
                *meter.work = meter.work.checked_sub(value.len()).ok_or_else(exhausted)?;
            }
            Walk::W11(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W12(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W13(child), depth));
                }
            }
            Walk::W13(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W371(&value.dyn_scopes), depth + 1));
                pending.push((Walk::W369(&value.sites), depth + 1));
                pending.push((Walk::W367(&value.instances), depth + 1));
                pending.push((Walk::W365(&value.definitions), depth + 1));
            }
            Walk::W14(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W15(child), depth));
                }
            }
            Walk::W15(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    13,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W286(&value.members), depth + 1));
                pending.push((Walk::W184(&value.parameters), depth + 1));
                pending.push((Walk::W285(&value.kind), depth + 1));
                pending.push((Walk::W183(&value.identity), depth + 1));
                pending.push((Walk::W74(&value.id), depth + 1));
                pending.push((Walk::W284(&value.implemented_trait), depth + 1));
                pending.push((Walk::W282(&value.self_pattern), depth + 1));
                pending.push((Walk::W279(&value.method_definitions), depth + 1));
                pending.push((Walk::W276(&value.method_signatures), depth + 1));
                pending.push((Walk::W273(&value.associated_definitions), depth + 1));
                pending.push((Walk::W270(&value.requirements), depth + 1));
                pending.push((Walk::W267(&value.direct_supertraits), depth + 1));
                pending.push((Walk::W264(&value.implicit_lifetimes), depth + 1));
            }
            Walk::W16(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W17(child), depth));
                }
            }
            Walk::W17(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    9,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W241(&value.slots), depth + 1));
                pending.push((Walk::W47(&value.retained_life), depth + 1));
                pending.push((Walk::W239(&value.associated_constants), depth + 1));
                pending.push((Walk::W237(&value.associated_types), depth + 1));
                pending.push((Walk::W236(&value.inherited), depth + 1));
                pending.push((Walk::W228(&value.application), depth + 1));
                pending.push((Walk::W44(&value.id), depth + 1));
                pending.push((Walk::W234(&value.owning_cleanup), depth + 1));
                pending.push((Walk::W128(&value.context), depth + 1));
            }
            Walk::W18(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W19(child), depth));
                }
            }
            Walk::W19(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    12,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W227(&value.owned_storage), depth + 1));
                pending.push((Walk::W226(&value.cleanup), depth + 1));
                pending.push((Walk::W224(&value.entries), depth + 1));
                pending.push((Walk::W222(&value.ports), depth + 1));
                pending.push((Walk::W212(&value.actuals), depth + 1));
                pending.push((Walk::W74(&value.implementation), depth + 1));
                pending.push((Walk::W43(&value.self_type), depth + 1));
                pending.push((Walk::W44(&value.interface), depth + 1));
                pending.push((Walk::W137(&value.id), depth + 1));
                pending.push((Walk::W214(&value.self_classes), depth + 1));
                pending.push((Walk::W219(&value.implementation_witnesses), depth + 1));
                pending.push((Walk::W218(&value.self_life), depth + 1));
            }
            Walk::W20(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W21(child), depth));
                }
            }
            Walk::W21(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W184(&value.parameters), depth + 1));
                pending.push((Walk::W217(&value.kind), depth + 1));
                pending.push((Walk::W183(&value.identity), depth + 1));
                pending.push((Walk::W74(&value.id), depth + 1));
            }
            Walk::W22(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W23(child), depth));
                }
            }
            Walk::W23(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W215(&value.arguments), depth + 1));
                pending.push((Walk::W43(&value.ty), depth + 1));
                pending.push((Walk::W74(&value.declaration), depth + 1));
            }
            Walk::W24(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W25(child), depth));
                }
            }
            Walk::W25(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    9,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W188(&value.inputs), depth + 1));
                pending.push((Walk::W186(&value.named_fixed), depth + 1));
                pending.push((Walk::W51(&value.signature), depth + 1));
                pending.push((Walk::W26(&value.occurrence), depth + 1));
                pending.push((Walk::W184(&value.parameters), depth + 1));
                pending.push((Walk::W183(&value.identity), depth + 1));
                pending.push((Walk::W74(&value.declaration), depth + 1));
                pending.push((Walk::W9(&value.expression), depth + 1));
                pending.push((Walk::W9(&value.creator), depth + 1));
            }
            Walk::W26(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W27(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W28(child), depth));
                }
            }
            Walk::W28(value) => {
                meter.remember(value.id)?;
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    21,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W123(&value.receiver), depth + 1));
                pending.push((Walk::W122(&value.body), depth + 1));
                pending.push((Walk::W121(&value.result_mode), depth + 1));
                pending.push((Walk::W43(&value.return_type), depth + 1));
                pending.push((Walk::W119(&value.parameters), depth + 1));
                pending.push((Walk::W117(&value.foreign), depth + 1));
                pending.push((Walk::W115(&value.callable_parameters), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
                pending.push((Walk::W9(&value.id), depth + 1));
                pending.push((Walk::W113(&value.callable_return), depth + 1));
                pending.push((Walk::W111(&value.protocol_contracts), depth + 1));
                pending.push((Walk::W109(&value.state_parameters), depth + 1));
                pending.push((Walk::W83(&value.type_parameters), depth + 1));
                pending.push((Walk::W107(&value.life), depth + 1));
                pending.push((Walk::W106(&value.resume_bound), depth + 1));
                pending.push((Walk::W104(&value.call_kind), depth + 1));
                pending.push((Walk::W103(&value.storage_life), depth + 1));
                pending.push((Walk::W101(&value.source_scope), depth + 1));
                pending.push((Walk::W98(&value.retained_return_sources), depth + 1));
                pending.push((Walk::W96(&value.dyn_instance), depth + 1));
            }
            Walk::W29(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W30(child), depth));
                }
            }
            Walk::W30(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    6,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W94(&value.fields), depth + 1));
                pending.push((Walk::W93(&value.teardown), depth + 1));
                pending.push((Walk::W92(&value.kind), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
                pending.push((Walk::W9(&value.id), depth + 1));
            }
            Walk::W31(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W32(child), depth));
                }
            }
            Walk::W32(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    5,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W85(&value.fields), depth + 1));
                pending.push((Walk::W83(&value.parameters), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
                pending.push((Walk::W9(&value.id), depth + 1));
            }
            Walk::W33(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W34(child), depth));
                }
            }
            Walk::W34(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    5,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W81(&value.arguments), depth + 1));
                pending.push((Walk::W79(&value.owner), depth + 1));
                pending.push((Walk::W9(&value.template), depth + 1));
                pending.push((Walk::W1(&value.template_module), depth + 1));
                pending.push((Walk::W9(&value.aggregate), depth + 1));
            }
            Walk::W35(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W36(child), depth));
                }
            }
            Walk::W36(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W40(&value.variants), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
                pending.push((Walk::W9(&value.id), depth + 1));
            }
            Walk::W37(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W38(child), depth));
                }
            }
            Walk::W38(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W39(&value.span), depth + 1));
                pending.push((Walk::W10(&value.feature), depth + 1));
            }
            Walk::W39(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W11(child), depth));
                }
            }
            Walk::W40(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W41(child), depth));
                }
            }
            Walk::W41(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W42(&value.1), depth + 1));
                pending.push((Walk::W10(&value.0), depth + 1));
            }
            Walk::W42(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W43(child), depth));
                }
            }
            Walk::W43(value) => {
                meter.node(depth)?;
                match value {
                    Type::Dyn { interface } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W44(interface), depth + 1));
                    }
                    Type::OwnedDyn { interface } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W44(interface), depth + 1));
                    }
                    Type::BorrowedStorage { storage, life } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W45(life), depth + 1));
                        pending.push((Walk::W43(storage), depth + 1));
                    }
                    Type::Unit => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::Bool => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::I32 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::I64 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::U64 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::Usize => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::F32 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::F64 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::U8 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::U16 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::U32 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::I8 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::I16 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::Isize => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::ByteBuffer => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::OwnedStore { element } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(element), depth + 1));
                    }
                    Type::ForeignHandle => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::SyncCell(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W46(value_0), depth + 1));
                    }
                    Type::Mutex { payload } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(payload), depth + 1));
                    }
                    Type::MutexGuard {
                        payload,
                        retained_life,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W47(retained_life), depth + 1));
                        pending.push((Walk::W43(payload), depth + 1));
                    }
                    Type::Registration { environment } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(environment), depth + 1));
                    }
                    Type::AsyncRegistration { environment } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(environment), depth + 1));
                    }
                    Type::TaskI32 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::Task {
                        result,
                        retained_life,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W47(retained_life), depth + 1));
                        pending.push((Walk::W43(result), depth + 1));
                    }
                    Type::CWideString => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Type::ForeignCallback { parameters, result } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(result), depth + 1));
                        pending.push((Walk::W42(parameters), depth + 1));
                    }
                    Type::Tuple(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W42(value_0), depth + 1));
                    }
                    Type::Array { element, length } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(length), depth + 1));
                        pending.push((Walk::W43(element), depth + 1));
                    }
                    Type::Slice { element } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(element), depth + 1));
                    }
                    Type::Named(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    Type::Record { name, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W48(fields), depth + 1));
                        pending.push((Walk::W10(name), depth + 1));
                    }
                    Type::Package { name, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W48(fields), depth + 1));
                        pending.push((Walk::W10(name), depth + 1));
                    }
                    Type::Enum { name, variants } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W40(variants), depth + 1));
                        pending.push((Walk::W10(name), depth + 1));
                    }
                    Type::ErrorUnion { error, result } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(result), depth + 1));
                        pending.push((Walk::W43(error), depth + 1));
                    }
                    Type::Address { mutable, inner } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(inner), depth + 1));
                        pending.push((Walk::W50(mutable), depth + 1));
                    }
                    Type::Managed(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    Type::Desc(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    Type::FunctionPointer { parameters, result } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(result), depth + 1));
                        pending.push((Walk::W42(parameters), depth + 1));
                    }
                    Type::BorrowedFunctionPointer {
                        parameters,
                        result,
                        life,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W51(life), depth + 1));
                        pending.push((Walk::W43(result), depth + 1));
                        pending.push((Walk::W42(parameters), depth + 1));
                    }
                    Type::Closure(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W52(value_0), depth + 1));
                    }
                    Type::Opaque { identity, hidden } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(hidden), depth + 1));
                        pending.push((Walk::W9(identity), depth + 1));
                    }
                    Type::Unsupported(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                }
            }
            Walk::W44(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.index), depth + 1));
                pending.push((Walk::W1(&value.module), depth + 1));
            }
            Walk::W45(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W72(&value.nominal_arguments), depth + 1));
                pending.push((Walk::W70(&value.retained), depth + 1));
                pending.push((Walk::W76(&value.slots), depth + 1));
                pending.push((Walk::W26(&value.binder_count), depth + 1));
            }
            Walk::W46(value) => {
                meter.node(depth)?;
                match value {
                    SyncCellKind::MutexI32 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SyncCellKind::AtomicI32 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SyncCellKind::AtomicI64 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SyncCellKind::AtomicU64 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SyncCellKind::AtomicUsize => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W47(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W72(&value.nominal_arguments), depth + 1));
                pending.push((Walk::W70(&value.retained), depth + 1));
                pending.push((Walk::W68(&value.ports), depth + 1));
                pending.push((Walk::W26(&value.binder_count), depth + 1));
            }
            Walk::W48(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W49(child), depth));
                }
            }
            Walk::W49(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W43(&value.1), depth + 1));
                pending.push((Walk::W10(&value.0), depth + 1));
            }
            Walk::W50(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W51(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    5,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W57(&value.classes), depth + 1));
                pending.push((Walk::W55(&value.static_bindings), depth + 1));
                pending.push((Walk::W53(&value.bindings), depth + 1));
                pending.push((Walk::W26(&value.invocation_binder_count), depth + 1));
                pending.push((Walk::W26(&value.fixed_binder_count), depth + 1));
            }
            Walk::W52(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W53(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W54(child), depth));
                }
            }
            Walk::W54(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W60(&value.binder), depth + 1));
                pending.push((Walk::W56(&value.position), depth + 1));
            }
            Walk::W55(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W56(child), depth));
                }
            }
            Walk::W56(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W66(&value.path), depth + 1));
                pending.push((Walk::W61(&value.base), depth + 1));
            }
            Walk::W57(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W58(child), depth));
                }
            }
            Walk::W58(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W60(&value.binder), depth + 1));
                pending.push((Walk::W59(&value.position), depth + 1));
            }
            Walk::W59(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.class), depth + 1));
                pending.push((Walk::W62(&value.path), depth + 1));
                pending.push((Walk::W61(&value.base), depth + 1));
            }
            Walk::W60(value) => {
                meter.node(depth)?;
                match value {
                    StorageBinder::Fixed(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    StorageBinder::Invocation(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                }
            }
            Walk::W61(value) => {
                meter.node(depth)?;
                match value {
                    StorageBase::Parameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    StorageBase::Capture(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    StorageBase::Receiver => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    StorageBase::Result => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W62(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W63(child), depth));
                }
            }
            Walk::W63(value) => {
                meter.node(depth)?;
                match value {
                    LifeTypeProjection::Place(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W64(value_0), depth + 1));
                    }
                    LifeTypeProjection::TypeArgument(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    LifeTypeProjection::Port(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    LifeTypeProjection::AssociatedType(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    LifeTypeProjection::Pointee => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    LifeTypeProjection::Element => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    LifeTypeProjection::Parameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    LifeTypeProjection::Result => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W64(value) => {
                meter.node(depth)?;
                match value {
                    PlaceProjection::Field(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    PlaceProjection::Variant(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W65(value_0), depth + 1));
                    }
                    PlaceProjection::ErrorArm(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(value_0), depth + 1));
                    }
                }
            }
            Walk::W65(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.index), depth + 1));
                pending.push((Walk::W26(&value.variant), depth + 1));
            }
            Walk::W66(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W67(child), depth));
                }
            }
            Walk::W67(value) => {
                meter.node(depth)?;
                match value {
                    ContractProjection::Place(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W64(value_0), depth + 1));
                    }
                    ContractProjection::RetainedPort(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    ContractProjection::RetainedClass(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    ContractProjection::Capture(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                }
            }
            Walk::W68(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W69(child), depth));
                }
            }
            Walk::W69(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.binder), depth + 1));
                pending.push((Walk::W75(&value.mode), depth + 1));
                pending.push((Walk::W43(&value.leaf), depth + 1));
            }
            Walk::W70(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W71(child), depth));
                }
            }
            Walk::W71(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.binder), depth + 1));
                pending.push((Walk::W26(&value.class), depth + 1));
                pending.push((Walk::W62(&value.path), depth + 1));
            }
            Walk::W72(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W73(child), depth));
                }
            }
            Walk::W73(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.binder), depth + 1));
                pending.push((Walk::W26(&value.argument), depth + 1));
                pending.push((Walk::W74(&value.declaration), depth + 1));
                pending.push((Walk::W62(&value.path), depth + 1));
            }
            Walk::W74(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W50(&value.compiler), depth + 1));
                pending.push((Walk::W9(&value.declaration), depth + 1));
                pending.push((Walk::W1(&value.module), depth + 1));
            }
            Walk::W75(value) => {
                meter.node(depth)?;
                match value {
                    StoredBorrowMode::SharedAddress => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    StoredBorrowMode::ExclusiveAddress => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    StoredBorrowMode::ReadOnlyDescription => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    StoredBorrowMode::WritableDescription => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W76(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W77(child), depth));
                }
            }
            Walk::W77(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W75(&value.mode), depth + 1));
                pending.push((Walk::W26(&value.binder), depth + 1));
                pending.push((Walk::W78(&value.path), depth + 1));
            }
            Walk::W78(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W64(child), depth));
                }
            }
            Walk::W79(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W80(child), depth));
                }
            }
            Walk::W80(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W9(&value.function), depth + 1));
                pending.push((Walk::W1(&value.module), depth + 1));
            }
            Walk::W81(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W82(child), depth));
                }
            }
            Walk::W82(value) => {
                meter.node(depth)?;
                match value {
                    CoreAggregateArgument::Concrete(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    CoreAggregateArgument::Parameter { index } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(index), depth + 1));
                    }
                }
            }
            Walk::W83(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W84(child), depth));
                }
            }
            Walk::W84(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W91(&value.bound), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
                pending.push((Walk::W26(&value.index), depth + 1));
            }
            Walk::W85(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W86(child), depth));
                }
            }
            Walk::W86(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    5,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W87(&value.ty), depth + 1));
                pending.push((Walk::W50(&value.manages), depth + 1));
                pending.push((Walk::W50(&value.public), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
            }
            Walk::W87(value) => {
                meter.node(depth)?;
                match value {
                    CoreAggregateTypeRecipe::Concrete(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    CoreAggregateTypeRecipe::Parameter { index } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(index), depth + 1));
                    }
                    CoreAggregateTypeRecipe::Store { element } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W87(element), depth + 1));
                    }
                    CoreAggregateTypeRecipe::Registration { environment } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W87(environment), depth + 1));
                    }
                    CoreAggregateTypeRecipe::AsyncRegistration { environment } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W87(environment), depth + 1));
                    }
                    CoreAggregateTypeRecipe::Task {
                        result,
                        binder_count,
                        ports,
                        retained,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W70(retained), depth + 1));
                        pending.push((Walk::W88(ports), depth + 1));
                        pending.push((Walk::W26(binder_count), depth + 1));
                        pending.push((Walk::W87(result), depth + 1));
                    }
                    CoreAggregateTypeRecipe::Apply {
                        module,
                        template,
                        arguments,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W90(arguments), depth + 1));
                        pending.push((Walk::W9(template), depth + 1));
                        pending.push((Walk::W1(module), depth + 1));
                    }
                }
            }
            Walk::W88(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W89(child), depth));
                }
            }
            Walk::W89(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.binder), depth + 1));
                pending.push((Walk::W50(&value.mutable), depth + 1));
                pending.push((Walk::W87(&value.pointee), depth + 1));
            }
            Walk::W90(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W87(child), depth));
                }
            }
            Walk::W91(value) => {
                meter.node(depth)?;
                match value {
                    CoreTypeBound::Value => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreTypeBound::SyncDrop => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreTypeBound::StoreElement => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W92(value) => {
                meter.node(depth)?;
                match value {
                    AggregateKind::Struct => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    AggregateKind::Package => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    AggregateKind::Resource => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W93(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W10(child), depth));
                }
            }
            Walk::W94(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W95(child), depth));
                }
            }
            Walk::W95(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    5,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W50(&value.manages), depth + 1));
                pending.push((Walk::W50(&value.public), depth + 1));
                pending.push((Walk::W43(&value.ty), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
            }
            Walk::W96(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W97(child), depth));
                }
            }
            Walk::W97(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    6,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W212(&value.method_actuals), depth + 1));
                pending.push((Walk::W212(&value.declaration_actuals), depth + 1));
                pending.push((Walk::W211(&value.receiver), depth + 1));
                pending.push((Walk::W210(&value.definition), depth + 1));
                pending.push((Walk::W208(&value.header_embedding), depth + 1));
                pending.push((Walk::W207(&value.scope), depth + 1));
            }
            Walk::W98(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W99(child), depth));
                }
            }
            Walk::W99(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W100(child), depth));
                }
            }
            Walk::W100(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W205(&value.sources), depth + 1));
                pending.push((Walk::W56(&value.position), depth + 1));
            }
            Walk::W101(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W102(child), depth));
                }
            }
            Walk::W102(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W188(&value.inputs), depth + 1));
                pending.push((Walk::W184(&value.parameters), depth + 1));
                pending.push((Walk::W183(&value.identity), depth + 1));
                pending.push((Walk::W74(&value.declaration), depth + 1));
            }
            Walk::W103(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W51(child), depth));
                }
            }
            Walk::W104(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W105(child), depth));
                }
            }
            Walk::W105(value) => {
                meter.node(depth)?;
                match value {
                    CallKind::Direct => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CallKind::Suspend => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W106(value) => {
                meter.node(depth)?;
                match value {
                    TaskResumeBound::Local => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    TaskResumeBound::Transfer => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W107(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W108(child), depth));
                }
            }
            Walk::W108(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W190(&value.result_life), depth + 1));
                pending.push((Walk::W204(&value.parameter_lives), depth + 1));
                pending.push((Walk::W151(&value.binders), depth + 1));
            }
            Walk::W109(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W110(child), depth));
                }
            }
            Walk::W110(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W202(&value.bindings), depth + 1));
                pending.push((Walk::W26(&value.index), depth + 1));
            }
            Walk::W111(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W112(child), depth));
                }
            }
            Walk::W112(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W199(&value.exits), depth + 1));
                pending.push((Walk::W197(&value.entry), depth + 1));
                pending.push((Walk::W26(&value.parameter), depth + 1));
                pending.push((Walk::W196(&value.protocol), depth + 1));
            }
            Walk::W113(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W114(child), depth));
                }
            }
            Walk::W114(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    5,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W43(&value.result), depth + 1));
                pending.push((Walk::W42(&value.parameters), depth + 1));
                pending.push((Walk::W182(&value.kind), depth + 1));
                pending.push((Walk::W192(&value.source_life), depth + 1));
                pending.push((Walk::W103(&value.life), depth + 1));
            }
            Walk::W115(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W116(child), depth));
                }
            }
            Walk::W116(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    11,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W43(&value.result), depth + 1));
                pending.push((Walk::W42(&value.parameters), depth + 1));
                pending.push((Walk::W182(&value.kind), depth + 1));
                pending.push((Walk::W50(&value.thread_callable), depth + 1));
                pending.push((Walk::W50(&value.transfer), depth + 1));
                pending.push((Walk::W50(&value.closed_environment), depth + 1));
                pending.push((Walk::W10(&value.generic_name), depth + 1));
                pending.push((Walk::W26(&value.generic_index), depth + 1));
                pending.push((Walk::W26(&value.index), depth + 1));
                pending.push((Walk::W180(&value.source_life), depth + 1));
                pending.push((Walk::W103(&value.life), depth + 1));
            }
            Walk::W117(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W118(child), depth));
                }
            }
            Walk::W118(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    6,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W178(&value.scoped_callback), depth + 1));
                pending.push((Walk::W50(&value.thread_driver_transfer), depth + 1));
                pending.push((Walk::W50(&value.static_callbacks), depth + 1));
                pending.push((Walk::W177(&value.noescape_parameters), depth + 1));
                pending.push((Walk::W10(&value.symbol), depth + 1));
                pending.push((Walk::W10(&value.abi), depth + 1));
            }
            Walk::W119(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W120(child), depth));
                }
            }
            Walk::W120(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W121(&value.mode), depth + 1));
                pending.push((Walk::W43(&value.ty), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
            }
            Walk::W121(value) => {
                meter.node(depth)?;
                match value {
                    PassingMode::Copy => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    PassingMode::Move => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    PassingMode::Describe => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    PassingMode::Create => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    PassingMode::Give => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    PassingMode::Borrow => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    PassingMode::BorrowMut => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    PassingMode::Infer => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W122(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W125(&value.statements), depth + 1));
            }
            Walk::W123(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W124(child), depth));
                }
            }
            Walk::W124(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W50(&value.implicit), depth + 1));
                pending.push((Walk::W121(&value.mode), depth + 1));
                pending.push((Walk::W43(&value.ty), depth + 1));
            }
            Walk::W125(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W126(child), depth));
                }
            }
            Walk::W126(value) => {
                meter.remember(value.id)?;
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W127(&value.kind), depth + 1));
                pending.push((Walk::W9(&value.id), depth + 1));
            }
            Walk::W127(value) => {
                meter.node(depth)?;
                match value {
                    CoreStatementKind::Binding {
                        name,
                        mutable,
                        ty,
                        initializer,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W129(initializer), depth + 1));
                        pending.push((Walk::W128(ty), depth + 1));
                        pending.push((Walk::W50(mutable), depth + 1));
                        pending.push((Walk::W10(name), depth + 1));
                    }
                    CoreStatementKind::TupleBinding {
                        pattern,
                        mutable,
                        ty,
                        initializer,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(initializer), depth + 1));
                        pending.push((Walk::W128(ty), depth + 1));
                        pending.push((Walk::W50(mutable), depth + 1));
                        pending.push((Walk::W131(pattern), depth + 1));
                    }
                    CoreStatementKind::Assign { target, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value), depth + 1));
                        pending.push((Walk::W130(target), depth + 1));
                    }
                    CoreStatementKind::Expression(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value_0), depth + 1));
                    }
                    CoreStatementKind::Return(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W129(value_0), depth + 1));
                    }
                    CoreStatementKind::If {
                        condition,
                        then_body,
                        else_body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W132(else_body), depth + 1));
                        pending.push((Walk::W122(then_body), depth + 1));
                        pending.push((Walk::W130(condition), depth + 1));
                    }
                    CoreStatementKind::IfLet {
                        pattern,
                        value,
                        then_body,
                        else_body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W132(else_body), depth + 1));
                        pending.push((Walk::W122(then_body), depth + 1));
                        pending.push((Walk::W130(value), depth + 1));
                        pending.push((Walk::W131(pattern), depth + 1));
                    }
                    CoreStatementKind::While { condition, body } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(body), depth + 1));
                        pending.push((Walk::W130(condition), depth + 1));
                    }
                    CoreStatementKind::WhileLet {
                        pattern,
                        value,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(body), depth + 1));
                        pending.push((Walk::W130(value), depth + 1));
                        pending.push((Walk::W131(pattern), depth + 1));
                    }
                    CoreStatementKind::ForRange {
                        name,
                        start,
                        end,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(body), depth + 1));
                        pending.push((Walk::W130(end), depth + 1));
                        pending.push((Walk::W130(start), depth + 1));
                        pending.push((Walk::W10(name), depth + 1));
                    }
                    CoreStatementKind::Match { value, arms } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W133(arms), depth + 1));
                        pending.push((Walk::W130(value), depth + 1));
                    }
                    CoreStatementKind::Block(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(value_0), depth + 1));
                    }
                    CoreStatementKind::Defer(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(value_0), depth + 1));
                    }
                    CoreStatementKind::Unsafe(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(value_0), depth + 1));
                    }
                    CoreStatementKind::Break => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreStatementKind::Continue => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreStatementKind::Unsupported(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CoreStatementKind::ForEach {
                        pattern,
                        iterator,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(body), depth + 1));
                        pending.push((Walk::W130(iterator), depth + 1));
                        pending.push((Walk::W131(pattern), depth + 1));
                    }
                    CoreStatementKind::CompoundAssign { target, op, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value), depth + 1));
                        pending.push((Walk::W135(op), depth + 1));
                        pending.push((Walk::W130(target), depth + 1));
                    }
                    CoreStatementKind::Yield(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value_0), depth + 1));
                    }
                    CoreStatementKind::Loop(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(value_0), depth + 1));
                    }
                    CoreStatementKind::With { name, value, body } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(body), depth + 1));
                        pending.push((Walk::W130(value), depth + 1));
                        pending.push((Walk::W10(name), depth + 1));
                    }
                    CoreStatementKind::CompileTime(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W126(value_0), depth + 1));
                    }
                }
            }
            Walk::W128(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W43(child), depth));
                }
            }
            Walk::W129(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W130(child), depth));
                }
            }
            Walk::W130(value) => {
                meter.remember(value.id)?;
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W136(&value.kind), depth + 1));
                pending.push((Walk::W9(&value.id), depth + 1));
            }
            Walk::W131(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W173(&value.kind), depth + 1));
            }
            Walk::W132(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W122(child), depth));
                }
            }
            Walk::W133(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W134(child), depth));
                }
            }
            Walk::W134(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W122(&value.body), depth + 1));
                pending.push((Walk::W131(&value.pattern), depth + 1));
            }
            Walk::W135(value) => {
                meter.node(depth)?;
                match value {
                    BinaryOp::Add => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::Subtract => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::Multiply => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::Divide => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::Remainder => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::AddWrapping => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::SubtractWrapping => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::MultiplyWrapping => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::Equal => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::NotEqual => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::Less => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::LessEqual => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::Greater => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::GreaterEqual => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::And => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::Or => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::BitAnd => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::BitOr => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::BitXor => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::ShiftLeft => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    BinaryOp::ShiftRight => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W136(value) => {
                meter.node(depth)?;
                match value {
                    CoreExprKind::MakeDynBorrow {
                        interface,
                        vtable,
                        source,
                        mutable,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(mutable), depth + 1));
                        pending.push((Walk::W130(source), depth + 1));
                        pending.push((Walk::W137(vtable), depth + 1));
                        pending.push((Walk::W44(interface), depth + 1));
                    }
                    CoreExprKind::MakeDynOwned {
                        interface,
                        vtable,
                        source,
                        route,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W138(route), depth + 1));
                        pending.push((Walk::W130(source), depth + 1));
                        pending.push((Walk::W137(vtable), depth + 1));
                        pending.push((Walk::W44(interface), depth + 1));
                    }
                    CoreExprKind::CallDynSlot {
                        callee,
                        interface,
                        slot,
                        arguments,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W139(arguments), depth + 1));
                        pending.push((Walk::W26(slot), depth + 1));
                        pending.push((Walk::W44(interface), depth + 1));
                        pending.push((Walk::W130(callee), depth + 1));
                    }
                    CoreExprKind::Literal(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W140(value_0), depth + 1));
                    }
                    CoreExprKind::Ascribed { ty, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value), depth + 1));
                        pending.push((Walk::W43(ty), depth + 1));
                    }
                    CoreExprKind::Integer(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CoreExprKind::Float(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CoreExprKind::NumericConvert { value, target } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(target), depth + 1));
                        pending.push((Walk::W130(value), depth + 1));
                    }
                    CoreExprKind::IntegerConvert { value, target } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(target), depth + 1));
                        pending.push((Walk::W130(value), depth + 1));
                    }
                    CoreExprKind::Bool(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(value_0), depth + 1));
                    }
                    CoreExprKind::String(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CoreExprKind::Unit => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreExprKind::Name(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CoreExprKind::Unary { op, operand } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(operand), depth + 1));
                        pending.push((Walk::W141(op), depth + 1));
                    }
                    CoreExprKind::Binary { op, left, right } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(right), depth + 1));
                        pending.push((Walk::W130(left), depth + 1));
                        pending.push((Walk::W135(op), depth + 1));
                    }
                    CoreExprKind::TypeApply {
                        callee,
                        type_arguments,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W42(type_arguments), depth + 1));
                        pending.push((Walk::W130(callee), depth + 1));
                    }
                    CoreExprKind::Call { callee, arguments } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W139(arguments), depth + 1));
                        pending.push((Walk::W130(callee), depth + 1));
                    }
                    CoreExprKind::Tuple(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W139(value_0), depth + 1));
                    }
                    CoreExprKind::Array(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W139(value_0), depth + 1));
                    }
                    CoreExprKind::Index { object, index } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(index), depth + 1));
                        pending.push((Walk::W130(object), depth + 1));
                    }
                    CoreExprKind::Slice { object, start, end } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W142(end), depth + 1));
                        pending.push((Walk::W142(start), depth + 1));
                        pending.push((Walk::W130(object), depth + 1));
                    }
                    CoreExprKind::Member {
                        object,
                        member,
                        owner,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W128(owner), depth + 1));
                        pending.push((Walk::W10(member), depth + 1));
                        pending.push((Walk::W130(object), depth + 1));
                    }
                    CoreExprKind::ReceiverMember(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CoreExprKind::Record { ty, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W143(fields), depth + 1));
                        pending.push((Walk::W43(ty), depth + 1));
                    }
                    CoreExprKind::RecordUpdate { ty, base, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W143(fields), depth + 1));
                        pending.push((Walk::W130(base), depth + 1));
                        pending.push((Walk::W128(ty), depth + 1));
                    }
                    CoreExprKind::Sequence { bindings, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value), depth + 1));
                        pending.push((Walk::W143(bindings), depth + 1));
                    }
                    CoreExprKind::CompileTime(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value_0), depth + 1));
                    }
                    CoreExprKind::Try(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value_0), depth + 1));
                    }
                    CoreExprKind::Closure {
                        move_capture,
                        parameters,
                        return_type,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(body), depth + 1));
                        pending.push((Walk::W128(return_type), depth + 1));
                        pending.push((Walk::W145(parameters), depth + 1));
                        pending.push((Walk::W50(move_capture), depth + 1));
                    }
                    CoreExprKind::If {
                        condition,
                        then_value,
                        else_value,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(else_value), depth + 1));
                        pending.push((Walk::W130(then_value), depth + 1));
                        pending.push((Walk::W130(condition), depth + 1));
                    }
                    CoreExprKind::Unsupported(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CoreExprKind::Spread(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value_0), depth + 1));
                    }
                    CoreExprKind::Group(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value_0), depth + 1));
                    }
                    CoreExprKind::Character(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CoreExprKind::Range { start, end } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(end), depth + 1));
                        pending.push((Walk::W130(start), depth + 1));
                    }
                    CoreExprKind::Pipeline { input, target } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(target), depth + 1));
                        pending.push((Walk::W130(input), depth + 1));
                    }
                    CoreExprKind::RecordConstructor { definition, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W143(fields), depth + 1));
                        pending.push((Walk::W147(definition), depth + 1));
                    }
                    CoreExprKind::Splice(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value_0), depth + 1));
                    }
                    CoreExprKind::CompileTimeBlock(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W122(value_0), depth + 1));
                    }
                    CoreExprKind::SyntaxQuote { category, template } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W150(template), depth + 1));
                        pending.push((Walk::W149(category), depth + 1));
                    }
                    CoreExprKind::SyntaxIsland { path, tree } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W152(tree), depth + 1));
                        pending.push((Walk::W151(path), depth + 1));
                    }
                    CoreExprKind::GenericApply { callee, arguments } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W153(arguments), depth + 1));
                        pending.push((Walk::W130(callee), depth + 1));
                    }
                }
            }
            Walk::W137(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.index), depth + 1));
                pending.push((Walk::W1(&value.module), depth + 1));
            }
            Walk::W138(value) => {
                meter.node(depth)?;
                match value {
                    DynOwnedStorage::Heap => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W139(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W130(child), depth));
                }
            }
            Walk::W140(value) => {
                meter.node(depth)?;
                match value {
                    Constant::Unit => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Constant::Bool(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(value_0), depth + 1));
                    }
                    Constant::I32(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W165(value_0), depth + 1));
                    }
                    Constant::I64(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W166(value_0), depth + 1));
                    }
                    Constant::U64(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W167(value_0), depth + 1));
                    }
                    Constant::Usize(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W167(value_0), depth + 1));
                    }
                    Constant::F32(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    Constant::F64(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W167(value_0), depth + 1));
                    }
                    Constant::U8(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W168(value_0), depth + 1));
                    }
                    Constant::U16(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W169(value_0), depth + 1));
                    }
                    Constant::U32(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    Constant::I8(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W170(value_0), depth + 1));
                    }
                    Constant::I16(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W171(value_0), depth + 1));
                    }
                    Constant::Isize(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W166(value_0), depth + 1));
                    }
                    Constant::NullHandle => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Constant::NullWideString => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    Constant::WideString(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W172(value_0), depth + 1));
                    }
                }
            }
            Walk::W141(value) => {
                meter.node(depth)?;
                match value {
                    UnaryOp::Negate => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    UnaryOp::Not => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    UnaryOp::BitNot => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    UnaryOp::Dereference => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    UnaryOp::Borrow => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    UnaryOp::BorrowMut => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    UnaryOp::Give => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W142(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W130(child), depth));
                }
            }
            Walk::W143(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W144(child), depth));
                }
            }
            Walk::W144(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W130(&value.1), depth + 1));
                pending.push((Walk::W10(&value.0), depth + 1));
            }
            Walk::W145(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W146(child), depth));
                }
            }
            Walk::W146(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W121(&value.mode), depth + 1));
                pending.push((Walk::W128(&value.ty), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
            }
            Walk::W147(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W148(child), depth));
                }
            }
            Walk::W148(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.index), depth + 1));
                pending.push((Walk::W1(&value.module), depth + 1));
            }
            Walk::W149(value) => {
                meter.node(depth)?;
                match value {
                    CoreSyntaxCategory::Expr => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreSyntaxCategory::Type => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreSyntaxCategory::StmtList => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreSyntaxCategory::ItemList => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W150(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W160(&value.name_sites), depth + 1));
                pending.push((Walk::W158(&value.holes), depth + 1));
                pending.push((Walk::W152(&value.tree), depth + 1));
            }
            Walk::W151(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W10(child), depth));
                }
            }
            Walk::W152(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W156(&value.children), depth + 1));
                pending.push((Walk::W155(&value.delimiter), depth + 1));
            }
            Walk::W153(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W154(child), depth));
                }
            }
            Walk::W154(value) => {
                meter.node(depth)?;
                match value {
                    CoreGenericArgument::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    CoreGenericArgument::Const(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W130(value_0), depth + 1));
                    }
                    CoreGenericArgument::Lifetime(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                }
            }
            Walk::W155(value) => {
                meter.node(depth)?;
                match value {
                    CoreSyntaxDelimiter::Parenthesis => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreSyntaxDelimiter::Bracket => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreSyntaxDelimiter::Brace => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W156(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W157(child), depth));
                }
            }
            Walk::W157(value) => {
                meter.node(depth)?;
                match value {
                    CoreSyntaxTreeEntry::Token { text, span } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W11(span), depth + 1));
                        pending.push((Walk::W10(text), depth + 1));
                    }
                    CoreSyntaxTreeEntry::Tree(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W152(value_0), depth + 1));
                    }
                    CoreSyntaxTreeEntry::Hole { index, span } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W11(span), depth + 1));
                        pending.push((Walk::W26(index), depth + 1));
                    }
                }
            }
            Walk::W158(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W159(child), depth));
                }
            }
            Walk::W159(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W130(&value.value), depth + 1));
                pending.push((Walk::W162(&value.kind), depth + 1));
            }
            Walk::W160(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W161(child), depth));
                }
            }
            Walk::W161(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.token_index), depth + 1));
                pending.push((Walk::W11(&value.span), depth + 1));
            }
            Walk::W162(value) => {
                meter.node(depth)?;
                match value {
                    CoreQuoteHoleKind::Expr => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreQuoteHoleKind::Type => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreQuoteHoleKind::StmtList => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreQuoteHoleKind::ItemList => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreQuoteHoleKind::Ident { namespace, role } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W164(role), depth + 1));
                        pending.push((Walk::W163(namespace), depth + 1));
                    }
                }
            }
            Walk::W163(value) => {
                meter.node(depth)?;
                match value {
                    CoreQuoteNameNamespace::Value => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreQuoteNameNamespace::Type => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreQuoteNameNamespace::Member => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreQuoteNameNamespace::Module => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreQuoteNameNamespace::Label => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W164(value) => {
                meter.node(depth)?;
                match value {
                    CoreQuoteNameRole::Binding => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CoreQuoteNameRole::Reference => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W165(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W166(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W167(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W168(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W169(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W170(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W171(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W172(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W169(child), depth));
                }
            }
            Walk::W173(value) => {
                meter.node(depth)?;
                match value {
                    CorePatternKind::Name(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CorePatternKind::Wildcard => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CorePatternKind::Tuple(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W174(value_0), depth + 1));
                    }
                    CorePatternKind::Record { ty, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W175(fields), depth + 1));
                        pending.push((Walk::W43(ty), depth + 1));
                    }
                    CorePatternKind::Reference { mutable, pattern } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W131(pattern), depth + 1));
                        pending.push((Walk::W50(mutable), depth + 1));
                    }
                    CorePatternKind::ErrorArm { is_error, pattern } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W131(pattern), depth + 1));
                        pending.push((Walk::W50(is_error), depth + 1));
                    }
                    CorePatternKind::Variant {
                        ty,
                        variant,
                        fields,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W174(fields), depth + 1));
                        pending.push((Walk::W10(variant), depth + 1));
                        pending.push((Walk::W43(ty), depth + 1));
                    }
                    CorePatternKind::Integer(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CorePatternKind::Bool(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(value_0), depth + 1));
                    }
                    CorePatternKind::Constructor {
                        definition,
                        variant,
                        fields,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W174(fields), depth + 1));
                        pending.push((Walk::W10(variant), depth + 1));
                        pending.push((Walk::W147(definition), depth + 1));
                    }
                    CorePatternKind::RecordConstructor { definition, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W175(fields), depth + 1));
                        pending.push((Walk::W147(definition), depth + 1));
                    }
                    CorePatternKind::Float(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CorePatternKind::String(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    CorePatternKind::Character(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                }
            }
            Walk::W174(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W131(child), depth));
                }
            }
            Walk::W175(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W176(child), depth));
                }
            }
            Walk::W176(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W131(&value.1), depth + 1));
                pending.push((Walk::W10(&value.0), depth + 1));
            }
            Walk::W177(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W26(child), depth));
                }
            }
            Walk::W178(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W179(child), depth));
                }
            }
            Walk::W179(value) => {
                meter.node(depth)?;
                match value {
                    ScopedCallbackRole::Register => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ScopedCallbackRole::Close => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ScopedCallbackRole::AsyncRegister => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ScopedCallbackRole::AsyncRequestClose => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ScopedCallbackRole::AsyncPollDrain => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ScopedCallbackRole::ThreadAsyncRegister => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ScopedCallbackRole::ThreadAsyncRequestClose => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ScopedCallbackRole::ThreadAsyncPollDrain => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W180(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W181(child), depth));
                }
            }
            Walk::W181(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    7,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W188(&value.inputs), depth + 1));
                pending.push((Walk::W186(&value.named_fixed), depth + 1));
                pending.push((Walk::W26(&value.bound_ordinal), depth + 1));
                pending.push((Walk::W26(&value.generic_parameter), depth + 1));
                pending.push((Walk::W184(&value.parameters), depth + 1));
                pending.push((Walk::W183(&value.identity), depth + 1));
                pending.push((Walk::W74(&value.declaration), depth + 1));
            }
            Walk::W182(value) => {
                meter.node(depth)?;
                match value {
                    CallableKind::FunctionPointer => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CallableKind::Fn => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CallableKind::FnMut => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    CallableKind::FnOnce => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W183(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W177(&value.generation_path), depth + 1));
                pending.push((Walk::W151(&value.declaration_path), depth + 1));
                pending.push((Walk::W151(&value.module_path), depth + 1));
                pending.push((Walk::W10(&value.package), depth + 1));
            }
            Walk::W184(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W185(child), depth));
                }
            }
            Walk::W185(value) => {
                meter.node(depth)?;
                match value {
                    LifeParameterKind::Type => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    LifeParameterKind::Const(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    LifeParameterKind::Lifetime => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W186(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W187(child), depth));
                }
            }
            Walk::W187(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.parameter), depth + 1));
                pending.push((Walk::W26(&value.fixed), depth + 1));
            }
            Walk::W188(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W189(child), depth));
                }
            }
            Walk::W189(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W191(&value.position), depth + 1));
                pending.push((Walk::W60(&value.header_binder), depth + 1));
                pending.push((Walk::W190(&value.source_parameter), depth + 1));
                pending.push((Walk::W26(&value.parameter), depth + 1));
            }
            Walk::W190(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W26(child), depth));
                }
            }
            Walk::W191(value) => {
                meter.node(depth)?;
                match value {
                    CoreClosureHeaderPosition::Borrow(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    CoreClosureHeaderPosition::Class(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W59(value_0), depth + 1));
                    }
                }
            }
            Walk::W192(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W193(child), depth));
                }
            }
            Walk::W193(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W194(&value.fixed), depth + 1));
                pending.push((Walk::W184(&value.parameters), depth + 1));
                pending.push((Walk::W183(&value.identity), depth + 1));
                pending.push((Walk::W74(&value.declaration), depth + 1));
            }
            Walk::W194(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W195(child), depth));
                }
            }
            Walk::W195(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W189(&value.input), depth + 1));
                pending.push((Walk::W26(&value.fixed), depth + 1));
            }
            Walk::W196(value) => {
                meter.node(depth)?;
                match value {
                    ProtocolKind::AsyncRegistrationV1 => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W197(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W198(child), depth));
                }
            }
            Walk::W198(value) => {
                meter.node(depth)?;
                match value {
                    ProtocolState::Open => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ProtocolState::Closing => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ProtocolState::Closed => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W199(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W200(child), depth));
                }
            }
            Walk::W200(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W197(&value.output), depth + 1));
                pending.push((Walk::W201(&value.exit), depth + 1));
                pending.push((Walk::W198(&value.input), depth + 1));
            }
            Walk::W201(value) => {
                meter.node(depth)?;
                match value {
                    ProtocolExit::Return => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ProtocolExit::ErrorOk => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ProtocolExit::ErrorErr => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ProtocolExit::PollPending => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    ProtocolExit::PollReady => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W202(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W203(child), depth));
                }
            }
            Walk::W203(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    6,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W177(&value.fields), depth + 1));
                pending.push((Walk::W43(&value.result), depth + 1));
                pending.push((Walk::W42(&value.parameters), depth + 1));
                pending.push((Walk::W182(&value.kind), depth + 1));
                pending.push((Walk::W10(&value.generic_name), depth + 1));
                pending.push((Walk::W26(&value.generic_index), depth + 1));
            }
            Walk::W204(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W190(child), depth));
                }
            }
            Walk::W205(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W206(child), depth));
                }
            }
            Walk::W206(value) => {
                meter.node(depth)?;
                match value {
                    StorageSource::Exact(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    StorageSource::PlaceAddress { place, mode } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W75(mode), depth + 1));
                        pending.push((Walk::W56(place), depth + 1));
                    }
                    StorageSource::OwnedView { head, leaf, mode } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W75(mode), depth + 1));
                        pending.push((Walk::W43(leaf), depth + 1));
                        pending.push((Walk::W56(head), depth + 1));
                    }
                    StorageSource::ReborrowedLeaf { source, head } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(head), depth + 1));
                        pending.push((Walk::W56(source), depth + 1));
                    }
                    StorageSource::Static { leaf, mode } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W75(mode), depth + 1));
                        pending.push((Walk::W43(leaf), depth + 1));
                    }
                }
            }
            Walk::W207(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.invocation_binder_count), depth + 1));
                pending.push((Walk::W26(&value.fixed_binder_count), depth + 1));
            }
            Walk::W208(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W209(child), depth));
                }
            }
            Walk::W209(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W60(&value.to), depth + 1));
                pending.push((Walk::W60(&value.from), depth + 1));
            }
            Walk::W210(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W9(&value.source), depth + 1));
                pending.push((Walk::W1(&value.module), depth + 1));
            }
            Walk::W211(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W214(&value.classes), depth + 1));
                pending.push((Walk::W45(&value.life), depth + 1));
                pending.push((Walk::W43(&value.ty), depth + 1));
            }
            Walk::W212(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W213(child), depth));
                }
            }
            Walk::W213(value) => {
                meter.node(depth)?;
                match value {
                    DynGenericActual::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    DynGenericActual::QualifiedType(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W211(value_0), depth + 1));
                    }
                    DynGenericActual::Const(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W140(value_0), depth + 1));
                    }
                    DynGenericActual::Life(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W60(value_0), depth + 1));
                    }
                }
            }
            Walk::W214(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W60(child), depth));
                }
            }
            Walk::W215(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W216(child), depth));
                }
            }
            Walk::W216(value) => {
                meter.node(depth)?;
                match value {
                    LifeNominalActual::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    LifeNominalActual::QualifiedType { ty, life } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W45(life), depth + 1));
                        pending.push((Walk::W43(ty), depth + 1));
                    }
                    LifeNominalActual::Const(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W140(value_0), depth + 1));
                    }
                    LifeNominalActual::Lifetime(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                }
            }
            Walk::W217(value) => {
                meter.node(depth)?;
                match value {
                    LifeNominalKind::Record => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    LifeNominalKind::Package => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    LifeNominalKind::Resource => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    LifeNominalKind::Enum => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W218(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W45(child), depth));
                }
            }
            Walk::W219(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W220(child), depth));
                }
            }
            Walk::W220(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W221(child), depth));
                }
            }
            Walk::W221(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    7,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W229(&value.associated), depth + 1));
                pending.push((Walk::W212(&value.actuals), depth + 1));
                pending.push((Walk::W74(&value.implementation), depth + 1));
                pending.push((Walk::W228(&value.application), depth + 1));
                pending.push((Walk::W211(&value.receiver), depth + 1));
                pending.push((Walk::W208(&value.to_context), depth + 1));
                pending.push((Walk::W207(&value.scope), depth + 1));
            }
            Walk::W222(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W223(child), depth));
                }
            }
            Walk::W223(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W55(&value.sources), depth + 1));
                pending.push((Walk::W26(&value.port), depth + 1));
            }
            Walk::W224(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W225(child), depth));
                }
            }
            Walk::W225(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W9(&value.function), depth + 1));
                pending.push((Walk::W1(&value.module), depth + 1));
                pending.push((Walk::W26(&value.slot), depth + 1));
            }
            Walk::W226(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W225(child), depth));
                }
            }
            Walk::W227(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W138(child), depth));
                }
            }
            Walk::W228(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W212(&value.actuals), depth + 1));
                pending.push((Walk::W74(&value.declaration), depth + 1));
            }
            Walk::W229(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W230(child), depth));
                }
            }
            Walk::W230(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W233(&value.value), depth + 1));
                pending.push((Walk::W232(&value.source), depth + 1));
                pending.push((Walk::W231(&value.member), depth + 1));
            }
            Walk::W231(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.ordinal), depth + 1));
                pending.push((Walk::W228(&value.application), depth + 1));
            }
            Walk::W232(value) => {
                meter.node(depth)?;
                match value {
                    DynAssociatedSource::TraitDefault {
                        declaration,
                        member,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(member), depth + 1));
                        pending.push((Walk::W74(declaration), depth + 1));
                    }
                    DynAssociatedSource::ImplOverride {
                        declaration,
                        member,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(member), depth + 1));
                        pending.push((Walk::W74(declaration), depth + 1));
                    }
                }
            }
            Walk::W233(value) => {
                meter.node(depth)?;
                match value {
                    DynAssociatedValue::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W211(value_0), depth + 1));
                    }
                    DynAssociatedValue::Constant { ty, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W140(value), depth + 1));
                        pending.push((Walk::W211(ty), depth + 1));
                    }
                }
            }
            Walk::W234(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W235(child), depth));
                }
            }
            Walk::W235(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W258(&value.effects), depth + 1));
                pending.push((Walk::W254(&value.access), depth + 1));
                pending.push((Walk::W55(&value.positions), depth + 1));
                pending.push((Walk::W50(&value.environment), depth + 1));
            }
            Walk::W236(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W228(child), depth));
                }
            }
            Walk::W237(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W238(child), depth));
                }
            }
            Walk::W238(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W214(&value.classes), depth + 1));
                pending.push((Walk::W45(&value.life), depth + 1));
                pending.push((Walk::W43(&value.ty), depth + 1));
                pending.push((Walk::W231(&value.member), depth + 1));
            }
            Walk::W239(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W240(child), depth));
                }
            }
            Walk::W240(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W140(&value.value), depth + 1));
                pending.push((Walk::W43(&value.ty), depth + 1));
                pending.push((Walk::W231(&value.member), depth + 1));
            }
            Walk::W241(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W242(child), depth));
                }
            }
            Walk::W242(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    10,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W245(&value.contract), depth + 1));
                pending.push((Walk::W103(&value.storage_life), depth + 1));
                pending.push((Walk::W105(&value.call_kind), depth + 1));
                pending.push((Walk::W121(&value.result_mode), depth + 1));
                pending.push((Walk::W43(&value.result), depth + 1));
                pending.push((Walk::W244(&value.parameter_modes), depth + 1));
                pending.push((Walk::W42(&value.parameters), depth + 1));
                pending.push((Walk::W243(&value.receiver), depth + 1));
                pending.push((Walk::W212(&value.actuals), depth + 1));
                pending.push((Walk::W231(&value.member), depth + 1));
            }
            Walk::W243(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W121(&value.mode), depth + 1));
                pending.push((Walk::W50(&value.mutable), depth + 1));
            }
            Walk::W244(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W121(child), depth));
                }
            }
            Walk::W245(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    9,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W262(&value.threads), depth + 1));
                pending.push((Walk::W260(&value.resume), depth + 1));
                pending.push((Walk::W257(&value.effects), depth + 1));
                pending.push((Walk::W234(&value.cleanup), depth + 1));
                pending.push((Walk::W255(&value.result), depth + 1));
                pending.push((Walk::W253(&value.call), depth + 1));
                pending.push((Walk::W252(&value.hold), depth + 1));
                pending.push((Walk::W249(&value.guarantees), depth + 1));
                pending.push((Walk::W246(&value.requires), depth + 1));
            }
            Walk::W246(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W247(child), depth));
                }
            }
            Walk::W247(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W248(child), depth));
                }
            }
            Walk::W248(value) => {
                meter.node(depth)?;
                match value {
                    DynRequirement::Read(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynRequirement::Write(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynRequirement::Manage(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynRequirement::Apart(value_0, value_1) => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_1), depth + 1));
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynRequirement::State {
                        position,
                        ty,
                        variant,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(variant), depth + 1));
                        pending.push((Walk::W43(ty), depth + 1));
                        pending.push((Walk::W56(position), depth + 1));
                    }
                }
            }
            Walk::W249(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W250(child), depth));
                }
            }
            Walk::W250(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W251(child), depth));
                }
            }
            Walk::W251(value) => {
                meter.node(depth)?;
                match value {
                    DynGuarantee::StorageSources(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W100(value_0), depth + 1));
                    }
                    DynGuarantee::ManageResult => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynGuarantee::State {
                        position,
                        ty,
                        variant,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(variant), depth + 1));
                        pending.push((Walk::W43(ty), depth + 1));
                        pending.push((Walk::W56(position), depth + 1));
                    }
                }
            }
            Walk::W252(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W55(child), depth));
                }
            }
            Walk::W253(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W254(child), depth));
                }
            }
            Walk::W254(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W55(&value.writes), depth + 1));
                pending.push((Walk::W55(&value.reads), depth + 1));
            }
            Walk::W255(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W256(child), depth));
                }
            }
            Walk::W256(value) => {
                meter.node(depth)?;
                match value {
                    DynResultSource::Value => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynResultSource::Create => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynResultSource::Give => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynResultSource::Borrow { sources, mutable } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(mutable), depth + 1));
                        pending.push((Walk::W205(sources), depth + 1));
                    }
                }
            }
            Walk::W257(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W258(child), depth));
                }
            }
            Walk::W258(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W259(child), depth));
                }
            }
            Walk::W259(value) => {
                meter.node(depth)?;
                match value {
                    DynEffect::Alloc => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffect::Io => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffect::Block => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffect::Suspend => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffect::Spawn => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffect::Ffi => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffect::Reenter => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffect::Panic => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffect::Call(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynEffect::Cleanup(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                }
            }
            Walk::W260(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W261(child), depth));
                }
            }
            Walk::W261(value) => {
                meter.node(depth)?;
                match value {
                    DynResumeBound::Local => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynResumeBound::Transfer => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W262(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W263(child), depth));
                }
            }
            Walk::W263(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W50(&value.share), depth + 1));
                pending.push((Walk::W50(&value.transfer), depth + 1));
            }
            Walk::W264(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W265(child), depth));
                }
            }
            Walk::W265(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W266(child), depth));
                }
            }
            Walk::W266(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    1,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W361(&value.occurrences), depth + 1));
            }
            Walk::W267(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W268(child), depth));
                }
            }
            Walk::W268(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W269(child), depth));
                }
            }
            Walk::W269(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W289(&value.arguments), depth + 1));
                pending.push((Walk::W74(&value.declaration), depth + 1));
            }
            Walk::W270(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W271(child), depth));
                }
            }
            Walk::W271(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W272(child), depth));
                }
            }
            Walk::W272(value) => {
                meter.node(depth)?;
                match value {
                    DynObligation::Structural { receiver, bound } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W357(bound), depth + 1));
                        pending.push((Walk::W283(receiver), depth + 1));
                    }
                    DynObligation::Trait {
                        receiver,
                        application,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W269(application), depth + 1));
                        pending.push((Walk::W283(receiver), depth + 1));
                    }
                    DynObligation::Callable {
                        receiver,
                        kind,
                        quantified_lifetimes,
                        signature,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W335(signature), depth + 1));
                        pending.push((Walk::W26(quantified_lifetimes), depth + 1));
                        pending.push((Walk::W182(kind), depth + 1));
                        pending.push((Walk::W283(receiver), depth + 1));
                    }
                    DynObligation::TypeEqual { left, right } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(right), depth + 1));
                        pending.push((Walk::W283(left), depth + 1));
                    }
                    DynObligation::ConstEqual { ty, left, right } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W291(right), depth + 1));
                        pending.push((Walk::W291(left), depth + 1));
                        pending.push((Walk::W283(ty), depth + 1));
                    }
                }
            }
            Walk::W273(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W274(child), depth));
                }
            }
            Walk::W274(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W275(child), depth));
                }
            }
            Walk::W275(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W271(&value.requirements), depth + 1));
                pending.push((Walk::W359(&value.value), depth + 1));
                pending.push((Walk::W26(&value.member), depth + 1));
                pending.push((Walk::W282(&value.carrier), depth + 1));
            }
            Walk::W276(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W277(child), depth));
                }
            }
            Walk::W277(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W278(child), depth));
                }
            }
            Walk::W278(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W358(&value.default_definition), depth + 1));
                pending.push((Walk::W335(&value.signature), depth + 1));
                pending.push((Walk::W26(&value.member), depth + 1));
                pending.push((Walk::W271(&value.requirements), depth + 1));
            }
            Walk::W279(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W280(child), depth));
                }
            }
            Walk::W280(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W281(child), depth));
                }
            }
            Walk::W281(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    7,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W335(&value.signature), depth + 1));
                pending.push((Walk::W334(&value.scope), depth + 1));
                pending.push((Walk::W26(&value.member), depth + 1));
                pending.push((Walk::W183(&value.identity), depth + 1));
                pending.push((Walk::W210(&value.id), depth + 1));
                pending.push((Walk::W332(&value.instances), depth + 1));
                pending.push((Walk::W271(&value.requirements), depth + 1));
            }
            Walk::W282(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W283(child), depth));
                }
            }
            Walk::W283(value) => {
                meter.node(depth)?;
                match value {
                    DynTypePattern::SelfType => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynTypePattern::MethodParameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynTypePattern::AssociatedProjection {
                        receiver,
                        application,
                        member,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(member), depth + 1));
                        pending.push((Walk::W269(application), depth + 1));
                        pending.push((Walk::W283(receiver), depth + 1));
                    }
                    DynTypePattern::Concrete(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    DynTypePattern::Parameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynTypePattern::Nominal(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W269(value_0), depth + 1));
                    }
                    DynTypePattern::Address {
                        mutable,
                        lifetime,
                        inner,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(inner), depth + 1));
                        pending.push((Walk::W293(lifetime), depth + 1));
                        pending.push((Walk::W50(mutable), depth + 1));
                    }
                    DynTypePattern::Tuple(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W294(value_0), depth + 1));
                    }
                    DynTypePattern::Array { element, length } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W291(length), depth + 1));
                        pending.push((Walk::W283(element), depth + 1));
                    }
                    DynTypePattern::Slice(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(value_0), depth + 1));
                    }
                    DynTypePattern::Managed(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(value_0), depth + 1));
                    }
                    DynTypePattern::Desc { lifetime, inner } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(inner), depth + 1));
                        pending.push((Walk::W293(lifetime), depth + 1));
                    }
                    DynTypePattern::OwnedStore(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(value_0), depth + 1));
                    }
                    DynTypePattern::ErrorUnion { error, result } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(result), depth + 1));
                        pending.push((Walk::W283(error), depth + 1));
                    }
                    DynTypePattern::Semantic { scope, ty } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(ty), depth + 1));
                        pending.push((Walk::W148(scope), depth + 1));
                    }
                }
            }
            Walk::W284(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W269(child), depth));
                }
            }
            Walk::W285(value) => {
                meter.node(depth)?;
                match value {
                    DynDeclarationKind::Trait => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynDeclarationKind::Impl => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W286(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W287(child), depth));
                }
            }
            Walk::W287(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W288(&value.kind), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
            }
            Walk::W288(value) => {
                meter.node(depth)?;
                match value {
                    DynMemberKind::Type => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynMemberKind::Constant => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynMemberKind::Method(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W184(value_0), depth + 1));
                    }
                }
            }
            Walk::W289(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W290(child), depth));
                }
            }
            Walk::W290(value) => {
                meter.node(depth)?;
                match value {
                    DynPatternActual::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(value_0), depth + 1));
                    }
                    DynPatternActual::Const(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W291(value_0), depth + 1));
                    }
                    DynPatternActual::Life(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W292(value_0), depth + 1));
                    }
                }
            }
            Walk::W291(value) => {
                meter.node(depth)?;
                match value {
                    DynConstPattern::IntegerLiteral(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    DynConstPattern::FloatLiteral(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    DynConstPattern::MethodParameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynConstPattern::Unary { op, value, ty } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(ty), depth + 1));
                        pending.push((Walk::W291(value), depth + 1));
                        pending.push((Walk::W141(op), depth + 1));
                    }
                    DynConstPattern::Binary {
                        op,
                        left,
                        right,
                        ty,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(ty), depth + 1));
                        pending.push((Walk::W291(right), depth + 1));
                        pending.push((Walk::W291(left), depth + 1));
                        pending.push((Walk::W135(op), depth + 1));
                    }
                    DynConstPattern::Convert { value, ty } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(ty), depth + 1));
                        pending.push((Walk::W291(value), depth + 1));
                    }
                    DynConstPattern::AssociatedProjection {
                        receiver,
                        application,
                        member,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(member), depth + 1));
                        pending.push((Walk::W269(application), depth + 1));
                        pending.push((Walk::W283(receiver), depth + 1));
                    }
                    DynConstPattern::Value(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W140(value_0), depth + 1));
                    }
                    DynConstPattern::Parameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                }
            }
            Walk::W292(value) => {
                meter.node(depth)?;
                match value {
                    DynLifePattern::DeclarationParameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynLifePattern::MethodParameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynLifePattern::Bound {
                        depth: bound_depth,
                        index,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(index), depth + 1));
                        pending.push((Walk::W26(bound_depth), depth + 1));
                    }
                    DynLifePattern::Invocation(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynLifePattern::ImplicitDeclaration(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                }
            }
            Walk::W293(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W292(child), depth));
                }
            }
            Walk::W294(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W283(child), depth));
                }
            }
            Walk::W295(value) => {
                meter.node(depth)?;
                match value {
                    SemanticTypeRecipe::Atom(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W43(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::Exact(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W296(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::Formal(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W297(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::Environment { scope, ordinal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(ordinal), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                    SemanticTypeRecipe::SelfType { scope } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                    SemanticTypeRecipe::Nominal {
                        definition,
                        arguments,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W299(arguments), depth + 1));
                        pending.push((Walk::W148(definition), depth + 1));
                    }
                    SemanticTypeRecipe::AssociatedType {
                        receiver,
                        application,
                        member,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(member), depth + 1));
                        pending.push((Walk::W301(application), depth + 1));
                        pending.push((Walk::W295(receiver), depth + 1));
                    }
                    SemanticTypeRecipe::Tuple(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W302(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::Array { element, length } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W303(length), depth + 1));
                        pending.push((Walk::W295(element), depth + 1));
                    }
                    SemanticTypeRecipe::Slice(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::Address {
                        mutable,
                        lifetime,
                        inner,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(inner), depth + 1));
                        pending.push((Walk::W304(lifetime), depth + 1));
                        pending.push((Walk::W50(mutable), depth + 1));
                    }
                    SemanticTypeRecipe::Desc { lifetime, inner } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(inner), depth + 1));
                        pending.push((Walk::W304(lifetime), depth + 1));
                    }
                    SemanticTypeRecipe::Managed(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::OwnedStore(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::Mutex(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::MutexGuard { payload, retained } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W305(retained), depth + 1));
                        pending.push((Walk::W295(payload), depth + 1));
                    }
                    SemanticTypeRecipe::Registration(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::AsyncRegistration(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::Task { result, retained } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W305(retained), depth + 1));
                        pending.push((Walk::W295(result), depth + 1));
                    }
                    SemanticTypeRecipe::ErrorUnion { error, result } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(result), depth + 1));
                        pending.push((Walk::W295(error), depth + 1));
                    }
                    SemanticTypeRecipe::Qualified { storage, life } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W306(life), depth + 1));
                        pending.push((Walk::W295(storage), depth + 1));
                    }
                    SemanticTypeRecipe::Code { kind, signature } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W308(signature), depth + 1));
                        pending.push((Walk::W307(kind), depth + 1));
                    }
                    SemanticTypeRecipe::Closure {
                        definition,
                        arguments,
                        environment,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W299(environment), depth + 1));
                        pending.push((Walk::W299(arguments), depth + 1));
                        pending.push((Walk::W148(definition), depth + 1));
                    }
                    SemanticTypeRecipe::Dyn { owned, interface } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W309(interface), depth + 1));
                        pending.push((Walk::W50(owned), depth + 1));
                    }
                    SemanticTypeRecipe::Opaque {
                        definition,
                        arguments,
                        hidden,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(hidden), depth + 1));
                        pending.push((Walk::W299(arguments), depth + 1));
                        pending.push((Walk::W148(definition), depth + 1));
                    }
                    SemanticTypeRecipe::AnonymousSum(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W302(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::UnresolvedNamed(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::Unsupported(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticTypeRecipe::OriginalType { scope, source } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W310(source), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                }
            }
            Walk::W296(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W331(&value.classes), depth + 1));
                pending.push((Walk::W45(&value.life), depth + 1));
                pending.push((Walk::W43(&value.ty), depth + 1));
            }
            Walk::W297(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.ordinal), depth + 1));
                pending.push((Walk::W298(&value.scope), depth + 1));
            }
            Walk::W298(value) => {
                meter.node(depth)?;
                match value {
                    SemanticScopeRef::Ancestor(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                }
            }
            Walk::W299(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W300(child), depth));
                }
            }
            Walk::W300(value) => {
                meter.node(depth)?;
                match value {
                    SemanticActualRecipe::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticActualRecipe::Const { carrier, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W303(value), depth + 1));
                        pending.push((Walk::W295(carrier), depth + 1));
                    }
                    SemanticActualRecipe::Life(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W312(value_0), depth + 1));
                    }
                    SemanticActualRecipe::Infer => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W301(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W299(&value.arguments), depth + 1));
                pending.push((Walk::W74(&value.declaration), depth + 1));
            }
            Walk::W302(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W295(child), depth));
                }
            }
            Walk::W303(value) => {
                meter.node(depth)?;
                match value {
                    SemanticConstRecipe::Value(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W140(value_0), depth + 1));
                    }
                    SemanticConstRecipe::IntegerLiteral(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticConstRecipe::FloatLiteral(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticConstRecipe::Formal(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W297(value_0), depth + 1));
                    }
                    SemanticConstRecipe::Environment { scope, ordinal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(ordinal), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                    SemanticConstRecipe::Unary {
                        op,
                        operand_type,
                        value,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W303(value), depth + 1));
                        pending.push((Walk::W295(operand_type), depth + 1));
                        pending.push((Walk::W141(op), depth + 1));
                    }
                    SemanticConstRecipe::Binary {
                        op,
                        operand_type,
                        left,
                        right,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W303(right), depth + 1));
                        pending.push((Walk::W303(left), depth + 1));
                        pending.push((Walk::W295(operand_type), depth + 1));
                        pending.push((Walk::W135(op), depth + 1));
                    }
                    SemanticConstRecipe::Convert {
                        source_type,
                        target_type,
                        value,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W303(value), depth + 1));
                        pending.push((Walk::W295(target_type), depth + 1));
                        pending.push((Walk::W295(source_type), depth + 1));
                    }
                    SemanticConstRecipe::AssociatedConst {
                        receiver,
                        application,
                        member,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(member), depth + 1));
                        pending.push((Walk::W301(application), depth + 1));
                        pending.push((Walk::W295(receiver), depth + 1));
                    }
                }
            }
            Walk::W304(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOuterLifeRecipe::Class(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W312(value_0), depth + 1));
                    }
                    SemanticOuterLifeRecipe::Static => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W305(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W323(&value.nominal), depth + 1));
                pending.push((Walk::W321(&value.retained), depth + 1));
                pending.push((Walk::W319(&value.ports), depth + 1));
                pending.push((Walk::W318(&value.domain), depth + 1));
            }
            Walk::W306(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W323(&value.nominal), depth + 1));
                pending.push((Walk::W321(&value.retained), depth + 1));
                pending.push((Walk::W329(&value.slots), depth + 1));
                pending.push((Walk::W318(&value.domain), depth + 1));
            }
            Walk::W307(value) => {
                meter.node(depth)?;
                match value {
                    SemanticCodeKind::FunctionPointer => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticCodeKind::BorrowedFunctionPointer => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticCodeKind::ForeignCallback => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W308(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    6,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W105(&value.call_kind), depth + 1));
                pending.push((Walk::W121(&value.result_mode), depth + 1));
                pending.push((Walk::W295(&value.result), depth + 1));
                pending.push((Walk::W327(&value.parameters), depth + 1));
                pending.push((Walk::W26(&value.invocation_lifetimes), depth + 1));
                pending.push((Walk::W26(&value.bound_lifetimes), depth + 1));
            }
            Walk::W309(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W315(&value.associated), depth + 1));
                pending.push((Walk::W314(&value.context), depth + 1));
                pending.push((Walk::W301(&value.application), depth + 1));
            }
            Walk::W310(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalTypeSource::Expression(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W9(value_0), depth + 1));
                    }
                    SemanticOriginalTypeSource::Binding(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W9(value_0), depth + 1));
                    }
                    SemanticOriginalTypeSource::Parameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    SemanticOriginalTypeSource::Result => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalTypeSource::CallConstCarrier { call, formal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(formal), depth + 1));
                        pending.push((Walk::W9(call), depth + 1));
                    }
                    SemanticOriginalTypeSource::BindingProjection { statement, pattern } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W177(pattern), depth + 1));
                        pending.push((Walk::W9(statement), depth + 1));
                    }
                    SemanticOriginalTypeSource::NameAt { at, name } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(name), depth + 1));
                        pending.push((Walk::W311(at), depth + 1));
                    }
                }
            }
            Walk::W311(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalNode::Expression(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W9(value_0), depth + 1));
                    }
                    SemanticOriginalNode::Statement(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W9(value_0), depth + 1));
                    }
                }
            }
            Walk::W312(value) => {
                meter.node(depth)?;
                match value {
                    SemanticLifeRecipe::Formal(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W297(value_0), depth + 1));
                    }
                    SemanticLifeRecipe::Implicit { scope, ordinal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(ordinal), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                    SemanticLifeRecipe::Environment { scope, ordinal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(ordinal), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                    SemanticLifeRecipe::Bound {
                        depth: bound_depth,
                        index,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(index), depth + 1));
                        pending.push((Walk::W26(bound_depth), depth + 1));
                    }
                    SemanticLifeRecipe::Invocation(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    SemanticLifeRecipe::OuterOccurrence {
                        operand,
                        path,
                        mode,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W75(mode), depth + 1));
                        pending.push((Walk::W62(path), depth + 1));
                        pending.push((Walk::W313(operand), depth + 1));
                    }
                    SemanticLifeRecipe::IntrinsicClass {
                        operand,
                        path,
                        class,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(class), depth + 1));
                        pending.push((Walk::W62(path), depth + 1));
                        pending.push((Walk::W313(operand), depth + 1));
                    }
                }
            }
            Walk::W313(value) => {
                meter.node(depth)?;
                match value {
                    SemanticRecipeOperand::TypeFormal(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W297(value_0), depth + 1));
                    }
                    SemanticRecipeOperand::Environment { scope, ordinal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(ordinal), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                    SemanticRecipeOperand::Parameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    SemanticRecipeOperand::Receiver => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W314(value) => {
                meter.node(depth)?;
                match value {
                    SemanticDynContextRecipe::Closed => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticDynContextRecipe::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticDynContextRecipe::Shape(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W305(value_0), depth + 1));
                    }
                }
            }
            Walk::W315(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W316(child), depth));
                }
            }
            Walk::W316(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W317(&value.value), depth + 1));
                pending.push((Walk::W26(&value.member), depth + 1));
                pending.push((Walk::W301(&value.application), depth + 1));
            }
            Walk::W317(value) => {
                meter.node(depth)?;
                match value {
                    SemanticAssociatedRecipe::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticAssociatedRecipe::Const { carrier, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W303(value), depth + 1));
                        pending.push((Walk::W295(carrier), depth + 1));
                    }
                }
            }
            Walk::W318(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    1,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W325(&value.parts), depth + 1));
            }
            Walk::W319(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W320(child), depth));
                }
            }
            Walk::W320(value) => {
                meter.node(depth)?;
                match value {
                    SemanticPortPart::One { leaf, mode, class } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W312(class), depth + 1));
                        pending.push((Walk::W75(mode), depth + 1));
                        pending.push((Walk::W295(leaf), depth + 1));
                    }
                    SemanticPortPart::FromType { ty, classes } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W318(classes), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                }
            }
            Walk::W321(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W322(child), depth));
                }
            }
            Walk::W322(value) => {
                meter.node(depth)?;
                match value {
                    SemanticRetainedPart::One {
                        path,
                        child_class,
                        parent,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W312(parent), depth + 1));
                        pending.push((Walk::W26(child_class), depth + 1));
                        pending.push((Walk::W62(path), depth + 1));
                    }
                    SemanticRetainedPart::All { path, parents } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W318(parents), depth + 1));
                        pending.push((Walk::W62(path), depth + 1));
                    }
                }
            }
            Walk::W323(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W324(child), depth));
                }
            }
            Walk::W324(value) => {
                meter.node(depth)?;
                match value {
                    SemanticNominalPart::One {
                        path,
                        declaration,
                        argument,
                        class,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W312(class), depth + 1));
                        pending.push((Walk::W26(argument), depth + 1));
                        pending.push((Walk::W74(declaration), depth + 1));
                        pending.push((Walk::W62(path), depth + 1));
                    }
                    SemanticNominalPart::FromType { path, ty, classes } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W318(classes), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                        pending.push((Walk::W62(path), depth + 1));
                    }
                }
            }
            Walk::W325(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W326(child), depth));
                }
            }
            Walk::W326(value) => {
                meter.node(depth)?;
                match value {
                    SemanticClassSequencePart::One(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W312(value_0), depth + 1));
                    }
                    SemanticClassSequencePart::AllClasses { ty, path } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W62(path), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                }
            }
            Walk::W327(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W328(child), depth));
                }
            }
            Walk::W328(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W121(&value.mode), depth + 1));
                pending.push((Walk::W295(&value.ty), depth + 1));
            }
            Walk::W329(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W330(child), depth));
                }
            }
            Walk::W330(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W312(&value.class), depth + 1));
                pending.push((Walk::W75(&value.mode), depth + 1));
                pending.push((Walk::W78(&value.path), depth + 1));
            }
            Walk::W331(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W312(child), depth));
                }
            }
            Walk::W332(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W333(child), depth));
                }
            }
            Walk::W333(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W212(&value.method_actuals), depth + 1));
                pending.push((Walk::W212(&value.declaration_actuals), depth + 1));
                pending.push((Walk::W211(&value.receiver), depth + 1));
                pending.push((Walk::W355(&value.function), depth + 1));
            }
            Walk::W334(value) => {
                meter.node(depth)?;
                match value {
                    DynDefinitionScope::TraitDefault => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynDefinitionScope::ImplOverride => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W335(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    6,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W338(&value.contract), depth + 1));
                pending.push((Walk::W26(&value.invocation_binder_count), depth + 1));
                pending.push((Walk::W105(&value.call_kind), depth + 1));
                pending.push((Walk::W121(&value.result_mode), depth + 1));
                pending.push((Walk::W283(&value.result), depth + 1));
                pending.push((Walk::W336(&value.parameters), depth + 1));
            }
            Walk::W336(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W337(child), depth));
                }
            }
            Walk::W337(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W121(&value.mode), depth + 1));
                pending.push((Walk::W283(&value.ty), depth + 1));
            }
            Walk::W338(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    9,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W262(&value.threads), depth + 1));
                pending.push((Walk::W260(&value.resume), depth + 1));
                pending.push((Walk::W349(&value.effects), depth + 1));
                pending.push((Walk::W347(&value.cleanup), depth + 1));
                pending.push((Walk::W345(&value.result), depth + 1));
                pending.push((Walk::W253(&value.call), depth + 1));
                pending.push((Walk::W252(&value.hold), depth + 1));
                pending.push((Walk::W342(&value.guarantees), depth + 1));
                pending.push((Walk::W339(&value.requires), depth + 1));
            }
            Walk::W339(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W340(child), depth));
                }
            }
            Walk::W340(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W341(child), depth));
                }
            }
            Walk::W341(value) => {
                meter.node(depth)?;
                match value {
                    DynRequirementRecipe::Read(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynRequirementRecipe::Write(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynRequirementRecipe::Manage(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynRequirementRecipe::Apart(value_0, value_1) => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_1), depth + 1));
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynRequirementRecipe::State {
                        position,
                        ty,
                        variant,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(variant), depth + 1));
                        pending.push((Walk::W283(ty), depth + 1));
                        pending.push((Walk::W56(position), depth + 1));
                    }
                }
            }
            Walk::W342(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W343(child), depth));
                }
            }
            Walk::W343(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W344(child), depth));
                }
            }
            Walk::W344(value) => {
                meter.node(depth)?;
                match value {
                    DynGuaranteeRecipe::StorageSources(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W354(value_0), depth + 1));
                    }
                    DynGuaranteeRecipe::ManageResult => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynGuaranteeRecipe::State {
                        position,
                        ty,
                        variant,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(variant), depth + 1));
                        pending.push((Walk::W283(ty), depth + 1));
                        pending.push((Walk::W56(position), depth + 1));
                    }
                }
            }
            Walk::W345(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W346(child), depth));
                }
            }
            Walk::W346(value) => {
                meter.node(depth)?;
                match value {
                    DynResultSourceRecipe::Value => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynResultSourceRecipe::Create => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynResultSourceRecipe::Give => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynResultSourceRecipe::Borrow { sources, mutable } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(mutable), depth + 1));
                        pending.push((Walk::W352(sources), depth + 1));
                    }
                }
            }
            Walk::W347(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W348(child), depth));
                }
            }
            Walk::W348(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W350(&value.effects), depth + 1));
                pending.push((Walk::W254(&value.access), depth + 1));
                pending.push((Walk::W55(&value.positions), depth + 1));
                pending.push((Walk::W50(&value.environment), depth + 1));
            }
            Walk::W349(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W350(child), depth));
                }
            }
            Walk::W350(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W351(child), depth));
                }
            }
            Walk::W351(value) => {
                meter.node(depth)?;
                match value {
                    DynEffectRecipe::Alloc => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffectRecipe::Io => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffectRecipe::Block => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffectRecipe::Suspend => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffectRecipe::Spawn => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffectRecipe::Ffi => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffectRecipe::Reenter => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffectRecipe::Panic => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynEffectRecipe::Call(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynEffectRecipe::Cleanup(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(value_0), depth + 1));
                    }
                }
            }
            Walk::W352(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W353(child), depth));
                }
            }
            Walk::W353(value) => {
                meter.node(depth)?;
                match value {
                    DynStorageSourceRecipe::Exact(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(value_0), depth + 1));
                    }
                    DynStorageSourceRecipe::PlaceAddress { place, mode } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W75(mode), depth + 1));
                        pending.push((Walk::W56(place), depth + 1));
                    }
                    DynStorageSourceRecipe::OwnedView { head, leaf, mode } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W75(mode), depth + 1));
                        pending.push((Walk::W283(leaf), depth + 1));
                        pending.push((Walk::W56(head), depth + 1));
                    }
                    DynStorageSourceRecipe::ReborrowedLeaf { source, head } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W56(head), depth + 1));
                        pending.push((Walk::W56(source), depth + 1));
                    }
                    DynStorageSourceRecipe::Static { leaf, mode } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W75(mode), depth + 1));
                        pending.push((Walk::W283(leaf), depth + 1));
                    }
                }
            }
            Walk::W354(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W352(&value.sources), depth + 1));
                pending.push((Walk::W56(&value.position), depth + 1));
            }
            Walk::W355(value) => {
                meter.node(depth)?;
                match value {
                    DynFunctionTarget::Core { module, source } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W9(source), depth + 1));
                        pending.push((Walk::W1(module), depth + 1));
                    }
                    DynFunctionTarget::Typed(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W356(value_0), depth + 1));
                    }
                }
            }
            Walk::W356(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W357(value) => {
                meter.node(depth)?;
                match value {
                    DynStructuralBound::Copy => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynStructuralBound::Value => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynStructuralBound::SyncDrop => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynStructuralBound::StoreElement => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W358(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W210(child), depth));
                }
            }
            Walk::W359(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W360(child), depth));
                }
            }
            Walk::W360(value) => {
                meter.node(depth)?;
                match value {
                    DynAssociatedValueRecipe::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W283(value_0), depth + 1));
                    }
                    DynAssociatedValueRecipe::Constant { ty, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W291(value), depth + 1));
                        pending.push((Walk::W283(ty), depth + 1));
                    }
                }
            }
            Walk::W361(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W362(child), depth));
                }
            }
            Walk::W362(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W364(&value.kind), depth + 1));
                pending.push((Walk::W177(&value.path), depth + 1));
                pending.push((Walk::W363(&value.root), depth + 1));
            }
            Walk::W363(value) => {
                meter.node(depth)?;
                match value {
                    DynImplicitLifeRoot::SelfPattern => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynImplicitLifeRoot::ImplementedTrait => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynImplicitLifeRoot::Supertrait(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynImplicitLifeRoot::Requirement(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynImplicitLifeRoot::AssociatedDefinition(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynImplicitLifeRoot::MethodSignature(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    DynImplicitLifeRoot::MethodDefinition(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                }
            }
            Walk::W364(value) => {
                meter.node(depth)?;
                match value {
                    DynImplicitLifeKind::Address => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynImplicitLifeKind::Description => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    DynImplicitLifeKind::LifetimeArgument => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W365(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W366(child), depth));
                }
            }
            Walk::W366(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    9,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W410(&value.body), depth + 1));
                pending.push((Walk::W408(&value.environment), depth + 1));
                pending.push((Walk::W406(&value.requirements), depth + 1));
                pending.push((Walk::W404(&value.implicit_lifetimes), depth + 1));
                pending.push((Walk::W402(&value.parameters), depth + 1));
                pending.push((Walk::W401(&value.kind), depth + 1));
                pending.push((Walk::W147(&value.parent), depth + 1));
                pending.push((Walk::W183(&value.identity), depth + 1));
                pending.push((Walk::W148(&value.id), depth + 1));
            }
            Walk::W367(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W368(child), depth));
                }
            }
            Walk::W368(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    9,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W400(&value.self_type), depth + 1));
                pending.push((Walk::W398(&value.environment), depth + 1));
                pending.push((Walk::W396(&value.header_embedding), depth + 1));
                pending.push((Walk::W381(&value.actuals), depth + 1));
                pending.push((Walk::W395(&value.parent), depth + 1));
                pending.push((Walk::W394(&value.lifetime_instantiation), depth + 1));
                pending.push((Walk::W393(&value.target), depth + 1));
                pending.push((Walk::W148(&value.definition), depth + 1));
                pending.push((Walk::W374(&value.id), depth + 1));
            }
            Walk::W369(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W370(child), depth));
                }
            }
            Walk::W370(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W377(&value.action), depth + 1));
                pending.push((Walk::W376(&value.at), depth + 1));
                pending.push((Walk::W375(&value.recipe), depth + 1));
                pending.push((Walk::W374(&value.instance), depth + 1));
            }
            Walk::W371(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W372(child), depth));
                }
            }
            Walk::W372(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W148(&value.scope), depth + 1));
                pending.push((Walk::W373(&value.owner), depth + 1));
            }
            Walk::W373(value) => {
                meter.node(depth)?;
                match value {
                    SemanticDynOwner::Declaration(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W74(value_0), depth + 1));
                    }
                    SemanticDynOwner::MethodSignature {
                        declaration,
                        member,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(member), depth + 1));
                        pending.push((Walk::W74(declaration), depth + 1));
                    }
                    SemanticDynOwner::MethodDefinition(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W210(value_0), depth + 1));
                    }
                }
            }
            Walk::W374(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.index), depth + 1));
                pending.push((Walk::W1(&value.module), depth + 1));
            }
            Walk::W375(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W376(value) => {
                meter.node(depth)?;
                match value {
                    SemanticSiteAddress::Core {
                        expression,
                        occurrence,
                        role,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W390(role), depth + 1));
                        pending.push((Walk::W26(occurrence), depth + 1));
                        pending.push((Walk::W9(expression), depth + 1));
                    }
                    SemanticSiteAddress::Typed {
                        block,
                        instruction,
                        edge,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W392(edge), depth + 1));
                        pending.push((Walk::W26(instruction), depth + 1));
                        pending.push((Walk::W391(block), depth + 1));
                    }
                }
            }
            Walk::W377(value) => {
                meter.node(depth)?;
                match value {
                    SemanticSiteAction::Construct { result, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W379(value), depth + 1));
                        pending.push((Walk::W378(result), depth + 1));
                    }
                    SemanticSiteAction::Call { target, actuals } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W381(actuals), depth + 1));
                        pending.push((Walk::W380(target), depth + 1));
                    }
                    SemanticSiteAction::CallableCall { callee } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W378(callee), depth + 1));
                    }
                    SemanticSiteAction::TypeUse { subject, expected } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W379(expected), depth + 1));
                        pending.push((Walk::W378(subject), depth + 1));
                    }
                    SemanticSiteAction::DeclareLocal { local, expected } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W379(expected), depth + 1));
                        pending.push((Walk::W383(local), depth + 1));
                    }
                    SemanticSiteAction::Closure {
                        result,
                        closure,
                        environment,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W381(environment), depth + 1));
                        pending.push((Walk::W384(closure), depth + 1));
                        pending.push((Walk::W378(result), depth + 1));
                    }
                    SemanticSiteAction::CoreBodyCall => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticSiteAction::BodyCall { actuals } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W381(actuals), depth + 1));
                    }
                }
            }
            Walk::W378(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOperand::Parameter(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    SemanticOperand::Capture(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    SemanticOperand::Receiver => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOperand::Value(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W386(value_0), depth + 1));
                    }
                    SemanticOperand::Local(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W387(value_0), depth + 1));
                    }
                    SemanticOperand::TypeArgument { scope, ordinal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(ordinal), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                    SemanticOperand::Environment(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(value_0), depth + 1));
                    }
                    SemanticOperand::CoreExpression {
                        expression,
                        occurrence,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(occurrence), depth + 1));
                        pending.push((Walk::W9(expression), depth + 1));
                    }
                }
            }
            Walk::W379(value) => {
                meter.node(depth)?;
                match value {
                    SemanticTypeValue::Resolved { ty, life, classes } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W388(classes), depth + 1));
                        pending.push((Walk::W45(life), depth + 1));
                        pending.push((Walk::W43(ty), depth + 1));
                    }
                    SemanticTypeValue::Core { scope, source } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W389(source), depth + 1));
                        pending.push((Walk::W374(scope), depth + 1));
                    }
                }
            }
            Walk::W380(value) => {
                meter.node(depth)?;
                match value {
                    SemanticFunctionTarget::Core { module, function } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W9(function), depth + 1));
                        pending.push((Walk::W1(module), depth + 1));
                    }
                    SemanticFunctionTarget::CoreClosure {
                        module,
                        creator,
                        expression,
                        occurrence,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(occurrence), depth + 1));
                        pending.push((Walk::W9(expression), depth + 1));
                        pending.push((Walk::W9(creator), depth + 1));
                        pending.push((Walk::W1(module), depth + 1));
                    }
                    SemanticFunctionTarget::Typed(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W356(value_0), depth + 1));
                    }
                }
            }
            Walk::W381(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W382(child), depth));
                }
            }
            Walk::W382(value) => {
                meter.node(depth)?;
                match value {
                    SemanticActual::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W379(value_0), depth + 1));
                    }
                    SemanticActual::Const { ty, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W140(value), depth + 1));
                        pending.push((Walk::W379(ty), depth + 1));
                    }
                    SemanticActual::Life(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W385(value_0), depth + 1));
                    }
                    SemanticActual::CoreRecipe { scope, recipe } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W300(recipe), depth + 1));
                        pending.push((Walk::W374(scope), depth + 1));
                    }
                    SemanticActual::CoreInferred {
                        scope,
                        call,
                        formal,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(formal), depth + 1));
                        pending.push((Walk::W375(call), depth + 1));
                        pending.push((Walk::W374(scope), depth + 1));
                    }
                }
            }
            Walk::W383(value) => {
                meter.node(depth)?;
                match value {
                    SemanticLocalTarget::Core {
                        statement,
                        occurrence,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(occurrence), depth + 1));
                        pending.push((Walk::W9(statement), depth + 1));
                    }
                    SemanticLocalTarget::Typed(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W387(value_0), depth + 1));
                    }
                }
            }
            Walk::W384(value) => {
                meter.node(depth)?;
                match value {
                    SemanticClosureTarget::Core {
                        module,
                        creator,
                        expression,
                        occurrence,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(occurrence), depth + 1));
                        pending.push((Walk::W9(expression), depth + 1));
                        pending.push((Walk::W9(creator), depth + 1));
                        pending.push((Walk::W1(module), depth + 1));
                    }
                    SemanticClosureTarget::Typed(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W52(value_0), depth + 1));
                    }
                }
            }
            Walk::W385(value) => {
                meter.node(depth)?;
                match value {
                    SemanticClassExpr::Formal { scope, ordinal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(ordinal), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                    SemanticClassExpr::Implicit { scope, ordinal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(ordinal), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                    SemanticClassExpr::Bound {
                        depth: bound_depth,
                        index,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(index), depth + 1));
                        pending.push((Walk::W26(bound_depth), depth + 1));
                    }
                    SemanticClassExpr::OuterOccurrence {
                        operand,
                        path,
                        mode,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W75(mode), depth + 1));
                        pending.push((Walk::W62(path), depth + 1));
                        pending.push((Walk::W378(operand), depth + 1));
                    }
                    SemanticClassExpr::IntrinsicClass {
                        operand,
                        path,
                        class,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(class), depth + 1));
                        pending.push((Walk::W62(path), depth + 1));
                        pending.push((Walk::W378(operand), depth + 1));
                    }
                    SemanticClassExpr::Environment { scope, ordinal } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W26(ordinal), depth + 1));
                        pending.push((Walk::W298(scope), depth + 1));
                    }
                }
            }
            Walk::W386(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W387(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W388(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W385(child), depth));
                }
            }
            Walk::W389(value) => {
                meter.node(depth)?;
                match value {
                    SemanticCoreTypeSource::OriginalRecipe(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticCoreTypeSource::Expression {
                        module,
                        function,
                        expression,
                        occurrence,
                        recipe,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            5,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(recipe), depth + 1));
                        pending.push((Walk::W26(occurrence), depth + 1));
                        pending.push((Walk::W9(expression), depth + 1));
                        pending.push((Walk::W9(function), depth + 1));
                        pending.push((Walk::W1(module), depth + 1));
                    }
                    SemanticCoreTypeSource::OriginalExpression {
                        module,
                        function,
                        expression,
                        occurrence,
                        source,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            5,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W310(source), depth + 1));
                        pending.push((Walk::W26(occurrence), depth + 1));
                        pending.push((Walk::W9(expression), depth + 1));
                        pending.push((Walk::W9(function), depth + 1));
                        pending.push((Walk::W1(module), depth + 1));
                    }
                }
            }
            Walk::W390(value) => {
                meter.node(depth)?;
                match value {
                    SemanticEmissionRole::Construct => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticEmissionRole::Call => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticEmissionRole::Ascription => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticEmissionRole::Binding => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticEmissionRole::Closure => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W391(value) => {
                let _ = value;
                meter.node(depth)?;
            }
            Walk::W392(value) => {
                meter.node(depth)?;
                match value {
                    SemanticSiteEdge::Before => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticSiteEdge::After => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W393(value) => {
                meter.node(depth)?;
                match value {
                    SemanticInstanceTarget::Function(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W380(value_0), depth + 1));
                    }
                    SemanticInstanceTarget::Scope => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W394(value) => {
                meter.node(depth)?;
                match value {
                    SemanticLifetimeInstantiation::Rigid => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticLifetimeInstantiation::Partition { rigid } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W374(rigid), depth + 1));
                    }
                }
            }
            Walk::W395(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W374(child), depth));
                }
            }
            Walk::W396(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W397(child), depth));
                }
            }
            Walk::W397(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W385(&value.actual), depth + 1));
                pending.push((Walk::W60(&value.header), depth + 1));
            }
            Walk::W398(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W399(child), depth));
                }
            }
            Walk::W399(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W381(&value.actuals), depth + 1));
                pending.push((Walk::W384(&value.closure), depth + 1));
            }
            Walk::W400(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W379(child), depth));
                }
            }
            Walk::W401(value) => {
                meter.node(depth)?;
                match value {
                    SemanticDefinitionKind::Function => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticDefinitionKind::Method => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticDefinitionKind::Closure => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticDefinitionKind::TaskEntry => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticDefinitionKind::Nominal => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticDefinitionKind::Scope => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W402(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W403(child), depth));
                }
            }
            Walk::W403(value) => {
                meter.node(depth)?;
                match value {
                    SemanticFormal::Type { requirements } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W406(requirements), depth + 1));
                    }
                    SemanticFormal::Const { carrier } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(carrier), depth + 1));
                    }
                    SemanticFormal::Lifetime => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W404(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W405(child), depth));
                }
            }
            Walk::W405(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W177(&value.occurrence_path), depth + 1));
                pending.push((Walk::W26(&value.ordinal), depth + 1));
            }
            Walk::W406(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W407(child), depth));
                }
            }
            Walk::W407(value) => {
                meter.node(depth)?;
                match value {
                    SemanticRequirementRecipe::Structural { ty, bound } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W357(bound), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                    SemanticRequirementRecipe::Trait { ty, application } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W301(application), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                    SemanticRequirementRecipe::Callable {
                        ty,
                        kind,
                        signature,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W308(signature), depth + 1));
                        pending.push((Walk::W182(kind), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                    SemanticRequirementRecipe::TypeEqual { left, right } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(right), depth + 1));
                        pending.push((Walk::W295(left), depth + 1));
                    }
                    SemanticRequirementRecipe::ConstEqual {
                        carrier,
                        left,
                        right,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W303(right), depth + 1));
                        pending.push((Walk::W303(left), depth + 1));
                        pending.push((Walk::W295(carrier), depth + 1));
                    }
                    SemanticRequirementRecipe::LifeEqual { left, right } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W312(right), depth + 1));
                        pending.push((Walk::W312(left), depth + 1));
                    }
                    SemanticRequirementRecipe::Outlives { longer, shorter } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W312(shorter), depth + 1));
                        pending.push((Walk::W312(longer), depth + 1));
                    }
                    SemanticRequirementRecipe::ForAll {
                        lifetimes,
                        requirements,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W406(requirements), depth + 1));
                        pending.push((Walk::W26(lifetimes), depth + 1));
                    }
                    SemanticRequirementRecipe::Builtin { ty, trait_kind } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W460(trait_kind), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                }
            }
            Walk::W408(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W409(child), depth));
                }
            }
            Walk::W409(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W300(&value.source), depth + 1));
                pending.push((Walk::W403(&value.kind), depth + 1));
            }
            Walk::W410(value) => {
                meter.node(depth)?;
                match value {
                    SemanticDefinitionBody::Function { signature, sites } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W411(sites), depth + 1));
                        pending.push((Walk::W308(signature), depth + 1));
                    }
                    SemanticDefinitionBody::Nominal {
                        declaration,
                        kind,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W413(body), depth + 1));
                        pending.push((Walk::W217(kind), depth + 1));
                        pending.push((Walk::W74(declaration), depth + 1));
                    }
                    SemanticDefinitionBody::Scope(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W414(value_0), depth + 1));
                    }
                    SemanticDefinitionBody::FunctionWithBody {
                        signature,
                        sites,
                        parameters,
                        receiver_parameter,
                        names,
                        site_nodes,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            7,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W421(body), depth + 1));
                        pending.push((Walk::W419(site_nodes), depth + 1));
                        pending.push((Walk::W417(names), depth + 1));
                        pending.push((Walk::W190(receiver_parameter), depth + 1));
                        pending.push((Walk::W415(parameters), depth + 1));
                        pending.push((Walk::W411(sites), depth + 1));
                        pending.push((Walk::W308(signature), depth + 1));
                    }
                }
            }
            Walk::W411(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W412(child), depth));
                }
            }
            Walk::W412(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W459(&value.kind), depth + 1));
                pending.push((Walk::W177(&value.occurrence_path), depth + 1));
                pending.push((Walk::W375(&value.id), depth + 1));
            }
            Walk::W413(value) => {
                meter.node(depth)?;
                match value {
                    SemanticNominalBody::Product { fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W461(fields), depth + 1));
                    }
                    SemanticNominalBody::Enum { variants } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W463(variants), depth + 1));
                    }
                }
            }
            Walk::W414(value) => {
                meter.node(depth)?;
                match value {
                    SemanticScopeRecipe::Trait { declaration } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W74(declaration), depth + 1));
                    }
                    SemanticScopeRecipe::InherentImpl {
                        declaration,
                        self_type,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(self_type), depth + 1));
                        pending.push((Walk::W74(declaration), depth + 1));
                    }
                    SemanticScopeRecipe::TraitImpl {
                        declaration,
                        self_type,
                        trait_application,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W301(trait_application), depth + 1));
                        pending.push((Walk::W295(self_type), depth + 1));
                        pending.push((Walk::W74(declaration), depth + 1));
                    }
                    SemanticScopeRecipe::BuiltinImpl {
                        self_type,
                        trait_kind,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W460(trait_kind), depth + 1));
                        pending.push((Walk::W295(self_type), depth + 1));
                    }
                }
            }
            Walk::W415(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W416(child), depth));
                }
            }
            Walk::W416(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    4,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W121(&value.mode), depth + 1));
                pending.push((Walk::W426(&value.ty), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
            }
            Walk::W417(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W418(child), depth));
                }
            }
            Walk::W418(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W458(&value.value), depth + 1));
                pending.push((Walk::W457(&value.namespace), depth + 1));
                pending.push((Walk::W151(&value.path), depth + 1));
            }
            Walk::W419(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W420(child), depth));
                }
            }
            Walk::W420(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W311(&value.node), depth + 1));
                pending.push((Walk::W375(&value.site), depth + 1));
            }
            Walk::W421(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalFunctionBody::Defined(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(value_0), depth + 1));
                    }
                    SemanticOriginalFunctionBody::External => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalFunctionBody::Declaration => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W422(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W423(&value.statements), depth + 1));
            }
            Walk::W423(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W424(child), depth));
                }
            }
            Walk::W424(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W425(&value.kind), depth + 1));
                pending.push((Walk::W9(&value.id), depth + 1));
            }
            Walk::W425(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalStatementKind::Binding {
                        name,
                        mutable,
                        ty,
                        initializer,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W427(initializer), depth + 1));
                        pending.push((Walk::W426(ty), depth + 1));
                        pending.push((Walk::W50(mutable), depth + 1));
                        pending.push((Walk::W10(name), depth + 1));
                    }
                    SemanticOriginalStatementKind::TupleBinding {
                        pattern,
                        mutable,
                        ty,
                        initializer,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(initializer), depth + 1));
                        pending.push((Walk::W426(ty), depth + 1));
                        pending.push((Walk::W50(mutable), depth + 1));
                        pending.push((Walk::W429(pattern), depth + 1));
                    }
                    SemanticOriginalStatementKind::Assign { target, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value), depth + 1));
                        pending.push((Walk::W428(target), depth + 1));
                    }
                    SemanticOriginalStatementKind::Expression(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value_0), depth + 1));
                    }
                    SemanticOriginalStatementKind::Return(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W427(value_0), depth + 1));
                    }
                    SemanticOriginalStatementKind::If {
                        condition,
                        then_body,
                        else_body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W430(else_body), depth + 1));
                        pending.push((Walk::W422(then_body), depth + 1));
                        pending.push((Walk::W428(condition), depth + 1));
                    }
                    SemanticOriginalStatementKind::IfLet {
                        pattern,
                        value,
                        then_body,
                        else_body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W430(else_body), depth + 1));
                        pending.push((Walk::W422(then_body), depth + 1));
                        pending.push((Walk::W428(value), depth + 1));
                        pending.push((Walk::W429(pattern), depth + 1));
                    }
                    SemanticOriginalStatementKind::While { condition, body } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(body), depth + 1));
                        pending.push((Walk::W428(condition), depth + 1));
                    }
                    SemanticOriginalStatementKind::WhileLet {
                        pattern,
                        value,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(body), depth + 1));
                        pending.push((Walk::W428(value), depth + 1));
                        pending.push((Walk::W429(pattern), depth + 1));
                    }
                    SemanticOriginalStatementKind::ForRange {
                        name,
                        start,
                        end,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(body), depth + 1));
                        pending.push((Walk::W428(end), depth + 1));
                        pending.push((Walk::W428(start), depth + 1));
                        pending.push((Walk::W10(name), depth + 1));
                    }
                    SemanticOriginalStatementKind::Match { value, arms } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W431(arms), depth + 1));
                        pending.push((Walk::W428(value), depth + 1));
                    }
                    SemanticOriginalStatementKind::Block(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(value_0), depth + 1));
                    }
                    SemanticOriginalStatementKind::Defer(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(value_0), depth + 1));
                    }
                    SemanticOriginalStatementKind::Unsafe(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(value_0), depth + 1));
                    }
                    SemanticOriginalStatementKind::Break => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalStatementKind::Continue => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalStatementKind::Unsupported(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalStatementKind::ForEach {
                        pattern,
                        iterator,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(body), depth + 1));
                        pending.push((Walk::W428(iterator), depth + 1));
                        pending.push((Walk::W429(pattern), depth + 1));
                    }
                    SemanticOriginalStatementKind::CompoundAssign { target, op, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value), depth + 1));
                        pending.push((Walk::W135(op), depth + 1));
                        pending.push((Walk::W428(target), depth + 1));
                    }
                    SemanticOriginalStatementKind::Yield(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value_0), depth + 1));
                    }
                    SemanticOriginalStatementKind::Loop(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(value_0), depth + 1));
                    }
                    SemanticOriginalStatementKind::With { name, value, body } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(body), depth + 1));
                        pending.push((Walk::W428(value), depth + 1));
                        pending.push((Walk::W10(name), depth + 1));
                    }
                    SemanticOriginalStatementKind::CompileTime(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W424(value_0), depth + 1));
                    }
                }
            }
            Walk::W426(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W295(child), depth));
                }
            }
            Walk::W427(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W428(child), depth));
                }
            }
            Walk::W428(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W433(&value.kind), depth + 1));
                pending.push((Walk::W9(&value.id), depth + 1));
            }
            Walk::W429(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W453(&value.kind), depth + 1));
            }
            Walk::W430(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W422(child), depth));
                }
            }
            Walk::W431(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W432(child), depth));
                }
            }
            Walk::W432(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W422(&value.body), depth + 1));
                pending.push((Walk::W429(&value.pattern), depth + 1));
            }
            Walk::W433(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalExprKind::MakeDynBorrow {
                        interface,
                        vtable,
                        source,
                        mutable,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(mutable), depth + 1));
                        pending.push((Walk::W428(source), depth + 1));
                        pending.push((Walk::W137(vtable), depth + 1));
                        pending.push((Walk::W44(interface), depth + 1));
                    }
                    SemanticOriginalExprKind::MakeDynOwned {
                        interface,
                        vtable,
                        source,
                        route,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W138(route), depth + 1));
                        pending.push((Walk::W428(source), depth + 1));
                        pending.push((Walk::W137(vtable), depth + 1));
                        pending.push((Walk::W44(interface), depth + 1));
                    }
                    SemanticOriginalExprKind::CallDynSlot {
                        callee,
                        interface,
                        slot,
                        arguments,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W434(arguments), depth + 1));
                        pending.push((Walk::W26(slot), depth + 1));
                        pending.push((Walk::W44(interface), depth + 1));
                        pending.push((Walk::W428(callee), depth + 1));
                    }
                    SemanticOriginalExprKind::Literal(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W140(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Ascribed { ty, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                    SemanticOriginalExprKind::Integer(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Float(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::NumericConvert { value, target } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(target), depth + 1));
                        pending.push((Walk::W428(value), depth + 1));
                    }
                    SemanticOriginalExprKind::IntegerConvert { value, target } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(target), depth + 1));
                        pending.push((Walk::W428(value), depth + 1));
                    }
                    SemanticOriginalExprKind::Bool(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::String(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Unit => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalExprKind::Name(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Unary { op, operand } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(operand), depth + 1));
                        pending.push((Walk::W141(op), depth + 1));
                    }
                    SemanticOriginalExprKind::Binary { op, left, right } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(right), depth + 1));
                        pending.push((Walk::W428(left), depth + 1));
                        pending.push((Walk::W135(op), depth + 1));
                    }
                    SemanticOriginalExprKind::TypeApply {
                        callee,
                        type_arguments,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W302(type_arguments), depth + 1));
                        pending.push((Walk::W428(callee), depth + 1));
                    }
                    SemanticOriginalExprKind::Call { callee, arguments } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W434(arguments), depth + 1));
                        pending.push((Walk::W428(callee), depth + 1));
                    }
                    SemanticOriginalExprKind::Tuple(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W434(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Array(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W434(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Index { object, index } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(index), depth + 1));
                        pending.push((Walk::W428(object), depth + 1));
                    }
                    SemanticOriginalExprKind::Slice { object, start, end } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W435(end), depth + 1));
                        pending.push((Walk::W435(start), depth + 1));
                        pending.push((Walk::W428(object), depth + 1));
                    }
                    SemanticOriginalExprKind::Member {
                        object,
                        member,
                        owner,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W426(owner), depth + 1));
                        pending.push((Walk::W10(member), depth + 1));
                        pending.push((Walk::W428(object), depth + 1));
                    }
                    SemanticOriginalExprKind::ReceiverMember(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Record { ty, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W436(fields), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                    SemanticOriginalExprKind::RecordUpdate { ty, base, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W436(fields), depth + 1));
                        pending.push((Walk::W428(base), depth + 1));
                        pending.push((Walk::W426(ty), depth + 1));
                    }
                    SemanticOriginalExprKind::Sequence { bindings, value } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value), depth + 1));
                        pending.push((Walk::W436(bindings), depth + 1));
                    }
                    SemanticOriginalExprKind::CompileTime(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Try(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Closure {
                        move_capture,
                        parameters,
                        return_type,
                        body,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            4,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(body), depth + 1));
                        pending.push((Walk::W426(return_type), depth + 1));
                        pending.push((Walk::W415(parameters), depth + 1));
                        pending.push((Walk::W50(move_capture), depth + 1));
                    }
                    SemanticOriginalExprKind::If {
                        condition,
                        then_value,
                        else_value,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(else_value), depth + 1));
                        pending.push((Walk::W428(then_value), depth + 1));
                        pending.push((Walk::W428(condition), depth + 1));
                    }
                    SemanticOriginalExprKind::Unsupported(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Spread(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Group(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Character(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::Range { start, end } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(end), depth + 1));
                        pending.push((Walk::W428(start), depth + 1));
                    }
                    SemanticOriginalExprKind::Pipeline { input, target } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(target), depth + 1));
                        pending.push((Walk::W428(input), depth + 1));
                    }
                    SemanticOriginalExprKind::RecordConstructor { definition, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W436(fields), depth + 1));
                        pending.push((Walk::W147(definition), depth + 1));
                    }
                    SemanticOriginalExprKind::Splice(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::CompileTimeBlock(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W422(value_0), depth + 1));
                    }
                    SemanticOriginalExprKind::SyntaxQuote { category, template } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W439(template), depth + 1));
                        pending.push((Walk::W438(category), depth + 1));
                    }
                    SemanticOriginalExprKind::SyntaxIsland { path, tree } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W440(tree), depth + 1));
                        pending.push((Walk::W151(path), depth + 1));
                    }
                    SemanticOriginalExprKind::GenericApply { callee, arguments } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W441(arguments), depth + 1));
                        pending.push((Walk::W428(callee), depth + 1));
                    }
                }
            }
            Walk::W434(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W428(child), depth));
                }
            }
            Walk::W435(value) => {
                meter.node(depth)?;
                if let Some(child) = value {
                    prepare(
                        &mut pending,
                        meter,
                        1,
                        std::mem::size_of::<(Walk<'_>, usize)>(),
                    )?;
                    pending.push((Walk::W428(child), depth));
                }
            }
            Walk::W436(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W437(child), depth));
                }
            }
            Walk::W437(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W428(&value.1), depth + 1));
                pending.push((Walk::W10(&value.0), depth + 1));
            }
            Walk::W438(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalSyntaxCategory::Expr => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalSyntaxCategory::Type => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalSyntaxCategory::StmtList => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalSyntaxCategory::ItemList => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W439(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W448(&value.name_sites), depth + 1));
                pending.push((Walk::W446(&value.holes), depth + 1));
                pending.push((Walk::W440(&value.tree), depth + 1));
            }
            Walk::W440(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W444(&value.children), depth + 1));
                pending.push((Walk::W443(&value.delimiter), depth + 1));
            }
            Walk::W441(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W442(child), depth));
                }
            }
            Walk::W442(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalGenericArgument::Type(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(value_0), depth + 1));
                    }
                    SemanticOriginalGenericArgument::Const(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W428(value_0), depth + 1));
                    }
                    SemanticOriginalGenericArgument::Lifetime(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                }
            }
            Walk::W443(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalSyntaxDelimiter::Parenthesis => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalSyntaxDelimiter::Bracket => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalSyntaxDelimiter::Brace => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W444(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W445(child), depth));
                }
            }
            Walk::W445(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalSyntaxTreeEntry::Token { text, span } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W11(span), depth + 1));
                        pending.push((Walk::W10(text), depth + 1));
                    }
                    SemanticOriginalSyntaxTreeEntry::Tree(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W440(value_0), depth + 1));
                    }
                    SemanticOriginalSyntaxTreeEntry::Hole { index, span } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W11(span), depth + 1));
                        pending.push((Walk::W26(index), depth + 1));
                    }
                }
            }
            Walk::W446(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W447(child), depth));
                }
            }
            Walk::W447(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    3,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W11(&value.span), depth + 1));
                pending.push((Walk::W428(&value.value), depth + 1));
                pending.push((Walk::W450(&value.kind), depth + 1));
            }
            Walk::W448(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W449(child), depth));
                }
            }
            Walk::W449(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W26(&value.token_index), depth + 1));
                pending.push((Walk::W11(&value.span), depth + 1));
            }
            Walk::W450(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalQuoteHoleKind::Expr => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalQuoteHoleKind::Type => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalQuoteHoleKind::StmtList => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalQuoteHoleKind::ItemList => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalQuoteHoleKind::Ident { namespace, role } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W452(role), depth + 1));
                        pending.push((Walk::W451(namespace), depth + 1));
                    }
                }
            }
            Walk::W451(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalQuoteNameNamespace::Value => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalQuoteNameNamespace::Type => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalQuoteNameNamespace::Member => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalQuoteNameNamespace::Module => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalQuoteNameNamespace::Label => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W452(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalQuoteNameRole::Binding => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalQuoteNameRole::Reference => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W453(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalPatternKind::Name(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalPatternKind::Wildcard => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalPatternKind::Tuple(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W454(value_0), depth + 1));
                    }
                    SemanticOriginalPatternKind::Record { ty, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W455(fields), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                    SemanticOriginalPatternKind::Reference { mutable, pattern } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W429(pattern), depth + 1));
                        pending.push((Walk::W50(mutable), depth + 1));
                    }
                    SemanticOriginalPatternKind::ErrorArm { is_error, pattern } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W429(pattern), depth + 1));
                        pending.push((Walk::W50(is_error), depth + 1));
                    }
                    SemanticOriginalPatternKind::Variant {
                        ty,
                        variant,
                        fields,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W454(fields), depth + 1));
                        pending.push((Walk::W10(variant), depth + 1));
                        pending.push((Walk::W295(ty), depth + 1));
                    }
                    SemanticOriginalPatternKind::Integer(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalPatternKind::Bool(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W50(value_0), depth + 1));
                    }
                    SemanticOriginalPatternKind::Constructor {
                        definition,
                        variant,
                        fields,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            3,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W454(fields), depth + 1));
                        pending.push((Walk::W10(variant), depth + 1));
                        pending.push((Walk::W147(definition), depth + 1));
                    }
                    SemanticOriginalPatternKind::RecordConstructor { definition, fields } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W455(fields), depth + 1));
                        pending.push((Walk::W147(definition), depth + 1));
                    }
                    SemanticOriginalPatternKind::Float(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalPatternKind::String(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                    SemanticOriginalPatternKind::Character(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W10(value_0), depth + 1));
                    }
                }
            }
            Walk::W454(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W429(child), depth));
                }
            }
            Walk::W455(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W456(child), depth));
                }
            }
            Walk::W456(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W429(&value.1), depth + 1));
                pending.push((Walk::W10(&value.0), depth + 1));
            }
            Walk::W457(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalNamespace::Value => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalNamespace::Type => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticOriginalNamespace::Lifetime => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W458(value) => {
                meter.node(depth)?;
                match value {
                    SemanticOriginalNameValue::Recipe(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W300(value_0), depth + 1));
                    }
                    SemanticOriginalNameValue::Definition(value_0) => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W148(value_0), depth + 1));
                    }
                }
            }
            Walk::W459(value) => {
                meter.node(depth)?;
                match value {
                    SemanticSiteRecipeKind::Construct { result } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(result), depth + 1));
                    }
                    SemanticSiteRecipeKind::Call { target, actuals } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W299(actuals), depth + 1));
                        pending.push((Walk::W148(target), depth + 1));
                    }
                    SemanticSiteRecipeKind::CallableCall { callee } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(callee), depth + 1));
                    }
                    SemanticSiteRecipeKind::TypeUse { expected } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(expected), depth + 1));
                    }
                    SemanticSiteRecipeKind::DeclareLocal { expected } => {
                        prepare(
                            &mut pending,
                            meter,
                            1,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W295(expected), depth + 1));
                    }
                    SemanticSiteRecipeKind::Closure {
                        definition,
                        environment,
                    } => {
                        prepare(
                            &mut pending,
                            meter,
                            2,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                        pending.push((Walk::W299(environment), depth + 1));
                        pending.push((Walk::W148(definition), depth + 1));
                    }
                    SemanticSiteRecipeKind::OriginalCall => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W460(value) => {
                meter.node(depth)?;
                match value {
                    SemanticBuiltinTrait::Drop => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::Copy => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::Value => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::SyncDrop => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::StoreElement => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::Transfer => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::Share => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::ThreadCallable => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::ClosedEnvironment => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::Send => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                    SemanticBuiltinTrait::Sync => {
                        prepare(
                            &mut pending,
                            meter,
                            0,
                            std::mem::size_of::<(Walk<'_>, usize)>(),
                        )?;
                    }
                }
            }
            Walk::W461(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W462(child), depth));
                }
            }
            Walk::W462(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W295(&value.ty), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
            }
            Walk::W463(value) => {
                prepare(
                    &mut pending,
                    meter,
                    value.len(),
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                for child in value.iter().rev() {
                    pending.push((Walk::W464(child), depth));
                }
            }
            Walk::W464(value) => {
                meter.node(depth)?;
                prepare(
                    &mut pending,
                    meter,
                    2,
                    std::mem::size_of::<(Walk<'_>, usize)>(),
                )?;
                pending.push((Walk::W302(&value.fields), depth + 1));
                pending.push((Walk::W10(&value.name), depth + 1));
            }
        }
    }
    meter.stack_bytes = 0;
    Ok(())
}
struct Digest<'a> {
    inventory: FingerprintBuilder,
    generation: FingerprintBuilder,
    work: &'a mut usize,
}
impl fmt::Write for Digest<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let Some(left) = self
            .work
            .checked_sub(text.len().saturating_mul(2).saturating_add(1))
        else {
            return Err(fmt::Error);
        };
        *self.work = left;
        self.inventory.update(text.as_bytes());
        self.generation.update(text.as_bytes());
        Ok(())
    }
}
pub(super) fn freeze(
    program: &CoreProgram,
    limits: &Limits,
    nodes: &mut usize,
    work: &mut usize,
) -> CompileResult<(CtStageIdentity, [NodeId; 2])> {
    let mut meter = Meter {
        limits,
        nodes,
        work,
        stack_bytes: 0,
        tree_bytes: 0,
        used: Some(BTreeSet::new()),
    };
    preflight(program, &mut meter)?;
    let mut ordinal = u32::MAX;
    let used = meter.used.as_ref().ok_or_else(exhausted)?;
    let mut next = || -> CompileResult<NodeId> {
        loop {
            *meter.work = meter
                .work
                .checked_sub(used.len().checked_ilog2().unwrap_or(0) as usize + 1)
                .ok_or_else(exhausted)?;
            let node = NodeId(ordinal);
            ordinal = ordinal.checked_sub(1).ok_or_else(exhausted)?;
            if !used.contains(&node) {
                return Ok(node);
            }
        }
    };
    let query_nodes = [next()?, next()?];
    drop(meter);
    *work = work
        .checked_sub(
            b"zil/core-frozen-inventory/v1\0".len()
                + b"zil/core-frozen-committed-content/v1\0".len(),
        )
        .ok_or_else(exhausted)?;
    let mut digest = Digest {
        inventory: FingerprintBuilder::new(),
        generation: FingerprintBuilder::new(),
        work,
    };
    digest.inventory.update(b"zil/core-frozen-inventory/v1\0");
    digest
        .generation
        .update(b"zil/core-frozen-committed-content/v1\0");
    if write!(
        &mut digest,
        "compiler={} runtime-schema={}\0{program:?}",
        env!("CARGO_PKG_VERSION"),
        SCHEMA_VERSION
    )
    .is_err()
    {
        return Err(exhausted());
    }
    let epoch = EPOCH
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| exhausted())?;
    Ok((
        CtStageIdentity {
            source_fingerprint: program.source_fingerprint,
            epoch,
            committed_generation_fingerprint: digest.generation.digest(),
            declaration_inventory_fingerprint: digest.inventory.digest(),
        },
        query_nodes,
    ))
}

/// Derive the expected raw anchor independently of the selected-code callback.
/// A later contextual verifier also checks original syntax and actual SSA.
pub(super) fn query(
    context: &SourceProgramContext<'_>,
    module: ModuleId,
    expression: NodeId,
    entry: SemanticFunctionTarget,
    limits: &Limits,
    nodes: &mut usize,
    work: &mut usize,
) -> CompileResult<Option<CtFrozenContext>> {
    let mut meter = Meter {
        limits,
        nodes,
        work,
        stack_bytes: 0,
        tree_bytes: 0,
        used: None,
    };
    let mut module_row = None;
    for candidate in &context.program.modules {
        meter.node(0)?;
        if candidate.core.module_id == module {
            if module_row.replace(candidate).is_some() {
                return Err(exhausted());
            }
        }
    }
    let source = module_row.ok_or_else(|| {
        vec![Diagnostic::error(
            Phase::Ct,
            "E-CT-QUERY-ORIGIN",
            None,
            "query module is absent from the actual frozen program",
        )]
    })?;
    let Some(semantic) = &source.core.semantic else {
        return Ok(None);
    };
    let invalid = || {
        vec![Diagnostic::error(
            Phase::Ct,
            "E-CT-QUERY-ORIGIN",
            None,
            "query has no unique complete original emission anchor",
        )]
    };
    let mut found = None;
    for site in &semantic.sites {
        meter.node(0)?;
        if !matches!(site.at,SemanticSiteAddress::Core{expression:node,occurrence:0,role:SemanticEmissionRole::Ascription}if node==expression)
        {
            continue;
        }
        if !matches!(&site.action,SemanticSiteAction::TypeUse{subject:SemanticOperand::CoreExpression{expression:node,occurrence:0},..}if *node==expression)
        {
            continue;
        }
        let mut owner = None;
        for instance in &semantic.instances {
            meter.node(0)?;
            if instance.id == site.instance && owner.replace(instance).is_some() {
                return Err(invalid());
            }
        }
        let owner = owner.ok_or_else(invalid)?;
        let mut original = None;
        for definition in &semantic.definitions {
            meter.node(0)?;
            if definition.id != owner.definition {
                continue;
            }
            if original.replace(definition).is_some() {
                return Err(invalid());
            }
        }
        let original = original.ok_or_else(invalid)?;
        let SemanticDefinitionBody::FunctionWithBody {
            sites,
            site_nodes,
            body: SemanticOriginalFunctionBody::Defined(_),
            ..
        } = &original.body
        else {
            return Err(invalid());
        };
        let mut linked = None;
        for link in site_nodes {
            meter.node(0)?;
            if link.site == site.recipe && linked.replace(link.node).is_some() {
                return Err(invalid());
            }
        }
        let Some(SemanticOriginalNode::Expression(original_node)) = linked else {
            return Err(invalid());
        };
        let mut count = 0usize;
        for recipe in sites {
            meter.node(0)?;
            if recipe.id != site.recipe {
                continue;
            }
            count += 1;
            if !matches!(&recipe.kind,SemanticSiteRecipeKind::TypeUse{expected:SemanticTypeRecipe::OriginalType{scope:SemanticScopeRef::Ancestor(0),source:SemanticOriginalTypeSource::Expression(node)}}if *node==original_node)
            {
                return Err(invalid());
            }
        }
        if count != 1 || found.is_some() {
            return Err(invalid());
        }
        found = Some(CtFrozenContext {
            stage: context.stage.clone(),
            query: CtQueryOrigin {
                owner: CtQueryOwner::Instance(owner.id),
                original: SemanticOriginalNode::Expression(original_node),
                expression,
                occurrence: 0,
                wrapper: entry.clone(),
            },
        });
    }
    found.map(Some).ok_or_else(invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> CoreProgram {
        CoreProgram {
            root: ModuleId(0),
            source_fingerprint: Fingerprint::ZERO,
            modules: Vec::new(),
        }
    }
    #[test]
    fn fresh_core_freezes_bind_content_and_do_not_reuse_epochs() {
        let limits = Limits::default();
        let freeze_one = |value: &CoreProgram| {
            let mut nodes = 0;
            let mut work = limits.max_analysis_steps;
            freeze(value, &limits, &mut nodes, &mut work).unwrap().0
        };
        let first = freeze_one(&input());
        let same = freeze_one(&input());
        assert!(first.epoch > 0);
        assert!(same.epoch > first.epoch);
        assert_eq!(
            first.declaration_inventory_fingerprint,
            same.declaration_inventory_fingerprint
        );
        assert_eq!(
            first.committed_generation_fingerprint,
            same.committed_generation_fingerprint
        );
        let mut changed = input();
        changed.root = ModuleId(1);
        let other = freeze_one(&changed);
        assert_ne!(
            first.declaration_inventory_fingerprint,
            other.declaration_inventory_fingerprint
        );
        assert_ne!(
            first.committed_generation_fingerprint,
            other.committed_generation_fingerprint
        );
    }
    #[test]
    fn frozen_inventory_preflight_has_an_exact_node_boundary() {
        let limits = Limits::default();
        let value = input();
        let mut count = 0;
        let mut work = limits.max_analysis_steps;
        let exact = freeze(&value, &limits, &mut count, &mut work).unwrap();
        let mut nodes = 0;
        let mut remaining = limits.max_analysis_steps;
        assert!(
            freeze(
                &value,
                &Limits {
                    max_nodes: count,
                    ..limits.clone()
                },
                &mut nodes,
                &mut remaining
            )
            .is_ok()
        );
        let mut nodes = 0;
        let mut remaining = limits.max_analysis_steps;
        assert!(
            freeze(
                &value,
                &Limits {
                    max_nodes: count - 1,
                    ..limits
                },
                &mut nodes,
                &mut remaining
            )
            .is_err()
        );
        assert_eq!(exact.1, [NodeId(u32::MAX), NodeId(u32::MAX - 1)]);
    }
}
