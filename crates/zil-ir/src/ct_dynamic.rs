//! Complete untrusted CT inventory for dynamic metadata and storage sources.
//! Type payloads are edges in the finite metadata Type DAG in this catalogue.
//! Catalogue membership grants no visibility, trait, provider or borrow right.
use super::{CtStageFacetRequest, CtStageIdentity};
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CtDynamicTypeId(pub u32);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CtDynamicCatalogue {
    pub semantic: Option<CtSemanticModule>,
    pub stage: Option<CtStageIdentity>,
    pub facet_requests: Vec<CtStageFacetRequest>,
    pub types: Vec<CtDynamicTypeNode>,
    pub life_declarations: Vec<CtLifeNominalDeclaration>,
    pub life_instances: Vec<CtLifeNominalInstance>,
    pub declarations: Vec<CtDynDeclaration>,
    pub interfaces: Vec<CtDynInterface>,
    pub vtables: Vec<CtCoreDynVtable>,
    pub function_instances: Vec<CtDynFunctionInstanceRecord>,
    pub function_sources: Vec<CtFunctionSourceBindings>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynFunctionInstanceRecord {
    pub module: ModuleId,
    pub source: NodeId,
    pub instance: CtDynFunctionInstance,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtFunctionSourceBindings {
    pub module: ModuleId,
    pub source: NodeId,
    pub retained_return_sources: Option<Vec<CtStorageSourceBinding>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynInterfaceId {
    pub module: ModuleId,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynVtableId {
    pub module: ModuleId,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynDeclaration {
    pub implicit_lifetimes: Option<Vec<CtDynImplicitLifeDeclaration>>,
    pub direct_supertraits: Option<Vec<CtDynSelfPattern>>,
    pub requirements: Option<Vec<CtDynObligation>>,
    pub associated_definitions: Option<Vec<CtDynAssociatedDefinition>>,
    pub method_signatures: Option<Vec<CtDynMethodRecipe>>,
    pub method_definitions: Option<Vec<CtDynDefinitionDeclaration>>,
    pub self_pattern: Option<CtDynTypePattern>,
    pub implemented_trait: Option<CtDynSelfPattern>,
    pub id: LifeDeclarationId,
    pub identity: LifeDeclarationIdentity,
    pub kind: CtDynDeclarationKind,
    pub parameters: Vec<CtLifeParameterKind>,
    pub members: Vec<CtDynMemberDeclaration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynSelfPattern {
    pub declaration: LifeDeclarationId,
    pub arguments: Vec<CtDynPatternActual>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynPatternActual {
    Type(CtDynTypePattern),
    Const(CtDynConstPattern),
    Life(CtDynLifePattern),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynConstPattern {
    IntegerLiteral(String),
    FloatLiteral(String),
    MethodParameter(u32),
    Unary {
        op: UnaryOp,
        value: Box<CtDynConstPattern>,
        ty: Box<CtDynTypePattern>,
    },
    Binary {
        op: BinaryOp,
        left: Box<CtDynConstPattern>,
        right: Box<CtDynConstPattern>,
        ty: Box<CtDynTypePattern>,
    },
    Convert {
        value: Box<CtDynConstPattern>,
        ty: Box<CtDynTypePattern>,
    },
    AssociatedProjection {
        receiver: Box<CtDynTypePattern>,
        application: Box<CtDynSelfPattern>,
        member: u32,
    },
    Value(Constant),
    Parameter(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynTypePattern {
    SelfType,
    MethodParameter(u32),
    AssociatedProjection {
        receiver: Box<CtDynTypePattern>,
        application: Box<CtDynSelfPattern>,
        member: u32,
    },
    Concrete(CtDynamicTypeId),
    Parameter(u32),
    Nominal(Box<CtDynSelfPattern>),
    Address {
        mutable: bool,
        lifetime: Option<CtDynLifePattern>,
        inner: Box<CtDynTypePattern>,
    },
    Tuple(Vec<CtDynTypePattern>),
    Array {
        element: Box<CtDynTypePattern>,
        length: CtDynConstPattern,
    },
    Slice(Box<CtDynTypePattern>),
    Managed(Box<CtDynTypePattern>),
    Desc {
        lifetime: Option<CtDynLifePattern>,
        inner: Box<CtDynTypePattern>,
    },
    OwnedStore(Box<CtDynTypePattern>),
    ErrorUnion {
        error: Box<CtDynTypePattern>,
        result: Box<CtDynTypePattern>,
    },
    Semantic {
        scope: CtSemanticDefinitionId,
        ty: Box<CtSemanticTypeRecipe>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynDeclarationKind {
    Trait,
    Impl,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynMemberDeclaration {
    pub name: String,
    pub kind: CtDynMemberKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynMemberKind {
    Type,
    Constant,
    Method(Vec<CtLifeParameterKind>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynGenericActual {
    Type(CtDynamicTypeId),
    QualifiedType(CtDynTypeValue),
    Const(Constant),
    Life(StorageBinder),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynTraitApplication {
    pub declaration: LifeDeclarationId,
    pub actuals: Vec<CtDynGenericActual>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynMember {
    pub application: CtDynTraitApplication,
    pub ordinal: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynAssociatedType {
    pub member: CtDynMember,
    pub ty: CtDynamicTypeId,
    pub life: StorageLifeShape,
    pub classes: Vec<StorageBinder>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynAssociatedConstant {
    pub member: CtDynMember,
    pub ty: CtDynamicTypeId,
    pub value: Constant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynReceiver {
    pub mutable: bool,
    pub mode: PassingMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynSlot {
    pub member: CtDynMember,
    pub actuals: Vec<CtDynGenericActual>,
    pub receiver: CtDynReceiver,
    pub parameters: Vec<CtDynamicTypeId>,
    pub parameter_modes: Vec<PassingMode>,
    pub result: CtDynamicTypeId,
    pub result_mode: PassingMode,
    pub call_kind: CallKind,
    pub storage_life: Option<StorageLifeContract>,
    pub contract: CtDynContract,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynInterface {
    pub context: Option<CtDynamicTypeId>,
    pub owning_cleanup: Option<CtDynCleanupContract>,
    pub id: CtDynInterfaceId,
    pub application: CtDynTraitApplication,
    pub inherited: Vec<CtDynTraitApplication>,
    pub associated_types: Vec<CtDynAssociatedType>,
    pub associated_constants: Vec<CtDynAssociatedConstant>,
    pub retained_life: CtRetainedLifeShape,
    pub slots: Vec<CtDynSlot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtCoreDynEntry {
    pub slot: u32,
    pub module: ModuleId,
    pub function: NodeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynEntry {
    pub slot: u32,
    pub function: FunctionId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynRetainedPortBinding {
    pub port: u32,
    pub sources: Vec<StoragePosition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtCoreDynVtable {
    pub self_life: Option<StorageLifeShape>,
    pub implementation_witnesses: Option<Vec<CtDynImplementationWitness>>,
    pub self_classes: Vec<StorageBinder>,
    pub id: CtDynVtableId,
    pub interface: CtDynInterfaceId,
    pub self_type: CtDynamicTypeId,
    pub implementation: LifeDeclarationId,
    pub actuals: Vec<CtDynGenericActual>,
    pub ports: Vec<CtDynRetainedPortBinding>,
    pub entries: Vec<CtCoreDynEntry>,
    pub cleanup: Option<CtCoreDynEntry>,
    pub owned_storage: Option<CtDynOwnedStorage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynVtable {
    pub self_life: Option<StorageLifeShape>,
    pub implementation_witnesses: Option<Vec<CtDynImplementationWitness>>,
    pub self_classes: Vec<StorageBinder>,
    pub id: CtDynVtableId,
    pub interface: CtDynInterfaceId,
    pub self_type: CtDynamicTypeId,
    pub implementation: LifeDeclarationId,
    pub actuals: Vec<CtDynGenericActual>,
    pub ports: Vec<CtDynRetainedPortBinding>,
    pub entries: Vec<CtDynEntry>,
    pub cleanup: Option<CtDynEntry>,
    pub owned_storage: Option<CtDynOwnedStorage>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynOwnedStorage {
    Heap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynRequirement {
    Read(StoragePosition),
    Write(StoragePosition),
    Manage(StoragePosition),
    Apart(StoragePosition, StoragePosition),
    State {
        position: StoragePosition,
        ty: CtDynamicTypeId,
        variant: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynGuarantee {
    StorageSources(CtStorageSourceBinding),
    ManageResult,
    State {
        position: StoragePosition,
        ty: CtDynamicTypeId,
        variant: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynEffect {
    Alloc,
    Io,
    Block,
    Suspend,
    Spawn,
    Ffi,
    Reenter,
    Panic,
    Call(StoragePosition),
    Cleanup(CtDynamicTypeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynCallAccess {
    pub reads: Vec<StoragePosition>,
    pub writes: Vec<StoragePosition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynResultSource {
    Value,
    Create,
    Give,
    Borrow {
        sources: Vec<CtStorageSource>,
        mutable: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynCleanupContract {
    pub environment: bool,
    pub positions: Vec<StoragePosition>,
    pub access: CtDynCallAccess,
    pub effects: Vec<CtDynEffect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynResumeBound {
    Local,
    Transfer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynThreadRequirements {
    pub transfer: bool,
    pub share: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynContract {
    pub requires: Option<Vec<CtDynRequirement>>,
    pub guarantees: Option<Vec<CtDynGuarantee>>,
    pub hold: Option<Vec<StoragePosition>>,
    pub call: Option<CtDynCallAccess>,
    pub result: Option<CtDynResultSource>,
    pub cleanup: Option<CtDynCleanupContract>,
    pub effects: Option<Vec<CtDynEffect>>,
    pub resume: Option<CtDynResumeBound>,
    pub threads: Option<CtDynThreadRequirements>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynTypeValue {
    pub ty: CtDynamicTypeId,
    pub life: StorageLifeShape,
    pub classes: Vec<StorageBinder>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynInstantiationScope {
    pub fixed_binder_count: u32,
    pub invocation_binder_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynLifeEmbedding {
    pub from: StorageBinder,
    pub to: StorageBinder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynImplicitLifeKind {
    Address,
    Description,
    LifetimeArgument,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynImplicitLifeRoot {
    SelfPattern,
    ImplementedTrait,
    Supertrait(u32),
    Requirement(u32),
    AssociatedDefinition(u32),
    MethodSignature(u32),
    MethodDefinition(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynImplicitLifeOccurrence {
    pub root: CtDynImplicitLifeRoot,
    pub path: Vec<u32>,
    pub kind: CtDynImplicitLifeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynImplicitLifeDeclaration {
    pub occurrences: Vec<CtDynImplicitLifeOccurrence>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynLifePattern {
    DeclarationParameter(u32),
    MethodParameter(u32),
    Bound { depth: u32, index: u32 },
    Invocation(u32),
    ImplicitDeclaration(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynObligation {
    Structural {
        receiver: CtDynTypePattern,
        bound: CtDynStructuralBound,
    },
    Trait {
        receiver: CtDynTypePattern,
        application: CtDynSelfPattern,
    },
    Callable {
        receiver: CtDynTypePattern,
        kind: CallableKind,
        quantified_lifetimes: u32,
        signature: Box<CtDynSignatureRecipe>,
    },
    TypeEqual {
        left: CtDynTypePattern,
        right: CtDynTypePattern,
    },
    ConstEqual {
        ty: CtDynTypePattern,
        left: CtDynConstPattern,
        right: CtDynConstPattern,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynParameterRecipe {
    pub ty: CtDynTypePattern,
    pub mode: PassingMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynSignatureRecipe {
    pub parameters: Vec<CtDynParameterRecipe>,
    pub result: CtDynTypePattern,
    pub result_mode: PassingMode,
    pub call_kind: CallKind,
    pub invocation_binder_count: u32,
    pub contract: CtDynContractRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynRequirementRecipe {
    Read(StoragePosition),
    Write(StoragePosition),
    Manage(StoragePosition),
    Apart(StoragePosition, StoragePosition),
    State {
        position: StoragePosition,
        ty: CtDynTypePattern,
        variant: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynGuaranteeRecipe {
    StorageSources(CtDynStorageSourceBindingRecipe),
    ManageResult,
    State {
        position: StoragePosition,
        ty: CtDynTypePattern,
        variant: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynEffectRecipe {
    Alloc,
    Io,
    Block,
    Suspend,
    Spawn,
    Ffi,
    Reenter,
    Panic,
    Call(StoragePosition),
    Cleanup(CtDynTypePattern),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynCleanupRecipe {
    pub environment: bool,
    pub positions: Vec<StoragePosition>,
    pub access: CtDynCallAccess,
    pub effects: Vec<CtDynEffectRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynContractRecipe {
    pub requires: Option<Vec<CtDynRequirementRecipe>>,
    pub guarantees: Option<Vec<CtDynGuaranteeRecipe>>,
    pub hold: Option<Vec<StoragePosition>>,
    pub call: Option<CtDynCallAccess>,
    pub result: Option<CtDynResultSourceRecipe>,
    pub cleanup: Option<CtDynCleanupRecipe>,
    pub effects: Option<Vec<CtDynEffectRecipe>>,
    pub resume: Option<CtDynResumeBound>,
    pub threads: Option<CtDynThreadRequirements>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynStorageSourceRecipe {
    Exact(StoragePosition),
    PlaceAddress {
        place: StoragePosition,
        mode: StoredBorrowMode,
    },
    OwnedView {
        head: StoragePosition,
        leaf: CtDynTypePattern,
        mode: StoredBorrowMode,
    },
    ReborrowedLeaf {
        source: StoragePosition,
        head: StoragePosition,
    },
    Static {
        leaf: CtDynTypePattern,
        mode: StoredBorrowMode,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynStorageSourceBindingRecipe {
    pub position: StoragePosition,
    pub sources: Vec<CtDynStorageSourceRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynResultSourceRecipe {
    Value,
    Create,
    Give,
    Borrow {
        sources: Vec<CtDynStorageSourceRecipe>,
        mutable: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynDefinitionId {
    pub module: ModuleId,
    pub source: NodeId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynDefinitionScope {
    TraitDefault,
    ImplOverride,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynDefinitionDeclaration {
    pub requirements: Vec<CtDynObligation>,
    pub instances: Vec<CtDynDefinitionSpecialization>,
    pub id: CtDynDefinitionId,
    pub identity: LifeDeclarationIdentity,
    pub member: u32,
    pub scope: CtDynDefinitionScope,
    pub signature: CtDynSignatureRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynMethodRecipe {
    pub requirements: Vec<CtDynObligation>,
    pub member: u32,
    pub signature: CtDynSignatureRecipe,
    pub default_definition: Option<CtDynDefinitionId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynFunctionInstance {
    pub scope: CtDynInstantiationScope,
    pub header_embedding: Vec<CtDynLifeEmbedding>,
    pub definition: CtDynDefinitionId,
    pub receiver: CtDynTypeValue,
    pub declaration_actuals: Vec<CtDynGenericActual>,
    pub method_actuals: Vec<CtDynGenericActual>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynAssociatedValueRecipe {
    Type(CtDynTypePattern),
    Constant {
        ty: CtDynTypePattern,
        value: CtDynConstPattern,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynAssociatedDefinition {
    pub carrier: Option<CtDynTypePattern>,
    pub member: u32,
    pub value: Option<CtDynAssociatedValueRecipe>,
    pub requirements: Vec<CtDynObligation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynAssociatedSource {
    TraitDefault {
        declaration: LifeDeclarationId,
        member: u32,
    },
    ImplOverride {
        declaration: LifeDeclarationId,
        member: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynAssociatedValue {
    Type(CtDynTypeValue),
    Constant { ty: CtDynTypeValue, value: Constant },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynAssociatedResolution {
    pub member: CtDynMember,
    pub source: CtDynAssociatedSource,
    pub value: CtDynAssociatedValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynImplementationWitness {
    pub scope: CtDynInstantiationScope,
    pub to_context: Vec<CtDynLifeEmbedding>,
    pub receiver: CtDynTypeValue,
    pub application: CtDynTraitApplication,
    pub implementation: LifeDeclarationId,
    pub actuals: Vec<CtDynGenericActual>,
    pub associated: Vec<CtDynAssociatedResolution>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynFunctionTarget {
    Core { module: ModuleId, source: NodeId },
    Typed(FunctionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDynDefinitionSpecialization {
    pub function: CtDynFunctionTarget,
    pub receiver: CtDynTypeValue,
    pub declaration_actuals: Vec<CtDynGenericActual>,
    pub method_actuals: Vec<CtDynGenericActual>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynStructuralBound {
    Copy,
    Value,
    SyncDrop,
    StoreElement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticDefinitionId {
    pub module: ModuleId,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticInstanceId {
    pub module: ModuleId,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticModule {
    pub definitions: Vec<CtSemanticDefinition>,
    pub instances: Vec<CtSemanticInstance>,
    pub sites: Vec<CtSemanticSite>,
    pub dyn_scopes: Vec<CtSemanticDynScopeBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticDynScopeBinding {
    pub owner: CtSemanticDynOwner,
    pub scope: CtSemanticDefinitionId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticDynOwner {
    Declaration(LifeDeclarationId),
    MethodSignature {
        declaration: LifeDeclarationId,
        member: u32,
    },
    MethodDefinition(CtDynDefinitionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticDefinition {
    pub id: CtSemanticDefinitionId,
    pub identity: LifeDeclarationIdentity,
    pub parent: Option<CtSemanticDefinitionId>,
    pub kind: CtSemanticDefinitionKind,
    pub parameters: Vec<CtSemanticFormal>,
    pub implicit_lifetimes: Vec<CtSemanticImplicitLifetime>,
    pub requirements: Vec<CtSemanticRequirementRecipe>,
    pub environment: Vec<CtSemanticEnvironmentFormal>,
    pub body: CtSemanticDefinitionBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticDefinitionKind {
    Function,
    Method,
    Closure,
    TaskEntry,
    Nominal,
    Scope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticDefinitionBody {
    Function {
        signature: CtSemanticSignatureRecipe,
        sites: Vec<CtSemanticSiteRecipe>,
    },
    Nominal {
        declaration: LifeDeclarationId,
        kind: CtLifeNominalKind,
        body: CtSemanticNominalBody,
    },
    Scope(CtSemanticScopeRecipe),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticScopeRecipe {
    Trait {
        declaration: LifeDeclarationId,
    },
    InherentImpl {
        declaration: LifeDeclarationId,
        self_type: CtSemanticTypeRecipe,
    },
    TraitImpl {
        declaration: LifeDeclarationId,
        self_type: CtSemanticTypeRecipe,
        trait_application: CtSemanticTraitRecipe,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticNominalBody {
    Product {
        fields: Vec<CtSemanticFieldRecipe>,
    },
    Enum {
        variants: Vec<CtSemanticVariantRecipe>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticFieldRecipe {
    pub name: String,
    pub ty: CtSemanticTypeRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticVariantRecipe {
    pub name: String,
    pub fields: Vec<CtSemanticTypeRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticFormal {
    Type {
        requirements: Vec<CtSemanticRequirementRecipe>,
    },
    Const {
        carrier: CtSemanticTypeRecipe,
    },
    Lifetime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticImplicitLifetime {
    pub ordinal: u32,
    pub occurrence_path: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticEnvironmentFormal {
    pub kind: CtSemanticFormal,
    pub source: CtSemanticActualRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticInstance {
    pub id: CtSemanticInstanceId,
    pub definition: CtSemanticDefinitionId,
    pub target: CtSemanticInstanceTarget,
    pub lifetime_instantiation: CtSemanticLifetimeInstantiation,
    pub parent: Option<CtSemanticInstanceId>,
    pub actuals: Vec<CtSemanticActual>,
    pub header_embedding: Vec<CtSemanticHeaderBinding>,
    pub environment: Option<CtSemanticEnvironmentInstance>,
    pub self_type: Option<CtSemanticTypeValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticLifetimeInstantiation {
    Rigid,
    Partition { rigid: CtSemanticInstanceId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticFunctionTarget {
    Core {
        module: ModuleId,
        function: NodeId,
    },
    CoreClosure {
        module: ModuleId,
        creator: NodeId,
        expression: NodeId,
        occurrence: u32,
    },
    Typed(FunctionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticInstanceTarget {
    Function(CtSemanticFunctionTarget),
    Scope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticClosureTarget {
    Core {
        module: ModuleId,
        creator: NodeId,
        expression: NodeId,
        occurrence: u32,
    },
    Typed(ClosureId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticHeaderBinding {
    pub header: StorageBinder,
    pub actual: CtSemanticClassExpr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticEnvironmentInstance {
    pub closure: CtSemanticClosureTarget,
    pub actuals: Vec<CtSemanticActual>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticActual {
    Type(CtSemanticTypeValue),
    Const {
        ty: CtSemanticTypeValue,
        value: Constant,
    },
    Life(CtSemanticClassExpr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticTypeValue {
    pub ty: CtDynamicTypeId,
    pub life: StorageLifeShape,
    pub classes: Vec<CtSemanticClassExpr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticScopeRef {
    Ancestor(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticClassExpr {
    Formal {
        scope: CtSemanticScopeRef,
        ordinal: u32,
    },
    Implicit {
        scope: CtSemanticScopeRef,
        ordinal: u32,
    },
    Bound {
        depth: u32,
        index: u32,
    },
    OuterOccurrence {
        operand: CtSemanticOperand,
        path: Vec<LifeTypeProjection>,
        mode: StoredBorrowMode,
    },
    IntrinsicClass {
        operand: CtSemanticOperand,
        path: Vec<LifeTypeProjection>,
        class: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticOperand {
    Parameter(u32),
    Capture(u32),
    Receiver,
    Value(ValueId),
    Local(LocalId),
    TypeArgument {
        scope: CtSemanticScopeRef,
        ordinal: u32,
    },
    Environment(u32),
    CoreExpression {
        expression: NodeId,
        occurrence: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticSiteRecipe {
    pub id: SemanticSiteId,
    pub occurrence_path: Vec<u32>,
    pub kind: CtSemanticSiteRecipeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticSiteRecipeKind {
    Construct {
        result: CtSemanticTypeRecipe,
    },
    Call {
        target: CtSemanticDefinitionId,
        actuals: Vec<CtSemanticActualRecipe>,
    },
    TypeUse {
        expected: CtSemanticTypeRecipe,
    },
    DeclareLocal {
        expected: CtSemanticTypeRecipe,
    },
    Closure {
        definition: CtSemanticDefinitionId,
        environment: Vec<CtSemanticActualRecipe>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticSite {
    pub instance: CtSemanticInstanceId,
    pub recipe: SemanticSiteId,
    pub at: CtSemanticSiteAddress,
    pub action: CtSemanticSiteAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticSiteAddress {
    Core {
        expression: NodeId,
        occurrence: u32,
        role: CtSemanticEmissionRole,
    },
    Typed {
        block: BlockId,
        instruction: u32,
        edge: CtSemanticSiteEdge,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticEmissionRole {
    Construct,
    Call,
    Ascription,
    Binding,
    Closure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticSiteEdge {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticSiteAction {
    Construct {
        result: CtSemanticOperand,
        value: CtSemanticTypeValue,
    },
    Call {
        target: CtSemanticFunctionTarget,
        actuals: Vec<CtSemanticActual>,
    },
    TypeUse {
        subject: CtSemanticOperand,
        expected: CtSemanticTypeValue,
    },
    DeclareLocal {
        local: CtSemanticLocalTarget,
        expected: CtSemanticTypeValue,
    },
    Closure {
        result: CtSemanticOperand,
        closure: CtSemanticClosureTarget,
        environment: Vec<CtSemanticActual>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticLocalTarget {
    Core { statement: NodeId, occurrence: u32 },
    Typed(LocalId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticRecipeOperand {
    TypeFormal(CtSemanticFormalRef),
    Environment {
        scope: CtSemanticScopeRef,
        ordinal: u32,
    },
    Parameter(u32),
    Receiver,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticFormalRef {
    pub scope: CtSemanticScopeRef,
    pub ordinal: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticLifeRecipe {
    Formal(CtSemanticFormalRef),
    Implicit {
        scope: CtSemanticScopeRef,
        ordinal: u32,
    },
    Environment {
        scope: CtSemanticScopeRef,
        ordinal: u32,
    },
    Bound {
        depth: u32,
        index: u32,
    },
    Invocation(u32),
    OuterOccurrence {
        operand: CtSemanticRecipeOperand,
        path: Vec<LifeTypeProjection>,
        mode: StoredBorrowMode,
    },
    IntrinsicClass {
        operand: CtSemanticRecipeOperand,
        path: Vec<LifeTypeProjection>,
        class: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticOuterLifeRecipe {
    Class(CtSemanticLifeRecipe),
    Static,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticTypeRecipe {
    Atom(CtDynamicTypeId),
    Exact(Box<CtSemanticExactTypeRecipe>),
    Formal(CtSemanticFormalRef),
    Environment {
        scope: CtSemanticScopeRef,
        ordinal: u32,
    },
    SelfType {
        scope: CtSemanticScopeRef,
    },
    Nominal {
        definition: CtSemanticDefinitionId,
        arguments: Vec<CtSemanticActualRecipe>,
    },
    AssociatedType {
        receiver: Box<CtSemanticTypeRecipe>,
        application: Box<CtSemanticTraitRecipe>,
        member: u32,
    },
    Tuple(Vec<CtSemanticTypeRecipe>),
    Array {
        element: Box<CtSemanticTypeRecipe>,
        length: Box<CtSemanticConstRecipe>,
    },
    Slice(Box<CtSemanticTypeRecipe>),
    Address {
        mutable: bool,
        lifetime: CtSemanticOuterLifeRecipe,
        inner: Box<CtSemanticTypeRecipe>,
    },
    Desc {
        lifetime: CtSemanticOuterLifeRecipe,
        inner: Box<CtSemanticTypeRecipe>,
    },
    Managed(Box<CtSemanticTypeRecipe>),
    OwnedStore(Box<CtSemanticTypeRecipe>),
    Mutex(Box<CtSemanticTypeRecipe>),
    MutexGuard {
        payload: Box<CtSemanticTypeRecipe>,
        retained: Box<CtSemanticRetainedShapeRecipe>,
    },
    Registration(Box<CtSemanticTypeRecipe>),
    AsyncRegistration(Box<CtSemanticTypeRecipe>),
    Task {
        result: Box<CtSemanticTypeRecipe>,
        retained: Box<CtSemanticRetainedShapeRecipe>,
    },
    ErrorUnion {
        error: Box<CtSemanticTypeRecipe>,
        result: Box<CtSemanticTypeRecipe>,
    },
    Qualified {
        storage: Box<CtSemanticTypeRecipe>,
        life: Box<CtSemanticStorageShapeRecipe>,
    },
    Code {
        kind: CtSemanticCodeKind,
        signature: Box<CtSemanticSignatureRecipe>,
    },
    Closure {
        definition: CtSemanticDefinitionId,
        arguments: Vec<CtSemanticActualRecipe>,
        environment: Vec<CtSemanticActualRecipe>,
    },
    Dyn {
        owned: bool,
        interface: Box<CtSemanticDynInterfaceRecipe>,
    },
    Opaque {
        definition: CtSemanticDefinitionId,
        arguments: Vec<CtSemanticActualRecipe>,
        hidden: Box<CtSemanticTypeRecipe>,
    },
    AnonymousSum(Vec<CtSemanticTypeRecipe>),
    UnresolvedNamed(String),
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticExactTypeRecipe {
    pub ty: CtDynamicTypeId,
    pub life: StorageLifeShape,
    pub classes: Vec<CtSemanticLifeRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticCodeKind {
    FunctionPointer,
    BorrowedFunctionPointer,
    ForeignCallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticActualRecipe {
    Type(CtSemanticTypeRecipe),
    Const {
        carrier: CtSemanticTypeRecipe,
        value: CtSemanticConstRecipe,
    },
    Life(CtSemanticLifeRecipe),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticClassSequenceRecipe {
    pub parts: Vec<CtSemanticClassSequencePart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticClassSequencePart {
    One(CtSemanticLifeRecipe),
    AllClasses {
        ty: Box<CtSemanticTypeRecipe>,
        path: Vec<LifeTypeProjection>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticStorageShapeRecipe {
    pub domain: CtSemanticClassSequenceRecipe,
    pub slots: Vec<CtSemanticSlotRecipe>,
    pub retained: Vec<CtSemanticRetainedPart>,
    pub nominal: Vec<CtSemanticNominalPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticSlotRecipe {
    pub path: Vec<PlaceProjection>,
    pub mode: StoredBorrowMode,
    pub class: CtSemanticLifeRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticRetainedPart {
    One {
        path: Vec<LifeTypeProjection>,
        child_class: u32,
        parent: CtSemanticLifeRecipe,
    },
    All {
        path: Vec<LifeTypeProjection>,
        parents: CtSemanticClassSequenceRecipe,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticNominalPart {
    One {
        path: Vec<LifeTypeProjection>,
        declaration: LifeDeclarationId,
        argument: u32,
        class: CtSemanticLifeRecipe,
    },
    FromType {
        path: Vec<LifeTypeProjection>,
        ty: Box<CtSemanticTypeRecipe>,
        classes: CtSemanticClassSequenceRecipe,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticRetainedShapeRecipe {
    pub domain: CtSemanticClassSequenceRecipe,
    pub ports: Vec<CtSemanticPortPart>,
    pub retained: Vec<CtSemanticRetainedPart>,
    pub nominal: Vec<CtSemanticNominalPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticPortPart {
    One {
        leaf: Box<CtSemanticTypeRecipe>,
        mode: StoredBorrowMode,
        class: CtSemanticLifeRecipe,
    },
    FromType {
        ty: Box<CtSemanticTypeRecipe>,
        classes: CtSemanticClassSequenceRecipe,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticConstRecipe {
    Value(Constant),
    IntegerLiteral(String),
    FloatLiteral(String),
    Formal(CtSemanticFormalRef),
    Environment {
        scope: CtSemanticScopeRef,
        ordinal: u32,
    },
    Unary {
        op: UnaryOp,
        operand_type: Box<CtSemanticTypeRecipe>,
        value: Box<CtSemanticConstRecipe>,
    },
    Binary {
        op: BinaryOp,
        operand_type: Box<CtSemanticTypeRecipe>,
        left: Box<CtSemanticConstRecipe>,
        right: Box<CtSemanticConstRecipe>,
    },
    Convert {
        source_type: Box<CtSemanticTypeRecipe>,
        target_type: Box<CtSemanticTypeRecipe>,
        value: Box<CtSemanticConstRecipe>,
    },
    AssociatedConst {
        receiver: Box<CtSemanticTypeRecipe>,
        application: Box<CtSemanticTraitRecipe>,
        member: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticTraitRecipe {
    pub declaration: LifeDeclarationId,
    pub arguments: Vec<CtSemanticActualRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticRequirementRecipe {
    Structural {
        ty: CtSemanticTypeRecipe,
        bound: CtDynStructuralBound,
    },
    Trait {
        ty: CtSemanticTypeRecipe,
        application: CtSemanticTraitRecipe,
    },
    Callable {
        ty: CtSemanticTypeRecipe,
        kind: CallableKind,
        signature: Box<CtSemanticSignatureRecipe>,
    },
    TypeEqual {
        left: CtSemanticTypeRecipe,
        right: CtSemanticTypeRecipe,
    },
    ConstEqual {
        carrier: CtSemanticTypeRecipe,
        left: CtSemanticConstRecipe,
        right: CtSemanticConstRecipe,
    },
    LifeEqual {
        left: CtSemanticLifeRecipe,
        right: CtSemanticLifeRecipe,
    },
    Outlives {
        longer: CtSemanticLifeRecipe,
        shorter: CtSemanticLifeRecipe,
    },
    ForAll {
        lifetimes: u32,
        requirements: Vec<CtSemanticRequirementRecipe>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticSignatureRecipe {
    pub bound_lifetimes: u32,
    pub invocation_lifetimes: u32,
    pub parameters: Vec<CtSemanticParameterRecipe>,
    pub result: CtSemanticTypeRecipe,
    pub result_mode: PassingMode,
    pub call_kind: CallKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticParameterRecipe {
    pub ty: CtSemanticTypeRecipe,
    pub mode: PassingMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticDynContextRecipe {
    Closed,
    Type(Box<CtSemanticTypeRecipe>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticDynInterfaceRecipe {
    pub application: CtSemanticTraitRecipe,
    pub context: CtSemanticDynContextRecipe,
    pub associated: Vec<CtSemanticAssociatedBindingRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSemanticAssociatedBindingRecipe {
    pub application: CtSemanticTraitRecipe,
    pub member: u32,
    pub value: CtSemanticAssociatedRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSemanticAssociatedRecipe {
    Type(CtSemanticTypeRecipe),
    Const {
        carrier: CtSemanticTypeRecipe,
        value: CtSemanticConstRecipe,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtStorageSource {
    Exact(StoragePosition),
    PlaceAddress {
        place: StoragePosition,
        mode: StoredBorrowMode,
    },
    OwnedView {
        head: StoragePosition,
        leaf: CtDynamicTypeId,
        mode: StoredBorrowMode,
    },
    ReborrowedLeaf {
        source: StoragePosition,
        head: StoragePosition,
    },
    Static {
        leaf: CtDynamicTypeId,
        mode: StoredBorrowMode,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtRetainedBorrowPort {
    pub leaf: CtDynamicTypeId,
    pub mode: StoredBorrowMode,
    pub binder: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtRetainedLifeShape {
    pub binder_count: u32,
    pub ports: Vec<CtRetainedBorrowPort>,
    pub retained: Vec<StorageRetainedBinding>,
    pub nominal_arguments: Vec<NominalLifeArgument>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtStorageSourceBinding {
    pub position: StoragePosition,
    pub sources: Vec<CtStorageSource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtLifeNominalKind {
    Record,
    Package,
    Resource,
    Enum,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtLifeParameterKind {
    Type,
    Const(CtDynamicTypeId),
    Lifetime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtLifeNominalDeclaration {
    pub id: LifeDeclarationId,
    pub identity: LifeDeclarationIdentity,
    pub kind: CtLifeNominalKind,
    pub parameters: Vec<CtLifeParameterKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtLifeNominalActual {
    Type(CtDynamicTypeId),
    QualifiedType {
        ty: CtDynamicTypeId,
        life: StorageLifeShape,
    },
    Const(Constant),
    Lifetime(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtLifeNominalInstance {
    pub declaration: LifeDeclarationId,
    pub ty: CtDynamicTypeId,
    pub arguments: Vec<CtLifeNominalActual>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtDynamicTypeNode {
    Dyn {
        interface: CtDynInterfaceId,
    },
    OwnedDyn {
        interface: CtDynInterfaceId,
    },
    BorrowedStorage {
        storage: Box<CtDynamicTypeId>,
        life: StorageLifeShape,
    },
    Unit,
    Bool,
    I32,
    I64,
    U64,
    Usize,
    F32,
    F64,
    U8,
    U16,
    U32,
    I8,
    I16,
    Isize,
    ByteBuffer,
    OwnedStore {
        element: Box<CtDynamicTypeId>,
    },
    ForeignHandle,
    SyncCell(SyncCellKind),
    Mutex {
        payload: Box<CtDynamicTypeId>,
    },
    MutexGuard {
        payload: Box<CtDynamicTypeId>,
        retained_life: CtRetainedLifeShape,
    },
    Registration {
        environment: Box<CtDynamicTypeId>,
    },
    AsyncRegistration {
        environment: Box<CtDynamicTypeId>,
    },
    TaskI32,
    Task {
        result: Box<CtDynamicTypeId>,
        retained_life: CtRetainedLifeShape,
    },
    CWideString,
    ForeignCallback {
        parameters: Vec<CtDynamicTypeId>,
        result: Box<CtDynamicTypeId>,
    },
    Tuple(Vec<CtDynamicTypeId>),
    Array {
        element: Box<CtDynamicTypeId>,
        length: u32,
    },
    Slice {
        element: Box<CtDynamicTypeId>,
    },
    Named(String),
    Record {
        name: String,
        fields: Vec<(String, CtDynamicTypeId)>,
    },
    Package {
        name: String,
        fields: Vec<(String, CtDynamicTypeId)>,
    },
    Enum {
        name: String,
        variants: Vec<(String, Vec<CtDynamicTypeId>)>,
    },
    ErrorUnion {
        error: Box<CtDynamicTypeId>,
        result: Box<CtDynamicTypeId>,
    },
    Address {
        mutable: bool,
        inner: Box<CtDynamicTypeId>,
    },
    Managed(Box<CtDynamicTypeId>),
    Desc(Box<CtDynamicTypeId>),
    FunctionPointer {
        parameters: Vec<CtDynamicTypeId>,
        result: Box<CtDynamicTypeId>,
    },
    BorrowedFunctionPointer {
        parameters: Vec<CtDynamicTypeId>,
        result: Box<CtDynamicTypeId>,
        life: StorageLifeContract,
    },
    Closure(ClosureId),
    Opaque {
        identity: NodeId,
        hidden: Box<CtDynamicTypeId>,
    },
    Unsupported(String),
}
