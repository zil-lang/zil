//! Private interfaces between independent lexical semantics and FullW.
//! These records are proof requests and scoped terms, never IR certificates.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FrameToken(pub(crate) u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScopeKey {
    Instance(SemanticInstanceId),
    Witness {
        definition: SemanticDefinitionId,
        frame: FrameToken,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SitePoint {
    pub(crate) function: FunctionId,
    pub(crate) block: BlockId,
    pub(crate) instruction: u32,
    pub(crate) after: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OperandKey {
    Parameter(u32),
    Capture(u32),
    Receiver,
    Value(ValueId),
    Local { local: LocalId, at: SitePoint },
    TypeArgument { scope: ScopeKey, ordinal: u32 },
    Environment { scope: ScopeKey, ordinal: u32 },
}
/// Identity is not apartness. Original where premises can relate distinct
/// terms; an actual caller must prove its obligations without adding such
/// premises merely to make the call type-check.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ClassKey {
    Formal {
        scope: ScopeKey,
        ordinal: u32,
    },
    Implicit {
        scope: ScopeKey,
        ordinal: u32,
    },
    Bound {
        frame: FrameToken,
        index: u32,
    },
    Invocation {
        frame: FrameToken,
        index: u32,
    },
    Outer {
        scope: ScopeKey,
        operand: OperandKey,
        path: Vec<LifeTypeProjection>,
        mode: StoredBorrowMode,
    },
    Intrinsic {
        scope: ScopeKey,
        operand: OperandKey,
        path: Vec<LifeTypeProjection>,
        class: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IntrinsicOrigin {
    pub(crate) scope: ScopeKey,
    pub(crate) operand: OperandKey,
    pub(crate) path: Vec<LifeTypeProjection>,
    pub(crate) class: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaticOccurrence {
    /// Type metadata, never a physical GEP or an owner capability.
    pub(crate) path: Vec<LifeTypeProjection>,
    pub(crate) leaf: Type,
    pub(crate) mode: StoredBorrowMode,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckedType {
    pub(crate) ty: Type,
    pub(crate) life: StorageLifeShape,
    pub(crate) classes: Vec<ClassKey>,
    /// Intrinsic domains survive many-to-one external images unchanged.
    pub(crate) origins: Vec<IntrinsicOrigin>,
    /// Pending actual-source obligations, not literal or Known0 evidence.
    pub(crate) statics: Vec<StaticOccurrence>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckedActual {
    Type(CheckedType),
    Const {
        carrier: CheckedType,
        value: Constant,
    },
    Life(ClassKey),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LexicalFrame {
    pub(crate) definition: SemanticDefinitionId,
    pub(crate) key: ScopeKey,
    pub(crate) actuals: Vec<CheckedActual>,
    pub(crate) environment: Vec<CheckedActual>,
    pub(crate) self_type: Option<CheckedType>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoundFrame {
    pub(crate) token: FrameToken,
    pub(crate) classes: Vec<ClassKey>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EvalEnv {
    /// Current exact lexical scope first, followed by its real parents.
    pub(crate) lexical: Vec<LexicalFrame>,
    /// Nearest higher-ranked binder first; never reset across a callback.
    pub(crate) bounds: Vec<BoundFrame>,
    pub(crate) invocations: Vec<ClassKey>,
    pub(crate) at: Option<SitePoint>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckedApplication {
    pub(crate) declaration: LifeDeclarationId,
    pub(crate) arguments: Vec<CheckedActual>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CheckedSignature {
    pub(crate) parameters: Vec<(CheckedType, PassingMode)>,
    pub(crate) result: CheckedType,
    pub(crate) result_mode: PassingMode,
    pub(crate) call_kind: CallKind,
    pub(crate) bound: Vec<ClassKey>,
    pub(crate) invocation: Vec<ClassKey>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RequirementMode {
    DeclarationPremise,
    ActualObligation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExternalRequest {
    Trait {
        receiver: CheckedType,
        application: CheckedApplication,
    },
    AssociatedType {
        receiver: CheckedType,
        application: CheckedApplication,
        member: u32,
    },
    AssociatedConst {
        receiver: CheckedType,
        application: CheckedApplication,
        member: u32,
    },
    Callable {
        receiver: CheckedType,
        kind: CallableKind,
        signature: CheckedSignature,
    },
    Structural {
        receiver: CheckedType,
        bound: DynStructuralBound,
    },
    Dyn {
        owned: bool,
        application: CheckedApplication,
        context: Option<CheckedType>,
        associated: Vec<(CheckedApplication, u32, CheckedActual)>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Relations {
    pub(crate) equalities: Vec<(ClassKey, ClassKey)>,
    pub(crate) outlives: Vec<(ClassKey, ClassKey)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExternalValue {
    Type(CheckedType),
    Const {
        carrier: CheckedType,
        value: Constant,
    },
    Proved(Relations),
}
/// A FullW callback receives the same context, environment, active dependency
/// session and remaining Budget. It cannot treat scope WF as a trait witness.
pub(crate) trait ExternalProofs {
    fn query(
        &mut self,
        module: &TypedModule,
        context: &super::CheckedContext,
        environment: &EvalEnv,
        request: &ExternalRequest,
        mode: RequirementMode,
        active: &mut super::ActiveQueries,
        budget: &mut Budget,
    ) -> Check<ExternalValue>;
}
