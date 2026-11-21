//! Declaration-scoped dynamic recipes. These records are untrusted programs;
//! neither a declaration link nor a specialization supplies an execution proof.
use crate::*;

/// Complete type value in one local persistent domain. Classes map that whole
/// domain into the enclosing interface, or a method's explicit call domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynTypeValue {
    pub ty: Type,
    pub life: StorageLifeShape,
    pub classes: Vec<StorageBinder>,
}

/// Lexical instantiation domain. It may contain impl classes with no runtime
/// header occurrence; header binders are a separate, explicitly embedded scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynInstantiationScope {
    pub fixed_binder_count: u32,
    pub invocation_binder_count: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DynLifeEmbedding {
    pub from: StorageBinder,
    pub to: StorageBinder,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DynImplicitLifeKind {
    Address,
    Description,
    LifetimeArgument,
}
/// Original recipe roots, each selected by its actual inventory row index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DynImplicitLifeRoot {
    SelfPattern,
    ImplementedTrait,
    Supertrait(u32),
    Requirement(u32),
    AssociatedDefinition(u32),
    MethodSignature(u32),
    MethodDefinition(u32),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DynImplicitLifeOccurrence {
    pub root: DynImplicitLifeRoot,
    /// Canonical recipe fields/active variant fields in declared order; Vec
    /// selects an element, and Option/Box are transparent. Selects the exact
    /// DynLifePattern occurrence, never a physical storage address.
    pub path: Vec<u32>,
    pub kind: DynImplicitLifeKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynImplicitLifeDeclaration {
    /// Nonempty complete occurrence set. Its table ordinal is its namespace.
    pub occurrences: Vec<DynImplicitLifeOccurrence>,
}

/// Full original ordinals and lexically scoped quantified binders never share
/// a numeric namespace. Bound depth zero names the nearest enclosing for-life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynLifePattern {
    DeclarationParameter(u32),
    MethodParameter(u32),
    Bound {
        depth: u32,
        index: u32,
    },
    Invocation(u32),
    /// Independent declaration-local anonymous binder. Never an explicit
    /// generic formal ordinal and never guessed from a header class number.
    ImplicitDeclaration(u32),
}

/// A dependency receiver can be a proper type argument, rather than root Self.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynObligation {
    Structural {
        receiver: DynTypePattern,
        bound: DynStructuralBound,
    },
    Trait {
        receiver: DynTypePattern,
        application: DynSelfPattern,
    },
    Callable {
        receiver: DynTypePattern,
        kind: CallableKind,
        quantified_lifetimes: u32,
        signature: Box<DynSignatureRecipe>,
    },
    /// Complete associated bindings and original where/super equations.
    /// This is equality of full types/domains, never physical layout equality.
    TypeEqual {
        left: DynTypePattern,
        right: DynTypePattern,
    },
    /// Both operands are interpreted in this exact substituted carrier.
    ConstEqual {
        ty: DynTypePattern,
        left: DynConstPattern,
        right: DynConstPattern,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynParameterRecipe {
    pub ty: DynTypePattern,
    pub mode: PassingMode,
}

/// Evaluation of complete type patterns deterministically produces every
/// outer, nested and phantom class occurrence. Unknown Type argument domains
/// are substituted whole, never represented by a guessed constant row count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynSignatureRecipe {
    pub parameters: Vec<DynParameterRecipe>,
    pub result: DynTypePattern,
    pub result_mode: PassingMode,
    pub call_kind: CallKind,
    pub invocation_binder_count: u32,
    pub contract: DynContractRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynRequirementRecipe {
    Read(StoragePosition),
    Write(StoragePosition),
    Manage(StoragePosition),
    Apart(StoragePosition, StoragePosition),
    State {
        position: StoragePosition,
        ty: DynTypePattern,
        variant: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynGuaranteeRecipe {
    StorageSources(DynStorageSourceBindingRecipe),
    ManageResult,
    State {
        position: StoragePosition,
        ty: DynTypePattern,
        variant: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynEffectRecipe {
    Alloc,
    Io,
    Block,
    Suspend,
    Spawn,
    Ffi,
    Reenter,
    Panic,
    Call(StoragePosition),
    Cleanup(DynTypePattern),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynCleanupRecipe {
    pub environment: bool,
    pub positions: Vec<StoragePosition>,
    pub access: DynCallAccess,
    pub effects: Vec<DynEffectRecipe>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynContractRecipe {
    pub requires: Option<Vec<DynRequirementRecipe>>,
    pub guarantees: Option<Vec<DynGuaranteeRecipe>>,
    pub hold: Option<Vec<StoragePosition>>,
    pub call: Option<DynCallAccess>,
    pub result: Option<DynResultSourceRecipe>,
    pub cleanup: Option<DynCleanupRecipe>,
    pub effects: Option<Vec<DynEffectRecipe>>,
    pub resume: Option<DynResumeBound>,
    pub threads: Option<DynThreadRequirements>,
}

/// Original declaration source expressions. Only independent instantiation of
/// the complete leaf recipe may produce a concrete runtime StorageSource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynStorageSourceRecipe {
    Exact(StoragePosition),
    PlaceAddress {
        place: StoragePosition,
        mode: StoredBorrowMode,
    },
    OwnedView {
        head: StoragePosition,
        leaf: DynTypePattern,
        mode: StoredBorrowMode,
    },
    ReborrowedLeaf {
        source: StoragePosition,
        head: StoragePosition,
    },
    Static {
        leaf: DynTypePattern,
        mode: StoredBorrowMode,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynStorageSourceBindingRecipe {
    pub position: StoragePosition,
    pub sources: Vec<DynStorageSourceRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynResultSourceRecipe {
    Value,
    Create,
    Give,
    Borrow {
        sources: Vec<DynStorageSourceRecipe>,
        mutable: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynDefinitionId {
    pub module: ModuleId,
    pub source: NodeId,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynDefinitionScope {
    TraitDefault,
    ImplOverride,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynDefinitionDeclaration {
    pub requirements: Vec<DynObligation>,
    /// Exact instantiation targets in this original definition's scope. A
    /// function cannot authenticate membership by asserting an origin tag.
    pub instances: Vec<DynDefinitionSpecialization>,
    pub id: DynDefinitionId,
    pub identity: LifeDeclarationIdentity,
    pub member: u32,
    pub scope: DynDefinitionScope,
    pub signature: DynSignatureRecipe,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynMethodRecipe {
    pub requirements: Vec<DynObligation>,
    pub member: u32,
    pub signature: DynSignatureRecipe,
    pub default_definition: Option<DynDefinitionId>,
}

/// Actual function instance links its original definition and full ordered
/// specialization to its independently checked actual header and CFG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynFunctionInstance {
    pub scope: DynInstantiationScope,
    /// Every actual header binder maps once into the lexical domain. Lexical
    /// classes need not occur in the header, but remain in full specialization.
    pub header_embedding: Vec<DynLifeEmbedding>,
    pub definition: DynDefinitionId,
    pub receiver: DynTypeValue,
    pub declaration_actuals: Vec<DynGenericActual>,
    pub method_actuals: Vec<DynGenericActual>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynAssociatedValueRecipe {
    Type(DynTypePattern),
    Constant {
        ty: DynTypePattern,
        value: DynConstPattern,
    },
}
/// On a trait, an absent value is an abstract member with explicit bounds. On
/// an impl, every row has a complete definition in that impl's original scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynAssociatedDefinition {
    /// Required for a constant, even when it has no default RHS. Type members
    /// have no carrier here; their complete value comes from their definition.
    pub carrier: Option<DynTypePattern>,
    pub member: u32,
    pub value: Option<DynAssociatedValueRecipe>,
    pub requirements: Vec<DynObligation>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynAssociatedSource {
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
pub enum DynAssociatedValue {
    Type(DynTypeValue),
    Constant { ty: DynTypeValue, value: Constant },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynAssociatedResolution {
    pub member: DynMember,
    pub source: DynAssociatedSource,
    pub value: DynAssociatedValue,
}

/// A node in the finite obligation graph. Its exact key is the complete actual
/// receiver and trait application, including every class and phantom actual.
/// Only nodes reachable from the root impl, supers, bounds or projections may
/// occur. Each reachable node has one legal implementation and full results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynImplementationWitness {
    pub scope: DynInstantiationScope,
    /// Complete lexical-domain map to the actual enclosing proof context.
    /// Local Self/trait/associated identities are checked before this mapping.
    pub to_context: Vec<DynLifeEmbedding>,
    pub receiver: DynTypeValue,
    pub application: DynTraitApplication,
    pub implementation: LifeDeclarationId,
    pub actuals: Vec<DynGenericActual>,
    pub associated: Vec<DynAssociatedResolution>,
}

/// Core targets are relocated through the actual signature inventory. Typed
/// records must name the exact function and never retain a Core relocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynFunctionTarget {
    Core { module: ModuleId, source: NodeId },
    Typed(FunctionId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynDefinitionSpecialization {
    pub function: DynFunctionTarget,
    pub receiver: DynTypeValue,
    pub declaration_actuals: Vec<DynGenericActual>,
    pub method_actuals: Vec<DynGenericActual>,
}

/// Sealed language predicates in bound position, never user trait names. Their
/// actual complete type and cleanup qualification is independently checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynStructuralBound {
    Copy,
    Value,
    SyncDrop,
    StoreElement,
}
