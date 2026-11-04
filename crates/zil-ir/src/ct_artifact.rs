//! Raw CT-only artifact schema, independently versioned from runtime Wire.
//! All IDs and records are untrusted data. Verification must be independent.
use crate::{BinaryOp, Constant, UnaryOp};
use zil_base::{Fingerprint, ModuleId, NodeId, Span};

pub const CT_ARTIFACT_SCHEMA_VERSION: u32 = 3;
pub const CT_ARTIFACT_FINGERPRINT_DOMAIN: &[u8] = b"zil/ct-artifact/v3";
pub const CT_PROVIDER_FINGERPRINT_DOMAIN: &[u8] = b"zil/ct-provider/v3";
pub const CT_DEPENDENCY_FINGERPRINT_DOMAIN: &[u8] = b"zil/ct-dependencies/v3";

macro_rules! ct_id {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);
    )+};
}
ct_id!(
    CtFunctionId,
    CtBlockId,
    CtLocalId,
    CtValueId,
    CtConstantId,
    CtTemplateId,
    CtSyntaxNodeId,
    CtTokenNodeId,
    CtOriginId,
    CtInputTokenId,
    CtRecordSchemaId,
    CtEnumSchemaId,
    CtTypeNodeId,
);

/// A declaration identity, not a visibility grant. Generated declarations also
/// carry a deterministic path. The verifier resolves each ID in a frozen view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSymbolId {
    pub module: ModuleId,
    pub declaration: NodeId,
    pub generation_path: Option<Fingerprint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtScalarTy {
    Unit,
    Bool,
    U8,
    U16,
    I32,
    I64,
    U64,
    Usize,
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtSyntaxCategory {
    Expr,
    Type,
    StmtList,
    ItemList,
}

/// Pure CT types only. No Address, Managed, Desc, closure, FFI, or runtime ABI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtTy {
    Scalar(CtScalarTy),
    Type,
    Syntax(CtSyntaxCategory),
    Ident,
    TokenTree,
    TokenCursor,
    Text,
    ShapeInfo,
    FieldInfo,
    ImplInfo,
    LayoutInfo,
    Tuple(Vec<CtTy>),
    Array {
        element: Box<CtTy>,
        length: u32,
    },
    List(Box<CtTy>),
    Record(CtRecordSchemaId),
    Enum(CtEnumSchemaId),
    /// Strictly canonical complete structural member identities. Record/Enum
    /// references resolve their public nominal symbol and ordered arguments;
    /// numeric schema IDs and payload layout are not member identities.
    AnonymousSum(Vec<CtTy>),
}

