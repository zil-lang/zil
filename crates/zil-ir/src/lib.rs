#![forbid(unsafe_code)]

//! Public, untrusted compiler schemas. Constructing these records never grants
//! safety authority. Only `zil-verify` can produce an immutable verified wrapper.

use zil_base::{
    Fingerprint, FingerprintBuilder, ModuleId, NodeId, SCHEMA_VERSION, Span, TargetProfile,
};

mod program;
pub mod wire;
pub use program::*;

/// Reserved raw identity for the sealed byte storage resource. Not authority.
pub const BYTE_BUFFER_RESOURCE_NAME: &str = "@builtin:ByteBuffer";

macro_rules! identity {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);
    )+};
}
identity!(
    FunctionId, LocalId, ValueId, BlockId, PlaceId, ClosureId, OriginId, ScopeId
);

/// Scalar B0 checking is deliberately a subset of this raw schema. All other
/// variants must be diagnosed as Unsupported until their checker is implemented.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    Unit,
    Bool,
    I32,
    I64,
    U8,
    U16,
    /// Sealed dynamic byte-storage payload, legal only in Managed/Desc and its
    /// exact resource definition. Not a Copy value or a forgeable raw pointer.
    ByteBuffer,
    /// Sealed dynamic storage payload. Only an exact Managed/Desc wrapper and
    /// independently checked element/resource definitions may establish a head.
    OwnedStore {
        element: Box<Type>,
    },
    /// An opaque external token, never a ZIL address or an ownership proof.
    ForeignHandle,
    /// Readonly static UTF-16 plus NUL, or null. No foreign return admission.
    CWideString,
    /// Static C ABI code only; deliberately distinct from FunctionPointer.
    ForeignCallback {
        parameters: Vec<Type>,
        result: Box<Type>,
    },
    Tuple(Vec<Type>),
    Array {
        element: Box<Type>,
        length: u32,
    },
    /// Unsized pointee only. A legal borrowed slice is Address<Slice<T>>;
    /// this raw shape never grants a lifetime or an access permission.
    Slice {
        element: Box<Type>,
    },
    Named(String),
    /// A resolved, nominal, finite Copy record. Name includes its module identity.
    Record {
        name: String,
        fields: Vec<(String, Type)>,
    },
    /// A nominal, finite, non-Copy value. SSA/ABI values are always complete;
    /// local storage may temporarily hold a checked subset of owning fields.
    /// At least one field is Managed; the remainder follow the independently
    /// checked finite Copy / complete owning-array field contract.
    Package {
        name: String,
        fields: Vec<(String, Type)>,
    },
    /// Nominal, finite tagged union. Variant numbers follow declaration order.
    Enum {
        name: String,
        variants: Vec<(String, Vec<Type>)>,
    },
    /// Synchronous tagged success/error value; its two alternatives stay distinct.
    ErrorUnion {
        error: Box<Type>,
        result: Box<Type>,
    },
    Address {
        mutable: bool,
        inner: Box<Type>,
    },
    Managed(Box<Type>),
    Desc(Box<Type>),
    FunctionPointer {
        parameters: Vec<Type>,
        result: Box<Type>,
    },
    Closure(ClosureId),
    Opaque {
        identity: NodeId,
        hidden: Box<Type>,
    },
    Unsupported(String),
}
impl Type {
    pub fn is_b0_scalar(&self) -> bool {
        matches!(
            self,
            Self::Unit | Self::Bool | Self::I32 | Self::I64 | Self::U8 | Self::U16
        )
    }
    /// Scalars or one shared address layer to a finite Copy value or the exact
    /// shared user-resource head shape. This does not establish loan authority.
    pub fn is_scalar_or_shared_address(&self) -> bool {
        self.is_b0_scalar()
            || self.is_shared_resource_head_address()
            || self.is_shared_package_address()
            || matches!(self, Self::Address { mutable: false, inner } if inner.is_address_pointee())
    }
    /// One address layer; admission is not a permission proof.
    pub fn is_scalar_or_address(&self) -> bool {
        self.is_b0_scalar()
            || self.is_shared_resource_head_address()
            || self.is_package_address()
            || matches!(self, Self::Address { inner, .. } if inner.is_address_pointee())
    }
    /// Structural eligibility only; each checker separately proves sources,
    /// bounds, liveness, permissions and the supported target profile.
    pub fn is_address_pointee(&self) -> bool {
        self.is_copy_value() || matches!(self, Self::Slice { element } if element.is_copy_value())
    }
    /// Structural eligibility for a shared actual user-resource header. This
    /// does not establish its nominal definition, initialization, provenance or
    /// permission. General pointee admission intentionally remains unchanged.
    pub fn is_shared_resource_head_address(&self) -> bool {
        matches!(self, Self::Address { mutable: false, inner }
            if matches!(inner.as_ref(), Self::Managed(payload)
                if matches!(payload.as_ref(), Self::Record { .. })
                    && payload.is_copy_value()))
    }
    /// Shape only: a Store element window requires separate exact element,
    /// provenance, initialization and loan checks. General pointees stay Copy.
    pub fn is_shared_package_address(&self) -> bool {
        matches!(self, Self::Address { mutable: false, inner }
            if matches!(inner.as_ref(), Self::Package { .. }))
    }
    /// Raw shape only. A mutable Package window still needs independent exact
    /// Store-element, origin, initialized-prefix and exclusive-loan checks.
    /// This does not make Package a general address pointee or a Copy value.
    pub fn is_package_address(&self) -> bool {
        matches!(self, Self::Address { inner, .. }
            if matches!(inner.as_ref(), Self::Package { .. }))
    }
    pub fn is_slice_address(&self) -> bool {
        matches!(self, Self::Address { inner, .. } if matches!(inner.as_ref(), Self::Slice { .. }))
    }
    pub fn is_copy_value(&self) -> bool {
        match self {
            Self::Unit
            | Self::Bool
            | Self::I32
            | Self::I64
            | Self::U8
            | Self::U16
            | Self::ForeignHandle
            | Self::CWideString
            | Self::ForeignCallback { .. } => true,
            Self::Tuple(fields) => fields.iter().all(Self::is_copy_value),
            Self::Array { element, .. } => element.is_copy_value(),
            Self::Record { fields, .. } => fields.iter().all(|(_, ty)| ty.is_copy_value()),
            Self::Enum { variants, .. } => variants
                .iter()
                .all(|(_, fields)| fields.iter().all(Self::is_copy_value)),
            Self::ErrorUnion { error, result } => error.is_copy_value() && result.is_copy_value(),
            Self::FunctionPointer { parameters, result } => {
                parameters.iter().all(Self::is_copy_value) && result.is_copy_value()
            }
            _ => false,
        }
    }
    pub fn aggregate_fields(&self) -> Option<Vec<&Type>> {
        match self {
            Self::Tuple(fields) => Some(fields.iter().collect()),
            Self::Array { element, length } => Some(vec![element.as_ref(); *length as usize]),
            Self::Record { fields, .. } => Some(fields.iter().map(|(_, ty)| ty).collect()),
            _ => None,
        }
    }
    pub fn is_integer(&self) -> bool {
        matches!(self, Self::I32 | Self::I64 | Self::U8 | Self::U16)
    }
    pub fn name(&self) -> String {
        match self {
            Self::Unit => "()".into(),
            Self::Bool => "bool".into(),
            Self::I32 => "i32".into(),
            Self::I64 => "i64".into(),
            Self::U8 => "u8".into(),
            Self::U16 => "u16".into(),
            Self::ByteBuffer => "ByteBuffer".into(),
            Self::OwnedStore { element } => format!("Store<{}>", element.name()),
            Self::ForeignHandle => "c_handle".into(),
            Self::CWideString => "c_wstr".into(),
            other => format!("{other:?}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnaryOp {
    Negate,
    Not,
    BitNot,
    Dereference,
    Borrow,
    BorrowMut,
    Give,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    AddWrapping,
    SubtractWrapping,
    MultiplyWrapping,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRight,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Constant {
    Unit,
    Bool(bool),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    NullHandle,
    NullWideString,
    /// Includes exactly one trailing NUL. Untrusted until independently checked.
    WideString(Vec<u16>),
}
impl Constant {
    pub const fn ty(&self) -> Type {
        match self {
            Self::Unit => Type::Unit,
            Self::Bool(_) => Type::Bool,
            Self::I32(_) => Type::I32,
            Self::I64(_) => Type::I64,
            Self::U8(_) => Type::U8,
            Self::U16(_) => Type::U16,
            Self::NullHandle => Type::ForeignHandle,
            Self::NullWideString | Self::WideString(_) => Type::CWideString,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedFeature {
    pub feature: String,
    pub span: Option<Span>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreModule {
    pub schema_version: u32,
    pub module_id: ModuleId,
    pub source_fingerprint: Fingerprint,
    pub functions: Vec<CoreFunction>,
    pub aggregates: Vec<CoreAggregate>,
    /// Untrusted finite Product recipes; empty on pre-237 source paths.
    pub aggregate_templates: Vec<CoreAggregateTemplate>,
    /// Exact nominal applications, never ownership certificates.
    pub aggregate_applications: Vec<CoreAggregateApplication>,
    pub enums: Vec<CoreEnum>,
    pub unsupported: Vec<UnsupportedFeature>,
}
/// A compiler-reserved source bound. This raw declaration grants no permission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoreTypeBound {
    StoreElement,
}

/// Lexically scoped ordinary type binder, distinct from actual callable roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreTypeParameter {
    pub index: u32,
    pub name: String,
    pub bound: CoreTypeBound,
}

/// A finite Package family's original declaration and field recipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreAggregateTemplate {
    pub id: NodeId,
    pub name: String,
    pub parameter: CoreTypeParameter,
    pub fields: Vec<CoreAggregateFieldRecipe>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreAggregateFieldRecipe {
    pub name: String,
    pub public: bool,
    pub manages: bool,
    pub ty: CoreAggregateTypeRecipe,
    pub span: Span,
}
/// Recipe Parameter refers only to this template's formal declaration slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreAggregateTypeRecipe {
    Concrete(Type),
    Parameter {
        index: u32,
    },
    Store {
        element: Box<CoreAggregateTypeRecipe>,
    },
    Apply {
        module: ModuleId,
        template: NodeId,
        arguments: Vec<CoreAggregateTypeRecipe>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreAggregateApplication {
    pub aggregate: NodeId,
    pub template_module: ModuleId,
    pub template: NodeId,
    pub owner: Option<CoreAggregateOwner>,
    pub arguments: Vec<CoreAggregateArgument>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreAggregateOwner {
    pub module: ModuleId,
    pub function: NodeId,
}
/// Application Parameter refers only to the actual owner's function binder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreAggregateArgument {
    Concrete(Type),
    Parameter { index: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreFunction {
    /// Retained ordinary template metadata; only a checked full instance may run.
    pub type_parameters: Vec<CoreTypeParameter>,
    /// Independent declared bindings within one owning State parameter.
    /// This raw table grants neither call authority nor ownership.
    pub state_parameters: Vec<CoreStateParameter>,
    /// Source-declared opaque callable result; still untrusted and unresolved.
    pub callable_return: Option<CoreCallableReturn>,
    pub id: NodeId,
    pub name: String,
    /// Retained callable template metadata; absent on ordinary concrete functions.
    pub callable_parameters: Vec<CoreCallableParameter>,
    pub foreign: Option<ForeignDeclaration>,
    pub parameters: Vec<CoreParameter>,
    pub return_type: Type,
    pub result_mode: PassingMode,
    pub body: CoreBlock,
    pub receiver: Option<ReceiverShape>,
    pub span: Span,
}
/// Source-declared bound for a finite, self-contained callable factory result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCallableReturn {
    pub kind: CallableKind,
    pub parameters: Vec<Type>,
    pub result: Type,
}

/// Raw bound for one retained, nonescaping callable template parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCallableParameter {
    pub index: u32,
    /// Position among callable generic declarations, independent of runtime role order.
    pub generic_index: u32,
    pub generic_name: String,
    pub kind: CallableKind,
    pub parameters: Vec<Type>,
    pub result: Type,
}

/// One real owning State parameter, evaluated and transferred only once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreStateParameter {
    pub index: u32,
    pub bindings: Vec<CoreStateBinding>,
}

/// One lexical callable declaration and its actual direct symbolic fields.
/// Declaration order is independent of Package field order and ClosureId.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreStateBinding {
    pub generic_index: u32,
    pub generic_name: String,
    pub kind: CallableKind,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub fields: Vec<u32>,
}

/// An untrusted external ABI/symbol declaration, never proof of its C implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignDeclaration {
    pub abi: String,
    pub symbol: String,
    /// Sorted exact indices of synchronous, non-retained borrowed parameters.
    /// An external promise only: independent checking still proves each loan.
    pub noescape_parameters: Vec<u32>,
    /// Explicit external promise allowing only the finite static callback profile.
    pub static_callbacks: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreParameter {
    pub name: String,
    pub ty: Type,
    pub mode: PassingMode,
    pub span: Span,
}
/// A source closure parameter may await a unique type constraint. An absent
/// annotation is not an unsupported type and must not reach final typed IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreClosureParameter {
    pub name: String,
    pub ty: Option<Type>,
    pub mode: PassingMode,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreAggregate {
    pub id: NodeId,
    pub name: String,
    pub kind: AggregateKind,
    /// Explicit builtin Drop implementation symbol; not an inferred method name.
    pub teardown: Option<String>,
    pub fields: Vec<CoreField>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreEnum {
    pub id: NodeId,
    pub name: String,
    pub variants: Vec<(String, Vec<Type>)>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CorePattern {
    pub kind: CorePatternKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CorePatternKind {
    Name(String),
    Wildcard,
    Tuple(Vec<CorePattern>),
    Variant {
        ty: Type,
        variant: String,
        fields: Vec<CorePattern>,
    },
    Integer(String),
    Bool(bool),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreMatchArm {
    pub pattern: CorePattern,
    pub body: CoreBlock,
    pub span: Span,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateKind {
    Struct,
    Package,
    Resource,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreField {
    pub name: String,
    pub ty: Type,
    /// Source visibility; declaring module is the containing ProgramModule.
    pub public: bool,
    pub manages: bool,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreBlock {
    pub statements: Vec<CoreStatement>,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreStatement {
    pub id: NodeId,
    pub kind: CoreStatementKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreStatementKind {
    Binding {
        name: String,
        mutable: bool,
        ty: Option<Type>,
        initializer: Option<CoreExpr>,
    },
    Assign {
        target: CoreExpr,
        value: CoreExpr,
    },
    Expression(CoreExpr),
    Return(Option<CoreExpr>),
    If {
        condition: CoreExpr,
        then_body: CoreBlock,
        else_body: Option<CoreBlock>,
    },
    IfLet {
        pattern: CorePattern,
        value: CoreExpr,
        then_body: CoreBlock,
        else_body: Option<CoreBlock>,
    },
    While {
        condition: CoreExpr,
        body: CoreBlock,
    },
    WhileLet {
        pattern: CorePattern,
        value: CoreExpr,
        body: CoreBlock,
    },
    ForRange {
        name: String,
        start: CoreExpr,
        end: CoreExpr,
        body: CoreBlock,
    },
    Match {
        value: CoreExpr,
        arms: Vec<CoreMatchArm>,
    },
    Block(CoreBlock),
    /// Register an ordinary synchronous cleanup body at this declaration point.
    Defer(CoreBlock),
    Unsafe(CoreBlock),
    Break,
    Continue,
    Unsupported(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreExpr {
    pub id: NodeId,
    pub kind: CoreExprKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreExprKind {
    /// Compiler-normalized static foreign literal; still untrusted Core data.
    Literal(Constant),
    /// Internal value type context; ordinary checking must validate its child.
    Ascribed {
        ty: Type,
        value: Box<CoreExpr>,
    },
    Integer(String),
    IntegerConvert {
        value: Box<CoreExpr>,
        target: Type,
    },
    Bool(bool),
    String(String),
    Unit,
    Name(String),
    Unary {
        op: UnaryOp,
        operand: Box<CoreExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<CoreExpr>,
        right: Box<CoreExpr>,
    },
    /// Untrusted complete type constraints, legal only at a static Call callee.
    TypeApply {
        callee: Box<CoreExpr>,
        type_arguments: Vec<Type>,
    },
    Call {
        callee: Box<CoreExpr>,
        arguments: Vec<CoreExpr>,
    },
    Tuple(Vec<CoreExpr>),
    Array(Vec<CoreExpr>),
    Index {
        object: Box<CoreExpr>,
        index: Box<CoreExpr>,
    },
    /// A source range place. Only a borrow may turn it into a slice address.
    Slice {
        object: Box<CoreExpr>,
        start: Option<Box<CoreExpr>>,
        end: Option<Box<CoreExpr>>,
    },
    Member {
        object: Box<CoreExpr>,
        member: String,
    },
    ReceiverMember(String),
    Record {
        ty: Type,
        fields: Vec<(String, CoreExpr)>,
    },
    RecordUpdate {
        ty: Option<Type>,
        base: Box<CoreExpr>,
        fields: Vec<(String, CoreExpr)>,
    },
    /// Ordered hygienic temporaries. Names use `$`, which source identifiers cannot contain.
    Sequence {
        bindings: Vec<(String, CoreExpr)>,
        value: Box<CoreExpr>,
    },
    /// Unevaluated source CT expression; it must enter VerifiedCTIR before use.
    CompileTime(Box<CoreExpr>),
    /// Single evaluation with propagation along the actual error edge.
    Try(Box<CoreExpr>),
    Closure {
        move_capture: bool,
        parameters: Vec<CoreClosureParameter>,
        return_type: Option<Type>,
        body: CoreBlock,
    },
    If {
        condition: Box<CoreExpr>,
        then_value: Box<CoreExpr>,
        else_value: Box<CoreExpr>,
    },
    Unsupported(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValueCategory {
    Value,
    Managed,
    Desc,
    Address,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PassingMode {
    Copy,
    Move,
    Describe,
    Create,
    Give,
    Borrow,
    BorrowMut,
    Infer,
}
fn passing_mode_tag(mode: PassingMode) -> u8 {
    match mode {
        PassingMode::Copy => 0,
        PassingMode::Move => 1,
        PassingMode::Describe => 2,
        PassingMode::Create => 3,
        PassingMode::Give => 4,
        PassingMode::Borrow => 5,
        PassingMode::BorrowMut => 6,
        PassingMode::Infer => 7,
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceiverShape {
    pub ty: Type,
    pub mode: PassingMode,
    pub implicit: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableKind {
    FunctionPointer,
    Fn,
    FnMut,
    FnOnce,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableShape {
    pub kind: CallableKind,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub environment: Option<ClosureId>,
    pub captures: Vec<CaptureField>,
    pub contracts: CallableContracts,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureField {
    pub name: String,
    pub ty: Type,
    pub category: ValueCategory,
    pub mode: PassingMode,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallableContracts {
    pub hold: Vec<String>,
    pub call: Vec<String>,
    pub cleanup: Vec<String>,
    pub resume: Vec<String>,
}

/// Public helper claim. It is checked from the callee CFG before use;
/// neither construction nor transport confers authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableParameterContract {
    /// Sorted unique actual parameter roles; runtime identity is never inferred from type identity.
    pub roles: Vec<CallableRole>,
    /// One joint ordered call language preserves correlation across every role.
    pub traces: CallableTraces,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableRole {
    pub parameter: u32,
    pub closure: ClosureId,
    /// Requested bound mode; the concrete closure may support an adaptation.
    pub kind: CallableKind,
    /// Captured Address indices whose targets must stay alive while held.
    pub hold: Vec<u32>,
    /// Captured Desc/Address indices potentially used during each callback call.
    pub call: Vec<u32>,
    /// Conservative target-write upper bound: Desc and mutable Address captures.
    pub writes: Vec<u32>,
    pub cleanup: CallableCleanup,
}
/// A raw opaque callable result claim. The first implemented form is a
/// self-contained concrete environment with no external Hold/Call dependencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableReturnContract {
    /// Untrusted result provenance; each reachable return must prove this relation.
    pub source: CallableReturnSource,
    pub closure: ClosureId,
    pub kind: CallableKind,
    pub hold: Vec<u32>,
    pub call: Vec<u32>,
    pub writes: Vec<u32>,
    /// Direct Managed capture indices, in exact environment field order.
    pub cleanup: Vec<u32>,
    /// Sorted unique actual environment captures lending the shared result.
    /// Raw provenance claim only; independent CFG and loan checks are required.
    pub lending_captures: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableReturnSource {
    /// A newly constructed self-contained environment under the factory profile.
    Fresh,
    /// The same live instance supplied in this actual function parameter position.
    Parameter(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableCleanup {
    /// The by-value helper settles its environment on every returning path.
    Owned,
    /// The helper preserves the borrowed carrier and acquires no cleanup debt.
    Borrowed,
    /// On normal return, transfer the actual remaining environment to the result.
    Returned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallableExit {
    /// Ordinary Return, including either tag of a Copy ErrorUnion.
    Return,
    /// Process abort; no unwind cleanup guarantee is implied.
    Trap,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallEvent {
    pub parameter: u32,
    pub kind: CallableKind,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallableTrace {
    pub calls: Vec<CallEvent>,
    pub exit: CallableExit,
}
/// Exact callback behavior, transported as an untrusted claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallableTraces {
    Finite(Vec<CallableTrace>),
    Flow(CallableFlow),
}
impl CallableTraces {
    /// Explicit projection: a cyclic graph is never an empty finite trace set.
    pub fn as_finite(&self) -> Option<&[CallableTrace]> {
        match self {
            Self::Finite(traces) => Some(traces),
            Self::Flow(_) => None,
        }
    }
    pub fn as_finite_mut(&mut self) -> Option<&mut Vec<CallableTrace>> {
        match self {
            Self::Finite(traces) => Some(traces),
            Self::Flow(_) => None,
        }
    }
}
/// Flat graph. Canonical semantic claims use entry zero and ordered BFS IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableFlow {
    pub entry: u32,
    pub nodes: Vec<CallableFlowNode>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableFlowNode {
    pub calls: Vec<CallEvent>,
    pub next: CallableFlowNext,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallableFlowNext {
    Edges(Vec<u32>),
    Exit(CallableExit),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedModule {
    pub schema_version: u32,
    pub module_id: ModuleId,
    pub source_fingerprint: Fingerprint,
    pub target: TargetProfile,
    pub functions: Vec<TypedFunction>,
    pub closures: Vec<ClosureDefinition>,
    pub resources: Vec<ResourceDefinition>,
    pub unsupported: Vec<UnsupportedFeature>,
}
/// Raw nominal cleanup binding. Every payload and entry is independently checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceDefinition {
    pub name: String,
    pub payload: Type,
    pub teardown: Option<FunctionId>,
}
/// Raw concrete environment claims. Supported captures are checked independently;
/// this record does not grant address lifetime or calling authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureDefinition {
    pub id: ClosureId,
    pub function: FunctionId,
    pub captures: Vec<Type>,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub kind: CallableKind,
    /// Sorted unique capture positions that own storage borrowed by the result.
    /// Distinct from external captured Address/Desc targets and ordinary parameters.
    pub lending_captures: Vec<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypedFunction {
    /// Raw opaque callable result contract, independently checked by Verify.
    pub callable_return_contract: Option<CallableReturnContract>,
    /// Raw finite helper relation, independently reconstructed by Verify.
    pub callable_contract: Option<CallableParameterContract>,
    pub id: FunctionId,
    pub name: String,
    /// A foreign declaration has parameter locals but no executable CFG.
    pub foreign: Option<ForeignDeclaration>,
    /// A closure entry needs its matching hidden environment value. It is not
    /// callable through an ordinary Call or an exported native entry wrapper.
    pub environment: Option<ClosureId>,
    /// Local IDs in source parameter order; parameters are initialized on entry.
    pub parameters: Vec<LocalId>,
    pub return_type: Type,
    pub result_mode: PassingMode,
    pub locals: Vec<LocalDecl>,
    /// ValueId is the index here. Every entry must have exactly one definition.
    pub values: Vec<Type>,
    pub blocks: Vec<BasicBlock>,
    pub entry: BlockId,
    pub span: Span,
}
pub type TypedCFG = TypedFunction;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalDecl {
    pub id: LocalId,
    pub name: String,
    pub ty: Type,
    pub mutable: bool,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub result: Option<ValueId>,
    pub kind: InstructionKind,
    pub span: Span,
}
/// Reason for leaving one lexical cleanup activation. This tag grants no authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CleanupExitKind {
    Fallthrough,
    Return,
    Error,
    Break,
    Continue,
}
fn cleanup_exit_tag(kind: CleanupExitKind) -> u8 {
    match kind {
        CleanupExitKind::Fallthrough => 0,
        CleanupExitKind::Return => 1,
        CleanupExitKind::Error => 2,
        CleanupExitKind::Break => 3,
        CleanupExitKind::Continue => 4,
    }
}
/// One checked step through a real Enum variant payload. It never creates a
/// view or grants ownership; each selected tag must be independently proved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VariantProjection {
    pub variant: u32,
    pub index: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstructionKind {
    /// Construct an empty owning prefix; no allocation. Element metadata is a
    /// raw claim and must be independently checked before executable admission.
    OwnedStoreNew {
        element: Type,
    },
    /// Explicit absolute capacity request; failure preserves the old header and
    /// every initialized element. Never expands runtime capacity into IR slots.
    OwnedStoreReserve {
        store: ValueId,
        capacity: ValueId,
    },
    /// Consume one complete element. Full capacity returns it in the error
    /// alternative of Element!Unit; success commits one initialized prefix slot.
    OwnedStorePush {
        store: ValueId,
        value: ValueId,
    },
    /// Consume the last initialized slot into Unit!Element, or return Unit error
    /// when empty. The returned element carries its complete responsibility.
    OwnedStorePop {
        store: ValueId,
    },
    OwnedStoreLen {
        store: ValueId,
    },
    OwnedStoreCapacity {
        store: ValueId,
    },
    /// Borrow a real initialized prefix element after a checked i64 index.
    /// Shared only; the Store retains responsibility for the complete element.
    OwnedStoreBorrow {
        store: ValueId,
        index: ValueId,
    },
    /// Exclusive, synchronous element window. The Store retains all element
    /// responsibility; only independently admitted Copy projections may change.
    OwnedStoreBorrowMut {
        store: ValueId,
        index: ValueId,
    },
    /// New empty managed storage; no allocation and no implicit user code.
    ByteBufferNew,
    /// Absolute-capacity request; failure preserves the complete old state.
    ByteBufferReserve {
        buffer: ValueId,
        capacity: ValueId,
    },
    /// Append one initialized byte without allocation; false if capacity is full.
    ByteBufferPush {
        buffer: ValueId,
        value: ValueId,
    },
    /// Shrink initialized prefix; false without mutation for negative/growing lengths.
    ByteBufferTruncate {
        buffer: ValueId,
        length: ValueId,
    },
    ByteBufferLen {
        buffer: ValueId,
    },
    ByteBufferCapacity {
        buffer: ValueId,
    },
    /// A fresh loan of the initialized prefix, rooted in the real carrier.
    ByteBufferBorrow {
        buffer: ValueId,
        mutable: bool,
    },
    /// Start a lexical activation; storage is owned by the innermost scope.
    ScopeEnter {
        scope: ScopeId,
    },
    /// End one activation after its storage and stored obligations are cleared.
    /// Owned SSA block results may cross this boundary; the independent resource
    /// checker still conserves their responsibilities through the whole function.
    ScopeExit {
        scope: ScopeId,
        kind: CleanupExitKind,
    },
    /// Register an initialized hidden FnOnce local in the current activation.
    /// This fact has no machine action; TakeLocal followed by CallClosureOnce
    /// performs the required cleanup, independently checked at every exit.
    RegisterDefer {
        scope: ScopeId,
        local: LocalId,
    },
    MakeErrorUnion {
        is_error: bool,
        value: ValueId,
    },
    ErrorTag {
        value: ValueId,
    },
    /// Runtime checked tag projection, never an unchecked reinterpretation.
    ReadErrorUnion {
        value: ValueId,
        is_error: bool,
    },
    /// Consume the complete non-Copy union and select its guarded active payload.
    /// The tag assertion grants no authority: Verify reconstructs branch guards.
    TakeErrorUnion {
        value: ValueId,
        is_error: bool,
    },
    /// Conditionally discharge the active owned payload of a local error union.
    DropErrorUnion {
        local: LocalId,
    },
    /// Construct a complete owned resource header; result is Managed<payload>.
    MakeManaged {
        fields: Vec<ValueId>,
    },
    /// Atomically consume fully prepared fields into a complete owned package.
    MakePackage {
        fields: Vec<ValueId>,
    },
    /// Atomically consume prepared homogeneous owned elements into an array.
    MakeOwnedArray {
        elements: Vec<ValueId>,
    },
    /// Read a Copy Package field or describe its Managed field at a static slot.
    /// This projection retains the owning array and its element responsibility.
    ProjectOwnedArrayField {
        local: LocalId,
        index: u32,
        field: u32,
    },
    /// Transfer one complete live element, retaining the array's local shell.
    TakeOwnedArrayElement {
        local: LocalId,
        index: u32,
    },
    /// Conditionally discharge one owned element before a prepared replacement.
    DropOwnedArrayElement {
        local: LocalId,
        index: u32,
    },
    /// Consume a prepared element into an empty slot of a mutable array local.
    StoreOwnedArrayElement {
        local: LocalId,
        index: u32,
        value: ValueId,
    },
    /// Conditionally discharge live elements in ascending index order.
    DropOwnedArray {
        local: LocalId,
    },
    /// Borrow one concrete Closure element at its actual static array slot.
    /// The owning array and element retain all cleanup responsibilities.
    BorrowOwnedArrayElement {
        local: LocalId,
        index: u32,
        mutable: bool,
    },
    /// Borrow a concrete Closure at its actual initialized Package field.
    /// The owner and field retain their cleanup responsibility.
    BorrowPackageField {
        local: LocalId,
        index: u32,
        mutable: bool,
    },
    /// Read a Copy field without loading or moving its owning package.
    ReadPackageField {
        local: LocalId,
        index: u32,
    },
    /// Borrow the actual resource header at this package field position.
    DescribePackageField {
        local: LocalId,
        index: u32,
    },
    /// Conditional whole cleanup, in declaration field order. The whole live
    /// flag is cleared before any field's user teardown is invoked.
    DropPackage {
        local: LocalId,
    },
    /// Transfer one supported live owning field, retaining the package shell.
    /// The result is an existing responsibility, never a fresh allocation.
    TakePackageField {
        local: LocalId,
        index: u32,
    },
    /// Conditionally discharge one owning field before a prepared replacement.
    /// This explicit cleanup never changes the shell or sibling field debts.
    DropPackageField {
        local: LocalId,
        index: u32,
    },
    /// Write a matching field of an active, mutable package local. Owning
    /// destinations must have no live debt and consume the input exactly once.
    StorePackageField {
        local: LocalId,
        index: u32,
        value: ValueId,
    },
    /// Activate the unique lexical view of one complete owned Package capture.
    /// Data, shell and field live flags alias the actual environment storage.
    /// Every Package capture is bound once in the entry ScopeEnter prefix;
    /// ordinary StorageLive/StorageDead and Package Read/TakeCapture are invalid.
    BindPackageCapture {
        local: LocalId,
        index: u32,
    },
    /// End the lexical view without changing any environment responsibility.
    /// Fn/FnMut exits retain a complete package; FnOnce may leave partial debt
    /// for caller-side environment cleanup in capture/field declaration order.
    EndPackageCapture {
        local: LocalId,
        index: u32,
    },
    /// Borrow an access description of the actual local header, not ownership.
    DescribeLocal {
        local: LocalId,
    },
    ReadDesc {
        desc: ValueId,
        index: u32,
    },
    WriteDesc {
        desc: ValueId,
        index: u32,
        value: ValueId,
    },
    /// Conditional lexical cleanup, tied to the actual runtime live state.
    DropManaged {
        local: LocalId,
    },
    Constant(Constant),
    /// Preserve the mathematical integer value, trapping when out of range.
    ConvertInteger {
        value: ValueId,
        target: Type,
    },
    Load(LocalId),
    /// Transfer a non-Copy carrier and make its source uninitialized.
    TakeLocal {
        local: LocalId,
    },
    /// Begin/end one activation of a lexical stack position; neither initializes it.
    StorageLive(LocalId),
    StorageDead(LocalId),
    /// Shared scalar address. The root is (enclosing FunctionId, LocalId).
    BorrowLocal {
        local: LocalId,
    },
    BorrowLocalMut {
        local: LocalId,
    },
    /// A child window; the parent retains no conflicting execution authority.
    ReborrowAddress {
        address: ValueId,
        mutable: bool,
    },
    ReadAddress {
        address: ValueId,
    },
    WriteAddress {
        address: ValueId,
        value: ValueId,
    },
    AddressField {
        address: ValueId,
        index: u32,
    },
    AddressIndex {
        address: ValueId,
        index: ValueId,
    },
    /// Range projection preserving its source authority; not a new reborrow.
    /// Bounds are checked before pointer arithmetic by the native consumer.
    AddressSlice {
        address: ValueId,
        start: Option<ValueId>,
        end: Option<ValueId>,
    },
    /// Metadata read still requires a live, accessible slice window.
    SliceLength {
        address: ValueId,
    },
    Store {
        local: LocalId,
        value: ValueId,
    },
    Unary {
        op: UnaryOp,
        operand: ValueId,
    },
    Binary {
        op: BinaryOp,
        left: ValueId,
        right: ValueId,
    },
    /// Unit calls have no result. All other calls have exactly one result.
    Call {
        function: FunctionId,
        arguments: Vec<ValueId>,
    },
    /// Explicit lexical unsafe boundary; independently reconstructed over the CFG.
    UnsafeEnter {
        region: u32,
    },
    UnsafeExit {
        region: u32,
    },
    /// Never interchangeable with Call or an ordinary FunctionRef.
    ForeignCall {
        function: FunctionId,
        arguments: Vec<ValueId>,
        region: u32,
    },
    /// A real, environment-free function address with its exact Copy signature.
    FunctionRef {
        function: FunctionId,
    },
    /// Static environment-free C ABI entry, never an ordinary indirect callee.
    ForeignFunctionRef {
        function: FunctionId,
    },
    /// The callee is evaluated before arguments. Unit calls have no result.
    CallIndirect {
        callee: ValueId,
        arguments: Vec<ValueId>,
    },
    MakeAggregate {
        fields: Vec<ValueId>,
    },
    MakeVariant {
        variant: u32,
        fields: Vec<ValueId>,
    },
    VariantTag {
        value: ValueId,
    },
    /// A checked projection: execution traps unless the active tag matches.
    ReadVariant {
        value: ValueId,
        variant: u32,
        index: u32,
    },
    /// Observe a complete live local through Enum-only checked projections.
    /// Empty path observes the root tag; the terminal type must be Enum.
    VariantTagLocal {
        local: LocalId,
        path: Vec<VariantProjection>,
    },
    /// Copy a strict finite Copy terminal through a nonempty Enum-only path.
    /// Every prefix must have the selected active tag; never produces a view.
    ReadVariantLocal {
        local: LocalId,
        path: Vec<VariantProjection>,
    },
    /// Atomically consume one complete Enum and initialize all variant fields.
    /// Fields are exact distinct vacant locals, registered in reverse order;
    /// this instruction has no SSA result and inherits no cleanup age.
    UnpackVariant {
        local: LocalId,
        variant: u32,
        fields: Vec<LocalId>,
    },
    /// Conditionally close only the actual active variant of an available Enum.
    /// Storage-live vacant/transferred states are no-ops; an exact all-path
    /// cleaned duplicate or storage-dead access remains invalid.
    DropEnum {
        local: LocalId,
    },
    ExtractIndex {
        aggregate: ValueId,
        index: ValueId,
    },
    UpdateIndex {
        aggregate: ValueId,
        index: ValueId,
        value: ValueId,
    },
    ExtractField {
        aggregate: ValueId,
        index: u32,
    },
    UpdateField {
        aggregate: ValueId,
        index: u32,
        value: ValueId,
    },
    MakeClosure {
        closure: ClosureId,
        captures: Vec<ValueId>,
    },
    ReadCapture {
        index: u32,
    },
    /// Describe a live Managed field of the active environment without moving it.
    DescribeCapture {
        index: u32,
    },
    /// Transfer a live owned field out of an active FnOnce environment.
    TakeCapture {
        index: u32,
    },
    /// Borrow a captured concrete closure at its actual environment address.
    /// No responsibility moves; Verify reconstructs capture and call authority.
    BorrowCapture {
        index: u32,
        mutable: bool,
    },
    /// Borrow the actual local Fn environment; never copy its owned fields.
    CallClosureShared {
        callee: LocalId,
        arguments: Vec<ValueId>,
    },
    /// Invoke a borrowed concrete environment at its real address. Only Fn and
    /// FnMut are valid requested modes; loading/copying the environment is not.
    CallClosureBorrowed {
        callee: ValueId,
        kind: CallableKind,
        arguments: Vec<ValueId>,
    },
    /// Consume FnOnce, then clean its remaining owned fields on normal return.
    CallClosureOnce {
        callee: ValueId,
        arguments: Vec<ValueId>,
    },
    /// Conditionally clean a live owning closure local in fixed capture order.
    DropClosure {
        local: LocalId,
    },
    /// Write a pure Copy field of the active FnMut environment.
    WriteCapture {
        index: u32,
        value: ValueId,
    },
    /// Invoke FnMut on its actual initialized, mutable local environment.
    CallClosureMut {
        callee: LocalId,
        arguments: Vec<ValueId>,
    },
    /// As with Call, Unit results have no result ValueId.
    CallClosure {
        callee: ValueId,
        arguments: Vec<ValueId>,
    },
    Unsupported(String),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Terminator {
    Return(Option<ValueId>),
    Jump(BlockId),
    Branch {
        condition: ValueId,
        then_block: BlockId,
        else_block: BlockId,
    },
    /// Defined, non-returning runtime failure; unlike Unreachable it may be reached.
    Trap,
    /// B0 rejects reachable Unreachable; it never licenses undefined behavior.
    Unreachable,
}

/// Raw candidate facts. The verifier recomputes and checks, never trusts them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidatePlan {
    pub schema_version: u32,
    pub module_fingerprint: Fingerprint,
    pub target_fingerprint: Fingerprint,
    pub functions: Vec<FunctionPlan>,
    pub origins: Vec<OriginClaim>,
    pub responsibilities: Vec<ResponsibilityClaim>,
    pub unsupported: Vec<UnsupportedFeature>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionPlan {
    pub function: FunctionId,
    pub blocks: Vec<BlockPlan>,
    /// Dense ValueId order; sorted unique roots. Raw claims, independently checked.
    pub holds: Vec<ValueHold>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueHold {
    pub value: ValueId,
    pub roots: Vec<HoldRoot>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HoldRoot {
    /// A position in the enclosing function. Names are not identities.
    Local(LocalId),
    /// An incoming shared address environment field, held by the caller.
    Capture(u32),
    /// Target of an incoming shared reference, indexed by source parameter order.
    Parameter(u32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockPlan {
    pub block: BlockId,
    /// Sorted, duplicate-free definite initialization sets at block boundaries.
    pub initialized_in: Vec<LocalId>,
    pub initialized_out: Vec<LocalId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginClaim {
    pub origin: OriginId,
    pub source: NodeId,
    pub multiplicity: OriginMultiplicity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginMultiplicity {
    One,
    Many,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponsibilityClaim {
    pub function: FunctionId,
    pub carrier: LocalId,
    pub origin: OriginId,
    pub state: ResponsibilityState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponsibilityState {
    Uninitialized,
    Live,
    Done,
    Moved,
}

/// B0 lowering retains the checked scalar CFG exactly. Backends must receive the
/// matching VerifiedModule and compare these public fields before code emission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LirModule {
    pub schema_version: u32,
    pub module_id: ModuleId,
    pub verified_fingerprint: Fingerprint,
    pub target: TargetProfile,
    pub functions: Vec<TypedFunction>,
    pub closures: Vec<ClosureDefinition>,
    pub resources: Vec<ResourceDefinition>,
    pub layouts: Vec<ScalarLayout>,
}
/// Borrowed raw LIR, not verification authority. Backends must still bind every
/// field to the current VerifiedModule. This view avoids copying the checked CFG
/// when lowering preserves it; the owned LirModule and its wire identity remain.
#[derive(Clone)]
pub struct LirView<'a> {
    pub schema_version: u32,
    pub module_id: ModuleId,
    pub verified_fingerprint: Fingerprint,
    pub target: &'a TargetProfile,
    pub functions: &'a [TypedFunction],
    pub closures: &'a [ClosureDefinition],
    pub resources: &'a [ResourceDefinition],
    pub layouts: std::borrow::Cow<'a, [ScalarLayout]>,
}
impl LirModule {
    pub fn as_view(&self) -> LirView<'_> {
        LirView {
            schema_version: self.schema_version,
            module_id: self.module_id,
            verified_fingerprint: self.verified_fingerprint,
            target: &self.target,
            functions: &self.functions,
            closures: &self.closures,
            resources: &self.resources,
            layouts: std::borrow::Cow::Borrowed(&self.layouts),
        }
    }
}
impl LirView<'_> {
    pub fn to_owned(&self) -> LirModule {
        LirModule {
            schema_version: self.schema_version,
            module_id: self.module_id,
            verified_fingerprint: self.verified_fingerprint,
            target: self.target.clone(),
            functions: self.functions.to_vec(),
            closures: self.closures.to_vec(),
            resources: self.resources.to_vec(),
            layouts: self.layouts.to_vec(),
        }
    }
}
impl std::fmt::Debug for LirView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LirModule")
            .field("schema_version", &self.schema_version)
            .field("module_id", &self.module_id)
            .field("verified_fingerprint", &self.verified_fingerprint)
            .field("target", &self.target)
            .field("functions", &self.functions)
            .field("closures", &self.closures)
            .field("resources", &self.resources)
            .field("layouts", &self.layouts)
            .finish()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalarLayout {
    pub ty: Type,
    pub size: u32,
    pub alignment: u32,
}

/// A raw, pure compile-time CFG candidate. The ordinary IR does not grant CT
/// authority: the independent verifier checks the closed operation set as well
/// as the ordinary control-flow/type/provenance invariants before execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtProgram {
    pub module: TypedModule,
    pub plan: CandidatePlan,
    pub entry: FunctionId,
    pub max_steps: u64,
    pub max_call_depth: usize,
    pub max_live_slots: usize,
}

/// A CT query's operation set is explicit and closed; unknown/forbidden
/// operations are data to reject, never dynamically executable host callbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtModule {
    pub schema_version: u32,
    pub input_fingerprint: Fingerprint,
    pub target: TargetProfile,
    pub instructions: Vec<CtInstruction>,
    pub result: ValueId,
    pub max_steps: u64,
    pub dependencies: Vec<CtDependency>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtDependency {
    pub name: String,
    pub fingerprint: Fingerprint,
    pub completed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtInstruction {
    pub result: ValueId,
    pub ty: Type,
    pub kind: CtInstructionKind,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtInstructionKind {
    Constant(Constant),
    ConvertInteger {
        value: ValueId,
        target: Type,
    },
    Unary {
        op: UnaryOp,
        operand: ValueId,
    },
    Binary {
        op: BinaryOp,
        left: ValueId,
        right: ValueId,
    },
    Forbidden(String),
}

impl TypedModule {
    pub fn fingerprint(&self) -> Fingerprint {
        // Versioned, length-delimited encoding is implemented below. This must
        // not depend on Debug formatting or process-randomized hashing.
        let mut bytes = FingerprintStream::new();
        encode_module(&mut bytes, self);
        bytes.finish()
    }
}
impl CandidatePlan {
    pub fn empty_for(module: &TypedModule) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            module_fingerprint: module.fingerprint(),
            target_fingerprint: module.target.fingerprint(),
            functions: Vec::new(),
            origins: Vec::new(),
            responsibilities: Vec::new(),
            unsupported: Vec::new(),
        }
    }
}

// Byte sinks share the exact versioned encoder. Vec remains available to the
// existing callable fingerprints and byte-level controls; only module hashing
// uses fixed storage instead of materializing the complete encoding.
trait FingerprintSink {
    fn push(&mut self, byte: u8);
    fn extend_from_slice(&mut self, bytes: &[u8]);
}
impl FingerprintSink for Vec<u8> {
    fn push(&mut self, byte: u8) {
        Vec::push(self, byte);
    }
    fn extend_from_slice(&mut self, bytes: &[u8]) {
        Vec::extend_from_slice(self, bytes);
    }
}
struct FingerprintStream {
    hash: FingerprintBuilder,
    buffer: [u8; 4096],
    used: usize,
}
impl FingerprintStream {
    fn new() -> Self {
        Self {
            hash: FingerprintBuilder::new(),
            buffer: [0; 4096],
            used: 0,
        }
    }
    fn flush(&mut self) {
        self.hash.update(&self.buffer[..self.used]);
        self.used = 0;
    }
    fn finish(mut self) -> Fingerprint {
        self.flush();
        self.hash.digest()
    }
}
impl FingerprintSink for FingerprintStream {
    fn push(&mut self, byte: u8) {
        if self.used == self.buffer.len() {
            self.flush();
        }
        self.buffer[self.used] = byte;
        self.used += 1;
    }
    fn extend_from_slice(&mut self, bytes: &[u8]) {
        if bytes.len() > self.buffer.len() - self.used {
            self.flush();
        }
        if bytes.len() >= self.buffer.len() {
            self.hash.update(bytes);
        } else {
            self.buffer[self.used..self.used + bytes.len()].copy_from_slice(bytes);
            self.used += bytes.len();
        }
    }
}

fn encode_module(bytes: &mut impl FingerprintSink, module: &TypedModule) {
    bytes.extend_from_slice(b"ZIL-TYPED-MODULE-V31\0");
    encode_u32(bytes, module.schema_version);
    encode_u32(bytes, module.module_id.0);
    bytes.extend_from_slice(&module.source_fingerprint.0);
    encode_text(bytes, &module.target.to_json());
    encode_len(bytes, module.functions.len());
    for f in &module.functions {
        encode_function(bytes, f);
    }
    encode_len(bytes, module.closures.len());
    for closure in &module.closures {
        encode_u32(bytes, closure.id.0);
        encode_u32(bytes, closure.function.0);
        encode_len(bytes, closure.captures.len());
        for ty in &closure.captures {
            encode_type(bytes, ty);
        }
        encode_len(bytes, closure.parameters.len());
        for ty in &closure.parameters {
            encode_type(bytes, ty);
        }
        encode_type(bytes, &closure.result);
        bytes.push(match closure.kind {
            CallableKind::FunctionPointer => 0,
            CallableKind::Fn => 1,
            CallableKind::FnMut => 2,
            CallableKind::FnOnce => 3,
        });
        encode_len(bytes, closure.lending_captures.len());
        for index in &closure.lending_captures {
            encode_u32(bytes, *index);
        }
    }
    encode_len(bytes, module.resources.len());
    for resource in &module.resources {
        encode_text(bytes, &resource.name);
        encode_type(bytes, &resource.payload);
        bytes.push(u8::from(resource.teardown.is_some()));
        if let Some(teardown) = resource.teardown {
            encode_u32(bytes, teardown.0);
        }
    }
    encode_len(bytes, module.unsupported.len());
    for feature in &module.unsupported {
        encode_text(bytes, &feature.feature);
        encode_optional_span(bytes, feature.span);
    }
}
fn encode_u32(b: &mut impl FingerprintSink, n: u32) {
    b.extend_from_slice(&n.to_le_bytes());
}
fn encode_len(b: &mut impl FingerprintSink, n: usize) {
    b.extend_from_slice(&(n as u64).to_le_bytes());
}
fn encode_text(b: &mut impl FingerprintSink, s: &str) {
    encode_len(b, s.len());
    b.extend_from_slice(s.as_bytes());
}
fn encode_span(b: &mut impl FingerprintSink, s: Span) {
    encode_u32(b, s.file.0);
    encode_len(b, s.start);
    encode_len(b, s.end);
}
fn encode_optional_span(b: &mut impl FingerprintSink, s: Option<Span>) {
    b.push(u8::from(s.is_some()));
    if let Some(s) = s {
        encode_span(b, s);
    }
}
fn encode_type(b: &mut impl FingerprintSink, t: &Type) {
    match t {
        Type::Unit => b.push(0),
        Type::Bool => b.push(1),
        Type::I32 => b.push(2),
        Type::I64 => b.push(3),
        Type::U8 => b.push(18),
        Type::U16 => b.push(19),
        Type::ByteBuffer => b.push(24),
        Type::OwnedStore { element } => {
            b.push(25);
            encode_type(b, element);
        }
        Type::ForeignHandle => b.push(20),
        Type::CWideString => b.push(21),
        Type::ForeignCallback { parameters, result } => {
            b.push(22);
            encode_len(b, parameters.len());
            for ty in parameters {
                encode_type(b, ty);
            }
            encode_type(b, result);
        }
        Type::Tuple(ts) => {
            b.push(4);
            encode_len(b, ts.len());
            for t in ts {
                encode_type(b, t);
            }
        }
        Type::Array { element, length } => {
            b.push(14);
            encode_type(b, element);
            encode_u32(b, *length);
        }
        Type::Slice { element } => {
            b.push(23);
            encode_type(b, element);
        }
        Type::Record { name, fields } => {
            b.push(13);
            encode_text(b, name);
            encode_len(b, fields.len());
            for (name, ty) in fields {
                encode_text(b, name);
                encode_type(b, ty);
            }
        }
        Type::Package { name, fields } => {
            b.push(17);
            encode_text(b, name);
            encode_len(b, fields.len());
            for (name, ty) in fields {
                encode_text(b, name);
                encode_type(b, ty);
            }
        }
        Type::Enum { name, variants } => {
            b.push(15);
            encode_text(b, name);
            encode_len(b, variants.len());
            for (name, fields) in variants {
                encode_text(b, name);
                encode_len(b, fields.len());
                for ty in fields {
                    encode_type(b, ty);
                }
            }
        }
        Type::ErrorUnion { error, result } => {
            b.push(16);
            encode_type(b, error);
            encode_type(b, result);
        }
        Type::Named(s) => {
            b.push(5);
            encode_text(b, s);
        }
        Type::Address { mutable, inner } => {
            b.extend_from_slice(&[6, u8::from(*mutable)]);
            encode_type(b, inner);
        }
        Type::Managed(t) => {
            b.push(7);
            encode_type(b, t);
        }
        Type::Desc(t) => {
            b.push(8);
            encode_type(b, t);
        }
        Type::FunctionPointer { parameters, result } => {
            b.push(9);
            encode_len(b, parameters.len());
            for t in parameters {
                encode_type(b, t);
            }
            encode_type(b, result);
        }
        Type::Closure(id) => {
            b.push(10);
            encode_u32(b, id.0);
        }
        Type::Opaque { identity, hidden } => {
            b.push(11);
            encode_u32(b, identity.0);
            encode_type(b, hidden);
        }
        Type::Unsupported(s) => {
            b.push(12);
            encode_text(b, s);
        }
    }
}
fn encode_function(b: &mut impl FingerprintSink, f: &TypedFunction) {
    b.push(u8::from(f.callable_return_contract.is_some()));
    if let Some(contract) = &f.callable_return_contract {
        encode_callable_return_contract(b, contract);
    }
    b.push(u8::from(f.callable_contract.is_some()));
    if let Some(contract) = &f.callable_contract {
        encode_callable_contract(b, contract);
    }
    encode_u32(b, f.id.0);
    encode_text(b, &f.name);
    b.push(u8::from(f.foreign.is_some()));
    if let Some(foreign) = &f.foreign {
        encode_text(b, &foreign.abi);
        encode_text(b, &foreign.symbol);
        encode_len(b, foreign.noescape_parameters.len());
        for parameter in &foreign.noescape_parameters {
            encode_u32(b, *parameter);
        }
        b.push(u8::from(foreign.static_callbacks));
    }
    b.push(u8::from(f.environment.is_some()));
    if let Some(environment) = f.environment {
        encode_u32(b, environment.0);
    }
    encode_len(b, f.parameters.len());
    for p in &f.parameters {
        encode_u32(b, p.0);
    }
    encode_type(b, &f.return_type);
    b.push(passing_mode_tag(f.result_mode));
    encode_len(b, f.locals.len());
    for l in &f.locals {
        encode_u32(b, l.id.0);
        encode_text(b, &l.name);
        encode_type(b, &l.ty);
        b.push(u8::from(l.mutable));
        encode_span(b, l.span);
    }
    encode_len(b, f.values.len());
    for t in &f.values {
        encode_type(b, t);
    }
    encode_len(b, f.blocks.len());
    for block in &f.blocks {
        encode_u32(b, block.id.0);
        encode_len(b, block.instructions.len());
        for i in &block.instructions {
            b.push(u8::from(i.result.is_some()));
            if let Some(v) = i.result {
                encode_u32(b, v.0);
            }
            encode_instruction(b, &i.kind);
            encode_span(b, i.span);
        }
        match &block.terminator {
            Terminator::Return(v) => {
                b.push(0);
                b.push(u8::from(v.is_some()));
                if let Some(v) = v {
                    encode_u32(b, v.0);
                }
            }
            Terminator::Jump(t) => {
                b.push(1);
                encode_u32(b, t.0);
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                b.push(2);
                encode_u32(b, condition.0);
                encode_u32(b, then_block.0);
                encode_u32(b, else_block.0);
            }
            Terminator::Unreachable => b.push(3),
            Terminator::Trap => b.push(4),
        }
    }
    encode_u32(b, f.entry.0);
    encode_span(b, f.span);
}
fn encode_constant(b: &mut impl FingerprintSink, c: &Constant) {
    match c {
        Constant::Unit => b.push(0),
        Constant::Bool(v) => b.extend_from_slice(&[1, u8::from(*v)]),
        Constant::U8(v) => b.extend_from_slice(&[4, *v]),
        Constant::U16(v) => {
            b.push(5);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::NullHandle => b.push(6),
        Constant::NullWideString => b.push(7),
        Constant::WideString(units) => {
            b.push(8);
            encode_len(b, units.len());
            for unit in units {
                b.extend_from_slice(&unit.to_le_bytes());
            }
        }
        Constant::I32(v) => {
            b.push(2);
            b.extend_from_slice(&v.to_le_bytes());
        }
        Constant::I64(v) => {
            b.push(3);
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
}
fn encode_instruction(b: &mut impl FingerprintSink, i: &InstructionKind) {
    match i {
        InstructionKind::ConvertInteger { value, target } => {
            b.push(59);
            encode_u32(b, value.0);
            encode_type(b, target);
        }
        InstructionKind::MakePackage { fields } => {
            b.push(49);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::MakeOwnedArray { elements } => {
            b.push(72);
            encode_len(b, elements.len());
            for element in elements {
                encode_u32(b, element.0);
            }
        }
        InstructionKind::ProjectOwnedArrayField {
            local,
            index,
            field,
        } => {
            b.push(73);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            encode_u32(b, *field);
        }
        InstructionKind::TakeOwnedArrayElement { local, index } => {
            b.push(74);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DropOwnedArrayElement { local, index } => {
            b.push(75);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::StoreOwnedArrayElement {
            local,
            index,
            value,
        } => {
            b.push(76);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::DropOwnedArray { local } => {
            b.push(77);
            encode_u32(b, local.0);
        }
        InstructionKind::BorrowOwnedArrayElement {
            local,
            index,
            mutable,
        } => {
            b.push(79);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            b.push(u8::from(*mutable));
        }
        InstructionKind::BorrowPackageField {
            local,
            index,
            mutable,
        } => {
            b.push(87);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            b.push(u8::from(*mutable));
        }
        InstructionKind::ReadPackageField { local, index } => {
            b.push(50);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DescribePackageField { local, index } => {
            b.push(51);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DropPackage { local } => {
            b.push(52);
            encode_u32(b, local.0);
        }
        InstructionKind::TakePackageField { local, index } => {
            b.push(54);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::DropPackageField { local, index } => {
            b.push(55);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::StorePackageField {
            local,
            index,
            value,
        } => {
            b.push(56);
            encode_u32(b, local.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::BindPackageCapture { local, index } => {
            b.push(57);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::EndPackageCapture { local, index } => {
            b.push(58);
            encode_u32(b, local.0);
            encode_u32(b, *index);
        }
        InstructionKind::RegisterDefer { scope, local } => {
            b.push(89);
            encode_u32(b, scope.0);
            encode_u32(b, local.0);
        }
        InstructionKind::ScopeEnter { scope } => {
            b.push(44);
            encode_u32(b, scope.0);
        }
        InstructionKind::ScopeExit { scope, kind } => {
            b.push(45);
            encode_u32(b, scope.0);
            b.push(cleanup_exit_tag(*kind));
        }
        InstructionKind::MakeErrorUnion { is_error, value } => {
            b.push(46);
            b.push(u8::from(*is_error));
            encode_u32(b, value.0);
        }
        InstructionKind::ErrorTag { value } => {
            b.push(47);
            encode_u32(b, value.0);
        }
        InstructionKind::ReadErrorUnion { value, is_error } => {
            b.push(48);
            encode_u32(b, value.0);
            b.push(u8::from(*is_error));
        }
        InstructionKind::TakeErrorUnion { value, is_error } => {
            b.push(70);
            encode_u32(b, value.0);
            b.push(u8::from(*is_error));
        }
        InstructionKind::DropErrorUnion { local } => {
            b.push(71);
            encode_u32(b, local.0);
        }
        InstructionKind::MakeManaged { fields } => {
            b.push(34);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::DescribeLocal { local } => {
            b.push(35);
            encode_u32(b, local.0);
        }
        InstructionKind::ReadDesc { desc, index } => {
            b.push(36);
            encode_u32(b, desc.0);
            encode_u32(b, *index);
        }
        InstructionKind::WriteDesc { desc, index, value } => {
            b.push(37);
            encode_u32(b, desc.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::DropManaged { local } => {
            b.push(38);
            encode_u32(b, local.0);
        }
        InstructionKind::DescribeCapture { index } => {
            b.push(39);
            encode_u32(b, *index);
        }
        InstructionKind::TakeCapture { index } => {
            b.push(40);
            encode_u32(b, *index);
        }
        InstructionKind::BorrowCapture { index, mutable } => {
            b.push(78);
            encode_u32(b, *index);
            b.push(u8::from(*mutable));
        }
        InstructionKind::CallClosureBorrowed {
            callee,
            kind,
            arguments,
        } => {
            b.push(53);
            encode_u32(b, callee.0);
            b.push(callable_kind_tag(*kind));
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::CallClosureShared { callee, arguments } => {
            b.push(41);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::CallClosureOnce { callee, arguments } => {
            b.push(42);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::DropClosure { local } => {
            b.push(43);
            encode_u32(b, local.0);
        }
        InstructionKind::UnsafeEnter { region } => {
            b.push(31);
            encode_u32(b, *region);
        }
        InstructionKind::UnsafeExit { region } => {
            b.push(32);
            encode_u32(b, *region);
        }
        InstructionKind::ForeignCall {
            function,
            arguments,
            region,
        } => {
            b.push(33);
            encode_u32(b, function.0);
            encode_u32(b, *region);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::Constant(c) => {
            b.push(0);
            encode_constant(b, c);
        }
        InstructionKind::Load(l) => {
            b.push(1);
            encode_u32(b, l.0);
        }
        InstructionKind::Store { local, value } => {
            b.push(2);
            encode_u32(b, local.0);
            encode_u32(b, value.0);
        }
        InstructionKind::Unary { op, operand } => {
            b.push(3);
            b.push(*op as u8);
            encode_u32(b, operand.0);
        }
        InstructionKind::Binary { op, left, right } => {
            b.push(4);
            b.push(*op as u8);
            encode_u32(b, left.0);
            encode_u32(b, right.0);
        }
        InstructionKind::Call {
            function,
            arguments,
        } => {
            b.push(5);
            encode_u32(b, function.0);
            encode_len(b, arguments.len());
            for a in arguments {
                encode_u32(b, a.0);
            }
        }
        InstructionKind::ForeignFunctionRef { function } => {
            b.push(60);
            encode_u32(b, function.0);
        }
        InstructionKind::FunctionRef { function } => {
            b.push(25);
            encode_u32(b, function.0);
        }
        InstructionKind::CallIndirect { callee, arguments } => {
            b.push(26);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::ExtractIndex { aggregate, index } => {
            b.push(16);
            encode_u32(b, aggregate.0);
            encode_u32(b, index.0);
        }
        InstructionKind::UpdateIndex {
            aggregate,
            index,
            value,
        } => {
            b.push(17);
            encode_u32(b, aggregate.0);
            encode_u32(b, index.0);
            encode_u32(b, value.0);
        }
        InstructionKind::MakeAggregate { fields } => {
            b.push(13);
            encode_len(b, fields.len());
            for value in fields {
                encode_u32(b, value.0);
            }
        }
        InstructionKind::MakeVariant { variant, fields } => {
            b.push(18);
            encode_u32(b, *variant);
            encode_len(b, fields.len());
            for value in fields {
                encode_u32(b, value.0);
            }
        }
        InstructionKind::VariantTag { value } => {
            b.push(19);
            encode_u32(b, value.0);
        }
        InstructionKind::ReadVariant {
            value,
            variant,
            index,
        } => {
            b.push(20);
            encode_u32(b, value.0);
            encode_u32(b, *variant);
            encode_u32(b, *index);
        }
        InstructionKind::VariantTagLocal { local, path }
        | InstructionKind::ReadVariantLocal { local, path } => {
            b.push(if matches!(i, InstructionKind::VariantTagLocal { .. }) {
                90
            } else {
                91
            });
            encode_u32(b, local.0);
            encode_len(b, path.len());
            for step in path {
                encode_u32(b, step.variant);
                encode_u32(b, step.index);
            }
        }
        InstructionKind::UnpackVariant {
            local,
            variant,
            fields,
        } => {
            b.push(92);
            encode_u32(b, local.0);
            encode_u32(b, *variant);
            encode_len(b, fields.len());
            for field in fields {
                encode_u32(b, field.0);
            }
        }
        InstructionKind::DropEnum { local } => {
            b.push(93);
            encode_u32(b, local.0);
        }
        InstructionKind::ExtractField { aggregate, index } => {
            b.push(14);
            encode_u32(b, aggregate.0);
            encode_u32(b, *index);
        }
        InstructionKind::UpdateField {
            aggregate,
            index,
            value,
        } => {
            b.push(15);
            encode_u32(b, aggregate.0);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::MakeClosure { closure, captures } => {
            b.push(6);
            encode_u32(b, closure.0);
            encode_len(b, captures.len());
            for capture in captures {
                encode_u32(b, capture.0);
            }
        }
        InstructionKind::ReadCapture { index } => {
            b.push(7);
            encode_u32(b, *index);
        }
        InstructionKind::WriteCapture { index, value } => {
            b.push(22);
            encode_u32(b, *index);
            encode_u32(b, value.0);
        }
        InstructionKind::CallClosureMut { callee, arguments } => {
            b.push(21);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::CallClosure { callee, arguments } => {
            b.push(8);
            encode_u32(b, callee.0);
            encode_len(b, arguments.len());
            for argument in arguments {
                encode_u32(b, argument.0);
            }
        }
        InstructionKind::BorrowLocal { local } => {
            b.push(9);
            encode_u32(b, local.0);
        }
        InstructionKind::BorrowLocalMut { local } => {
            b.push(27);
            encode_u32(b, local.0);
        }
        InstructionKind::ReborrowAddress { address, mutable } => {
            b.push(28);
            encode_u32(b, address.0);
            b.push(u8::from(*mutable));
        }
        InstructionKind::WriteAddress { address, value } => {
            b.push(29);
            encode_u32(b, address.0);
            encode_u32(b, value.0);
        }
        InstructionKind::TakeLocal { local } => {
            b.push(30);
            encode_u32(b, local.0);
        }
        InstructionKind::ReadAddress { address } => {
            b.push(10);
            encode_u32(b, address.0);
        }
        InstructionKind::AddressField { address, index } => {
            b.push(23);
            encode_u32(b, address.0);
            encode_u32(b, *index);
        }
        InstructionKind::AddressIndex { address, index } => {
            b.push(24);
            encode_u32(b, address.0);
            encode_u32(b, index.0);
        }
        InstructionKind::AddressSlice {
            address,
            start,
            end,
        } => {
            b.push(61);
            encode_u32(b, address.0);
            for endpoint in [start, end] {
                b.push(u8::from(endpoint.is_some()));
                if let Some(value) = endpoint {
                    encode_u32(b, value.0);
                }
            }
        }
        InstructionKind::SliceLength { address } => {
            b.push(62);
            encode_u32(b, address.0);
        }
        InstructionKind::ByteBufferNew => b.push(63),
        InstructionKind::OwnedStoreNew { element } => {
            b.push(80);
            encode_type(b, element);
        }
        InstructionKind::OwnedStoreReserve { store, capacity } => {
            b.push(81);
            encode_u32(b, store.0);
            encode_u32(b, capacity.0);
        }
        InstructionKind::OwnedStorePush { store, value } => {
            b.push(82);
            encode_u32(b, store.0);
            encode_u32(b, value.0);
        }
        InstructionKind::OwnedStorePop { store } => {
            b.push(83);
            encode_u32(b, store.0);
        }
        InstructionKind::OwnedStoreLen { store } => {
            b.push(84);
            encode_u32(b, store.0);
        }
        InstructionKind::OwnedStoreCapacity { store } => {
            b.push(85);
            encode_u32(b, store.0);
        }
        InstructionKind::OwnedStoreBorrow { store, index } => {
            b.push(86);
            encode_u32(b, store.0);
            encode_u32(b, index.0);
        }
        InstructionKind::OwnedStoreBorrowMut { store, index } => {
            b.push(88);
            encode_u32(b, store.0);
            encode_u32(b, index.0);
        }
        InstructionKind::ByteBufferReserve { buffer, capacity } => {
            b.push(64);
            encode_u32(b, buffer.0);
            encode_u32(b, capacity.0);
        }
        InstructionKind::ByteBufferPush { buffer, value } => {
            b.push(65);
            encode_u32(b, buffer.0);
            encode_u32(b, value.0);
        }
        InstructionKind::ByteBufferTruncate { buffer, length } => {
            b.push(69);
            encode_u32(b, buffer.0);
            encode_u32(b, length.0);
        }
        InstructionKind::ByteBufferLen { buffer } => {
            b.push(66);
            encode_u32(b, buffer.0);
        }
        InstructionKind::ByteBufferCapacity { buffer } => {
            b.push(67);
            encode_u32(b, buffer.0);
        }
        InstructionKind::ByteBufferBorrow { buffer, mutable } => {
            b.push(68);
            encode_u32(b, buffer.0);
            b.push(u8::from(*mutable));
        }
        InstructionKind::StorageLive(local) => {
            b.push(11);
            encode_u32(b, local.0);
        }
        InstructionKind::StorageDead(local) => {
            b.push(12);
            encode_u32(b, local.0);
        }
        InstructionKind::Unsupported(s) => {
            b.push(255);
            encode_text(b, s);
        }
    }
}

/// Deterministic binding for a raw callable relation, not a verification token.
pub fn callable_contract_fingerprint(contract: &CallableParameterContract) -> Fingerprint {
    let mut bytes = b"ZIL-CALLABLE-CONTRACT-4\0".to_vec();
    encode_callable_contract(&mut bytes, contract);
    Fingerprint::of(&bytes)
}
/// Deterministic identity only; this hash never grants verification authority.
pub fn callable_return_contract_fingerprint(contract: &CallableReturnContract) -> Fingerprint {
    let mut bytes = b"ZIL-CALLABLE-RETURN-CONTRACT-3\0".to_vec();
    encode_callable_return_contract(&mut bytes, contract);
    Fingerprint::of(&bytes)
}
fn encode_callable_return_contract(b: &mut impl FingerprintSink, c: &CallableReturnContract) {
    match c.source {
        CallableReturnSource::Fresh => b.push(0),
        CallableReturnSource::Parameter(parameter) => {
            b.push(1);
            encode_u32(b, parameter);
        }
    }
    encode_u32(b, c.closure.0);
    b.push(callable_kind_tag(c.kind));
    for indices in [&c.hold, &c.call, &c.writes, &c.cleanup, &c.lending_captures] {
        encode_len(b, indices.len());
        for index in indices {
            encode_u32(b, *index);
        }
    }
}
fn callable_kind_tag(kind: CallableKind) -> u8 {
    match kind {
        CallableKind::FunctionPointer => 0,
        CallableKind::Fn => 1,
        CallableKind::FnMut => 2,
        CallableKind::FnOnce => 3,
    }
}
fn encode_callable_contract(b: &mut impl FingerprintSink, c: &CallableParameterContract) {
    encode_len(b, c.roles.len());
    for role in &c.roles {
        encode_u32(b, role.parameter);
        encode_u32(b, role.closure.0);
        b.push(callable_kind_tag(role.kind));
        for indices in [&role.hold, &role.call, &role.writes] {
            encode_len(b, indices.len());
            for index in indices {
                encode_u32(b, *index);
            }
        }
        b.push(match role.cleanup {
            CallableCleanup::Owned => 0,
            CallableCleanup::Borrowed => 1,
            CallableCleanup::Returned => 2,
        });
    }
    match &c.traces {
        CallableTraces::Finite(traces) => {
            b.push(0);
            encode_len(b, traces.len());
            for trace in traces {
                encode_callable_calls(b, &trace.calls);
                encode_callable_exit(b, trace.exit);
            }
        }
        CallableTraces::Flow(flow) => {
            b.push(1);
            encode_u32(b, flow.entry);
            encode_len(b, flow.nodes.len());
            for node in &flow.nodes {
                encode_callable_calls(b, &node.calls);
                match &node.next {
                    CallableFlowNext::Edges(edges) => {
                        b.push(0);
                        encode_len(b, edges.len());
                        for edge in edges {
                            encode_u32(b, *edge);
                        }
                    }
                    CallableFlowNext::Exit(exit) => {
                        b.push(1);
                        encode_callable_exit(b, *exit);
                    }
                }
            }
        }
    }
}
fn encode_callable_calls(b: &mut impl FingerprintSink, calls: &[CallEvent]) {
    encode_len(b, calls.len());
    for call in calls {
        encode_u32(b, call.parameter);
        b.push(callable_kind_tag(call.kind));
    }
}
fn encode_callable_exit(b: &mut impl FingerprintSink, exit: CallableExit) {
    b.push(match exit {
        CallableExit::Return => 0,
        CallableExit::Trap => 1,
    });
}
