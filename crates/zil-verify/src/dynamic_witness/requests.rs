//! Private obligations, never serialized authority. Static interpretation may
//! construct these records only from a checked original specialization; the
//! ordinary CFG kernel must independently discharge each request before release.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    TableSlot {
        table: DynVtableId,
        slot: u32,
        member: DynMember,
        definition: DynDefinitionId,
    },
    Definition {
        owner: LifeDeclarationId,
        member: u32,
        definition: DynDefinitionId,
    },
    Callable {
        closure: ClosureId,
    },
}
/// Scope-local semantic identity only. These symbols correlate complete
/// occurrences; they never stand for a target, permission or physical address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IntrinsicScope {
    Function(FunctionId),
    Witness { table: DynVtableId, node: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IntrinsicRoot {
    Receiver,
    DeclarationType(u32),
    MethodType(u32),
    ApplicationType(u32),
    Lexical(StorageBinder),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Intrinsic {
    pub scope: IntrinsicScope,
    pub root: IntrinsicRoot,
    pub class: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Occurrence {
    Borrow {
        position: StoragePosition,
        binder: StorageBinder,
        origin: Intrinsic,
    },
    Class {
        position: SignatureLifeClass,
        binder: StorageBinder,
        origin: Intrinsic,
    },
}
/// A program-static borrow contributes an occurrence and pending provenance
/// proof, but never a variable class or a fabricated Fixed binder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Borrow {
    Variable {
        binder: StorageBinder,
        origin: Intrinsic,
    },
    Static,
}
/// Physical inline fields, logical retained ports and an owning head are
/// different selectors. In particular, a retained port is never a GEP.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeafKind {
    Physical,
    RetainedPort,
    OwningHead,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LeafClass {
    pub path: Vec<LifeTypeProjection>,
    pub class: u32,
    pub binder: StorageBinder,
    pub origin: Intrinsic,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Leaf {
    pub source: StoragePosition,
    pub kind: LeafKind,
    pub ty: Type,
    pub mode: StoredBorrowMode,
    pub outer: Borrow,
    /// Complete inner occurrence map, including phantom classes. A phantom
    /// class never invents a target, loan, byte access or physical field.
    pub classes: Vec<LeafClass>,
}
/// An embedded type operand retains its whole scoped value in addition to
/// DynContract's machine Type. Equal layouts or outside class images do not
/// authorize a different State or Cleanup qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperandPosition {
    RequirementState(u32),
    GuaranteeState(u32),
    EffectCleanup(u32),
    CleanupEffectCleanup(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Operand {
    pub position: OperandPosition,
    pub value: DynTypeValue,
}
/// Expected occurrences use the request's complete lexical domain. Unlike an
/// actual runtime header this may mention a lexical class with no input header
/// occurrence (for example an unused original lifetime argument). It must not
/// be presented as a canonical Typed StorageLifeContract.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Header {
    pub bindings: Vec<StorageLifeBinding>,
    pub static_bindings: Vec<StoragePosition>,
    pub classes: Vec<SignatureLifeClassBinding>,
    pub origins: Vec<Occurrence>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContractRequest {
    pub origin: Origin,
    pub function: FunctionId,
    /// Exact complete specialization; no asserted origin tag alone suffices.
    pub specialization: Option<DynFunctionInstance>,
    /// Context Fixed plus this slot's Invocation domain for TableSlot; the
    /// actual instance lexical domain for Definition. Callable has a separate
    /// hidden Receiver, including FnOnce's consuming environment responsibility.
    pub scope: DynInstantiationScope,
    pub parameters: Vec<DynTypeValue>,
    pub parameter_modes: Vec<PassingMode>,
    pub result: DynTypeValue,
    pub result_mode: PassingMode,
    pub call_kind: CallKind,
    pub contract: DynContract,
    pub operands: Vec<Operand>,
    pub expected_header: Header,
    /// Complete actual header, including constant borrow occurrences. None
    /// remains Unknown unless the independent type inventory proves Known0.
    pub actual_header: Option<StorageLifeContract>,
    /// Actual runtime header binder -> request domain. Constant positions do
    /// not appear in this map and never create a variable binder.
    pub header_embedding: Vec<DynLifeEmbedding>,
    pub self_leaves: Vec<Leaf>,
}