/// Public user schemas are pure data. Built-in query result schemas are not
/// constructible through MakeRecord/MakeVariant; verifier checks their origin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSchemaArgument {
    Type(CtTy),
    Scalar(Constant),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtRecordSchema {
    pub name: CtSymbolId,
    /// Complete actual arguments in declaration parameter order, including
    /// phantom arguments. Data alone grants no declaration access or bounds.
    pub arguments: Vec<CtSchemaArgument>,
    pub fields: Vec<CtTy>,
    pub origin: CtSchemaOrigin,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtEnumSchema {
    pub name: CtSymbolId,
    pub arguments: Vec<CtSchemaArgument>,
    pub variants: Vec<Vec<CtTy>>,
    pub origin: CtSchemaOrigin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtSchemaOrigin {
    UserPure,
    BuiltinQuery,
}

/// Existing runtime type spellings represented as CT data, never a live resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtBuiltinType {
    ByteBuffer,
    ForeignHandle,
    CWideString,
}

/// Type identity is structural for constructors and nominal for declarations.
/// Nodes are a bounded, canonical DAG; raw nodes cannot authorize visibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtTypeRef {
    pub nodes: Vec<CtTypeNode>,
    pub root: CtTypeNodeId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtTypeNode {
    Primitive(CtScalarTy),
    Builtin(CtBuiltinType),
    Nominal {
        symbol: CtSymbolId,
        arguments: Vec<CtTypeArgument>,
    },
    Tuple(Vec<CtTypeNodeId>),
    Array {
        element: CtTypeNodeId,
        length: u32,
    },
    FunctionPointer {
        parameters: Vec<CtTypeNodeId>,
        result: CtTypeNodeId,
    },
    Address {
        mutable: bool,
        inner: CtTypeNodeId,
    },
    Slice(CtTypeNodeId),
    Managed(CtTypeNodeId),
    Desc(CtTypeNodeId),
    OwnedStore(CtTypeNodeId),
    ForeignCallback {
        parameters: Vec<CtTypeNodeId>,
        result: CtTypeNodeId,
    },
    /// The private underlying type stays in the frozen type table; a Type
    /// value alone does not reveal it or grant Shape/Layout access.
    Opaque {
        identity: CtSymbolId,
    },
    ErrorUnion {
        error: CtTypeNodeId,
        result: CtTypeNodeId,
    },
    /// Canonical structural tagged sum of complete member type identities.
    /// Raw nodes confer no constructor, copy, ownership or access authority.
    AnonymousSum(Vec<CtTypeNodeId>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtTypeArgument {
    Type(CtTypeNodeId),
    Scalar(Constant),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtNameNamespace {
    /// Fresh/export intent only; the actual name slot supplies its namespace.
    Unbound,
    Value,
    Type,
    Member,
    Module,
    Label,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtNameSource {
    Input(CtInputTokenId),
    Definition(CtSymbolId),
    Fresh {
        invocation: Fingerprint,
        binder: u32,
    },
    /// Only a declaration-name position can consume this intent.
    Export,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtIdent {
    pub text: String,
    pub namespace: CtNameNamespace,
    pub source: CtNameSource,
    /// Field/member names carry owner identity, not just matching text.
    pub member_owner: Option<CtSymbolId>,
    /// Full instantiated owner; a declaration ID alone loses generic arguments.
    pub member_owner_type: Option<CtTypeRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtDelimiter {
    Parenthesis,
    Bracket,
    Brace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtTokenClass {
    Identifier,
    Integer,
    String,
    Punctuation,
    Keyword,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtOriginRole {
    Input,
    QuoteDefinition,
    Generated,
    Splice,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtOrigin {
    pub span: Span,
    pub parent: Option<CtOriginId>,
    pub role: CtOriginRole,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtToken {
    pub class: CtTokenClass,
    pub text: String,
    pub origin: CtOriginId,
    pub name: Option<CtIdent>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtTokenNode {
    Token(CtToken),
    Group {
        delimiter: CtDelimiter,
        children: Vec<CtTokenNodeId>,
        origin: CtOriginId,
    },
}
/// Provider input is unclassified source tokens, never Syntax<Expr> by fiat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtTokenTree {
    pub nodes: Vec<CtTokenNode>,
    pub roots: Vec<CtTokenNodeId>,
    pub origins: Vec<CtOrigin>,
}
/// A checked cursor into an immutable tree, rather than a detached token with
/// a forgeable binding. Execution checks the tree fingerprint and node bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtTokenCursor {
    /// Fingerprint includes token text, tree structure, scope and origin data.
    pub tree_fingerprint: Fingerprint,
    pub node: CtTokenNodeId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtSyntaxNode {
    Token(CtToken),
    Group {
        delimiter: CtDelimiter,
        children: Vec<CtSyntaxNodeId>,
        origin: CtOriginId,
    },
    /// Only templates may contain holes. Evaluated Syntax has none.
    Hole {
        index: u32,
        origin: CtOriginId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSyntaxTree {
    pub category: CtSyntaxCategory,
    pub nodes: Vec<CtSyntaxNode>,
    pub roots: Vec<CtSyntaxNodeId>,
    pub origins: Vec<CtOrigin>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtHoleKind {
    Expr,
    Type,
    StmtList,
    ItemList,
    Ident(CtNameNamespace),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtTemplate {
    pub definition: CtSymbolId,
    pub syntax: CtSyntaxTree,
    pub holes: Vec<CtHoleKind>,
}
/// Raw requests are authenticated against the actual invocation source map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtInputToken {
    pub span: Span,
    pub class: CtTokenClass,
    pub text: String,
    pub scope_fingerprint: Fingerprint,
    pub binding: Option<CtSymbolId>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtInputTable {
    pub invocation_fingerprint: Fingerprint,
    pub tokens: Vec<CtInputToken>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtFieldInfo {
    pub owner: CtSymbolId,
    pub owner_type: CtTypeRef,
    pub field: CtSymbolId,
    pub name: CtIdent,
    pub ty: CtTypeRef,
    pub managed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtShapeInfo {
    pub nominal: CtSymbolId,
    pub visible_fields: Vec<CtFieldInfo>,
    /// False means hidden members exist; absence cannot prove Copy/Store.
    pub complete: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtImplInfo {
    pub trait_symbol: CtSymbolId,
    pub implementation: CtSymbolId,
    pub for_type: CtTypeRef,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtLayoutInfo {
    pub target_fingerprint: Fingerprint,
    pub size: u64,
    pub alignment: u64,
    pub field_offsets: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtValue {
    Scalar(Constant),
    Text(String),
    Type(CtTypeRef),
    Syntax(CtSyntaxTree),
    Ident(CtIdent),
    TokenTree(CtTokenTree),
    TokenCursor(CtTokenCursor),
    ShapeInfo(CtShapeInfo),
    FieldInfo(CtFieldInfo),
    ImplInfo(CtImplInfo),
    LayoutInfo(CtLayoutInfo),
    Tuple(Vec<CtValue>),
    Array(Vec<CtValue>),
    List(Vec<CtValue>),
    Record {
        schema: CtRecordSchemaId,
        fields: Vec<CtValue>,
    },
    Variant {
        schema: CtEnumSchemaId,
        tag: u32,
        fields: Vec<CtValue>,
    },
    /// Retains inactive member identities; only the matching active member
    /// supplies a payload. Raw values confer no nominal/syntax authority.
    AnonymousSum {
        members: Vec<CtTy>,
        active: u32,
        payload: Box<CtValue>,
    },
}
/// Only scalar/text literals are raw constants. Type, Syntax, Ident, and query
/// descriptors must be produced by independently checked instructions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtConstant {
    Scalar(Constant),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtFunction {
    pub id: CtFunctionId,
    pub definition: CtSymbolId,
    pub parameters: Vec<CtLocalId>,
    pub parameter_types: Vec<CtTy>,
    pub result_type: CtTy,
    pub locals: Vec<CtTy>,
    pub values: Vec<CtTy>,
    pub blocks: Vec<CtBlock>,
    pub entry: CtBlockId,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtBlock {
    pub id: CtBlockId,
    pub instructions: Vec<CtInstruction>,
    pub terminator: CtTerminator,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtInstruction {
    pub result: Option<CtValueId>,
    pub kind: CtInstructionKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtTerminator {
    Return(Option<CtValueId>),
    Jump(CtBlockId),
    Branch {
        condition: CtValueId,
        then_block: CtBlockId,
        else_block: CtBlockId,
    },
    Trap,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtTypeConstructor {
    Array,
    Tuple,
    FunctionPointer,
    Address { mutable: bool },
    Slice,
    Managed,
    Desc,
    OwnedStore,
    ErrorUnion,
    AnonymousSum,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtSyntaxBuilder {
    Literal,
    TypeRef,
    NameExpr,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtNameBuilder {
    Fresh,
    Export,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtTextOp {
    Equal,
    Length,
    Slice,
    Concatenate,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtTokenProjection {
    RootCount,
    RootAt,
    Class,
    Text,
    Delimiter,
    ChildCount,
    ChildAt,
    InputName,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtMetaProjection {
    ShapeFields,
    ShapeComplete,
    FieldName,
    FieldType,
    FieldManaged,
    ImplTrait,
    ImplType,
    LayoutSize,
    LayoutAlignment,
    LayoutFieldOffsets,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtQueryKind {
    Shape,
    Field,
    ImplIndex,
    Layout,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtInstructionKind {
    Constant(CtConstantId),
    InvocationInput {
        index: u32,
    },
    StorageLive(CtLocalId),
    StorageDead(CtLocalId),
    Load(CtLocalId),
    Store {
        local: CtLocalId,
        value: CtValueId,
    },
    ConvertInteger {
        value: CtValueId,
        target: CtScalarTy,
    },
    ConvertNumber {
        value: CtValueId,
        target: CtScalarTy,
    },
    Unary {
        op: UnaryOp,
        operand: CtValueId,
    },
    Binary {
        op: BinaryOp,
        left: CtValueId,
        right: CtValueId,
    },
    MakeTuple(Vec<CtValueId>),
    MakeArray(Vec<CtValueId>),
    MakeList(Vec<CtValueId>),
    MakeRecord {
        schema: CtRecordSchemaId,
        fields: Vec<CtValueId>,
    },
    MakeVariant {
        schema: CtEnumSchemaId,
        tag: u32,
        fields: Vec<CtValueId>,
    },
    /// Exact-type injection after independent member canonicality validation.
    /// VariantTag/VariantField also consume this structural one-field sum;
    /// extraction must match the real active tag and field zero.
    MakeAnonymousSum {
        members: Vec<CtTy>,
        active: u32,
        payload: CtValueId,
    },
    Length(CtValueId),
    VariantTag(CtValueId),
    VariantField {
        value: CtValueId,
        variant: u32,
        field: u32,
    },
    Extract {
        aggregate: CtValueId,
        index: CtValueId,
    },
    ExtractField {
        aggregate: CtValueId,
        index: u32,
    },
    Replace {
        aggregate: CtValueId,
        index: CtValueId,
        value: CtValueId,
    },
    ReplaceField {
        aggregate: CtValueId,
        index: u32,
        value: CtValueId,
    },
    Call {
        function: CtFunctionId,
        arguments: Vec<CtValueId>,
    },
    TypePrimitive(CtScalarTy),
    TypeBuiltin(CtBuiltinType),
    TypeName(CtSymbolId),
    TypeApply {
        nominal: CtSymbolId,
        arguments: Vec<CtValueId>,
    },
    TypeConstruct {
        constructor: CtTypeConstructor,
        arguments: Vec<CtValueId>,
    },
    Quote {
        template: CtTemplateId,
        holes: Vec<CtValueId>,
    },
    Parse {
        category: CtSyntaxCategory,
        input: CtValueId,
    },
    ParseIdent {
        input: CtValueId,
    },
    TokenInspect {
        source: CtValueId,
        projection: CtTokenProjection,
        index: Option<CtValueId>,
    },
    Text {
        op: CtTextOp,
        arguments: Vec<CtValueId>,
    },
    SyntaxBuild {
        kind: CtSyntaxBuilder,
        arguments: Vec<CtValueId>,
    },
    NameBuild {
        kind: CtNameBuilder,
        text: CtValueId,
    },
    Query {
        kind: CtQueryKind,
        arguments: Vec<CtValueId>,
    },
    MetaProject {
        value: CtValueId,
        projection: CtMetaProjection,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtReadyStage {
    Tokens,
    Config,
    Shape,
    ImplIndex,
    Layout,
    Contract,
    VerifiedBody,
}
/// Snapshot metadata is a request, never authority. The verifier/driver bind
/// it to an immutable view and lexical definition-site visibility.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtSnapshotRequest {
    pub fingerprint: Fingerprint,
    pub ready: CtReadyStage,
    pub target_fingerprint: Option<Fingerprint>,
    pub requested_queries: Vec<CtQueryKind>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtQueryDependency {
    pub kind: CtQueryKind,
    pub subject_fingerprint: Fingerprint,
    pub result_fingerprint: Fingerprint,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CtDeclaredBudget {
    pub max_steps: u64,
    pub max_call_depth: u32,
    pub max_live_cells: u64,
    pub max_temporary_bytes: u64,
    pub max_generated_nodes: u64,
    pub max_instances: u64,
    pub max_analysis_work: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtUsageReceipt {
    pub steps: u64,
    pub peak_call_depth: u32,
    pub peak_live_cells: u64,
    pub peak_temporary_bytes: u64,
    pub generated_nodes: u64,
    pub instances: u64,
    pub analysis_work: u64,
    pub queries: Vec<CtQueryDependency>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtArtifactProgram {
    pub schema_version: u32,
    /// Selected target width, never the host VM pointer width.
    pub target_pointer_width: u16,
    pub source_fingerprint: Fingerprint,
    pub functions: Vec<CtFunction>,
    pub entry: CtFunctionId,
    pub constants: Vec<CtConstant>,
    pub record_schemas: Vec<CtRecordSchema>,
    pub enum_schemas: Vec<CtEnumSchema>,
    pub templates: Vec<CtTemplate>,
    pub input_tokens: CtInputTable,
    pub snapshot: CtSnapshotRequest,
    pub budget: CtDeclaredBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtProviderEntry {
    pub path: String,
    pub export: String,
    pub symbol: CtSymbolId,
    pub output: CtSyntaxCategory,
    pub delimiters: Vec<CtDelimiter>,
    pub implementation_fingerprint: Fingerprint,
    pub grammar_fingerprint: Fingerprint,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtLockedProviderRegistry {
    pub schema_version: u32,
    pub entries: Vec<CtProviderEntry>,
    pub fingerprint: Fingerprint,
}

// Encoding and bounded decoding are implemented by the CT artifact wire module;
// they do not construct a verified wrapper. Runtime Wire31 is a separate domain.
