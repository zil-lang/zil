//! Complete untrusted CT inventory for dynamic metadata and storage sources.
//! Type payloads are edges in the finite metadata Type DAG in this catalogue.
//! Catalogue membership grants no visibility, trait, provider or borrow right.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CtDynamicTypeId(pub u32);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CtDynamicCatalogue {
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
    StorageSources(CtStorageSourceBinding),
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
    pub result: Option<CtDynResultSource>,
    pub cleanup: Option<CtDynCleanupRecipe>,
    pub effects: Option<Vec<CtDynEffectRecipe>>,
    pub resume: Option<CtDynResumeBound>,
    pub threads: Option<CtDynThreadRequirements>,
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
