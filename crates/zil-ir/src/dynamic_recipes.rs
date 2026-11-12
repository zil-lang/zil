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

/// Full original ordinals and lexically scoped quantified binders never share
/// a numeric namespace. Bound depth zero names the nearest enclosing for-life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynLifePattern {
    DeclarationParameter(u32),
    MethodParameter(u32),
    Bound { depth: u32, index: u32 },
    Invocation(u32),
}

/// A dependency receiver can be a proper type argument, rather than root Self.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynObligation {
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
    StorageSources(StorageSourceBinding),
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
    pub result: Option<DynResultSource>,
    pub cleanup: Option<DynCleanupRecipe>,
    pub effects: Option<Vec<DynEffectRecipe>>,
    pub resume: Option<DynResumeBound>,
    pub threads: Option<DynThreadRequirements>,
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
    pub id: DynDefinitionId,
    pub identity: LifeDeclarationIdentity,
    pub member: u32,
    pub scope: DynDefinitionScope,
    pub signature: DynSignatureRecipe,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynMethodRecipe {
    pub member: u32,
    pub signature: DynSignatureRecipe,
    pub default_definition: Option<DynDefinitionId>,
}

/// Actual function instance links its original definition and full ordered
/// specialization to its independently checked actual header and CFG.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DynFunctionInstance {
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
    pub receiver: DynTypeValue,
    pub application: DynTraitApplication,
    pub implementation: LifeDeclarationId,
    pub actuals: Vec<DynGenericActual>,
    pub associated: Vec<DynAssociatedResolution>,
}
