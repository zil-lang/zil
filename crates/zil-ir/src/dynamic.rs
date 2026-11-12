//! Raw finite dynamic interfaces. These tables carry no verification authority.
use crate::*;

/// Request-local relocation only. Semantic identity resolves the complete table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynInterfaceId {
    pub module: ModuleId,
    pub index: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynVtableId {
    pub module: ModuleId,
    pub index: u32,
}

/// Shares the declaration-ID namespace and stable identity vocabulary with the
/// nominal life inventory. Keys and stable identities must be jointly unique.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynDeclaration {
    /// Impl Self recipe in original formal order. None is unknown for a
    /// generic borrowing impl; it never means an empty lifetime relation.
    pub self_pattern: Option<DynSelfPattern>,
    /// Complete implemented trait application in this impl's original formal
    /// scope. Every used provider must prove it; None remains unknown.
    pub implemented_trait: Option<DynSelfPattern>,
    pub id: LifeDeclarationId,
    pub identity: LifeDeclarationIdentity,
    pub kind: DynDeclarationKind,
    pub parameters: Vec<LifeParameterKind>,
    pub members: Vec<DynMemberDeclaration>,
}
/// Source-authenticated nominal header recipe. Raw recipes remain untrusted.
/// Every actual position survives, including phantom and equal life actuals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynSelfPattern {
    pub declaration: LifeDeclarationId,
    pub arguments: Vec<DynPatternActual>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynPatternActual {
    Type(DynTypePattern),
    Const(DynConstPattern),
    /// Full ordinal in DynDeclaration.parameters, never a Self class index.
    LifeParameter(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynConstPattern {
    Value(Constant),
    Parameter(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynTypePattern {
    /// Complete closed semantic type; Named placeholders are not executable.
    Concrete(Type),
    Parameter(u32),
    Nominal(Box<DynSelfPattern>),
    Address {
        mutable: bool,
        lifetime: Option<u32>,
        inner: Box<DynTypePattern>,
    },
    Tuple(Vec<DynTypePattern>),
    Array {
        element: Box<DynTypePattern>,
        length: DynConstPattern,
    },
    Slice(Box<DynTypePattern>),
    Managed(Box<DynTypePattern>),
    Desc(Box<DynTypePattern>),
    OwnedStore(Box<DynTypePattern>),
    ErrorUnion {
        error: Box<DynTypePattern>,
        result: Box<DynTypePattern>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynDeclarationKind {
    Trait,
    Impl,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynMemberDeclaration {
    pub name: String,
    pub kind: DynMemberKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynMemberKind {
    Type,
    Constant(Type),
    Method(Vec<LifeParameterKind>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynGenericActual {
    Type(Type),
    Const(Constant),
    Life(StorageBinder),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynTraitApplication {
    pub declaration: LifeDeclarationId,
    /// Full original generic order. Life actuals use this interface's Fixed domain.
    pub actuals: Vec<DynGenericActual>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynMember {
    pub application: DynTraitApplication,
    pub ordinal: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynAssociatedType {
    pub member: DynMember,
    pub ty: Type,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynAssociatedConstant {
    pub member: DynMember,
    pub ty: Type,
    /// Exact closed scalar bits in the first executable lane; no opaque hash.
    pub value: Constant,
}
/// Erased receiver at full signature position zero. The public callable type is
/// Address<Dyn(enclosing interface)>; the selected entry instead receives the
/// corresponding Address<concrete Self>. No self-reference cycle is serialized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynReceiver {
    pub mutable: bool,
    pub mode: PassingMode,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynSlot {
    pub member: DynMember,
    /// Full method binder order; Life actuals use THIS slot's Invocation domain.
    /// Every type/const application is concrete. There is no runtime quantifier.
    pub actuals: Vec<DynGenericActual>,
    pub receiver: DynReceiver,
    /// Nonreceiver parameters only; receiver remains full signature position0.
    pub parameters: Vec<Type>,
    pub parameter_modes: Vec<PassingMode>,
    pub result: Type,
    pub result_mode: PassingMode,
    pub call_kind: CallKind,
    pub storage_life: Option<StorageLifeContract>,
    pub contract: DynContract,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynInterface {
    /// Complete compile-time retained dependency envelope. None denotes the
    /// closed empty envelope; concrete nonempty contexts must match retained_life.
    pub context: Option<Type>,
    /// Whole erased-owner cleanup upper bound; None is unknown.
    pub owning_cleanup: Option<DynCleanupContract>,
    pub id: DynInterfaceId,
    pub application: DynTraitApplication,
    pub inherited: Vec<DynTraitApplication>,
    pub associated_types: Vec<DynAssociatedType>,
    pub associated_constants: Vec<DynAssociatedConstant>,
    pub retained_life: RetainedLifeShape,
    pub slots: Vec<DynSlot>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreDynEntry {
    pub slot: u32,
    pub module: ModuleId,
    pub function: NodeId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynEntry {
    pub slot: u32,
    pub function: FunctionId,
}
/// One explicit row for every public port. Empty sources prove that this
/// provider does not use that upper-bound port; an omitted row is unknown.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynRetainedPortBinding {
    pub port: u32,
    pub sources: Vec<StoragePosition>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreDynVtable {
    /// Complete persistent Self-class order mapped into the interface Context.
    /// Untrusted: includes phantom classes; only actual Fixed mappings qualify.
    pub self_classes: Vec<StorageBinder>,
    pub id: DynVtableId,
    pub interface: DynInterfaceId,
    pub self_type: Type,
    pub implementation: LifeDeclarationId,
    pub actuals: Vec<DynGenericActual>,
    pub ports: Vec<DynRetainedPortBinding>,
    pub entries: Vec<CoreDynEntry>,
    pub cleanup: Option<CoreDynEntry>,
    /// Explicit storage route for an owning erased payload. Ordinary cleanup
    /// entries are not owning destructor glue.
    pub owned_storage: Option<DynOwnedStorage>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynVtable {
    /// Complete persistent Self-class order mapped into the interface Context.
    pub self_classes: Vec<StorageBinder>,
    pub id: DynVtableId,
    pub interface: DynInterfaceId,
    pub self_type: Type,
    pub implementation: LifeDeclarationId,
    pub actuals: Vec<DynGenericActual>,
    pub ports: Vec<DynRetainedPortBinding>,
    pub entries: Vec<DynEntry>,
    pub cleanup: Option<DynEntry>,
    pub owned_storage: Option<DynOwnedStorage>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynOwnedStorage {
    Heap,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynRequirement {
    Read(StoragePosition),
    Write(StoragePosition),
    Manage(StoragePosition),
    Apart(StoragePosition, StoragePosition),
    State {
        position: StoragePosition,
        ty: Type,
        variant: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynGuarantee {
    /// Post-call retained source upper, not an exact assignment. Parameter(0)
    /// sources select the call-before snapshot. Complete rows preserve all
    /// parents; conservative old+argument uppers may prolong the old borrow.
    StorageSources(StorageSourceBinding),
    ManageResult,
    State {
        position: StoragePosition,
        ty: Type,
        variant: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynEffect {
    Alloc,
    Io,
    Block,
    Suspend,
    Spawn,
    Ffi,
    Reenter,
    Panic,
    Call(StoragePosition),
    Cleanup(Type),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynCallAccess {
    pub reads: Vec<StoragePosition>,
    pub writes: Vec<StoragePosition>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynResultSource {
    Value,
    Create,
    Give,
    Borrow {
        sources: Vec<StoragePosition>,
        mutable: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynCleanupContract {
    pub environment: bool,
    pub positions: Vec<StoragePosition>,
    pub access: DynCallAccess,
    pub effects: Vec<DynEffect>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynResumeBound {
    Local,
    Transfer,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynThreadRequirements {
    pub transfer: bool,
    pub share: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynContract {
    /// None means UNKNOWN, distinct from finite empty. Any construction/call
    /// needing an unknown component fails closed until independently derived.
    pub requires: Option<Vec<DynRequirement>>,
    pub guarantees: Option<Vec<DynGuarantee>>,
    pub hold: Option<Vec<StoragePosition>>,
    pub call: Option<DynCallAccess>,
    pub result: Option<DynResultSource>,
    pub cleanup: Option<DynCleanupContract>,
    pub effects: Option<Vec<DynEffect>>,
    pub resume: Option<DynResumeBound>,
    pub threads: Option<DynThreadRequirements>,
}
