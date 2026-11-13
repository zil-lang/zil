//! Unverified, source-shaped syntax. No node in this module grants a capability.
//! `Span` always refers to the original UTF-8 byte range. Sugar is preserved here;
//! zil-expand, not the parser, is responsible for explicit core temporaries.

use zil_base::{ModuleId, Span};

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub id: ModuleId,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub kind: ItemKind,
    pub attributes: Vec<Attribute>,
    pub public: bool,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    Import(Import),
    Function(Function),
    Aggregate(Aggregate),
    Enum(EnumDecl),
    Trait(TraitDecl),
    Impl(ImplBlock),
    TypeAlias(TypeAlias),
    Const(ConstDecl),
    Extern { abi: String, function: Function },
    Meta(MetaItem),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub path: Vec<String>,
    pub alias: Option<String>,
    pub syntax: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: Option<TypeRef>,
    pub where_clause: Vec<BoundClause>,
    pub contract: Vec<ContractClause>,
    pub body: FunctionBody,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FunctionBody {
    Block(Block),
    Expression(Expr),
    Declaration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    pub give: bool,
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenericParam {
    pub name: String,
    pub kind: GenericParamKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenericParamKind {
    Type(Vec<TypeRef>),
    Lifetime,
    Const(TypeRef),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundClause {
    pub subject: BoundSubject,
    pub bounds: Vec<TypeRef>,
    pub lifetime_bound: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundSubject {
    Type(TypeRef),
    Lifetime(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeRef {
    pub kind: TypeKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeKind {
    Named {
        path: Vec<String>,
        args: Vec<GenericArg>,
    },
    Reference {
        lifetime: Option<String>,
        mutable: bool,
        inner: Box<TypeRef>,
    },
    Tuple(Vec<TypeRef>),
    Group(Box<TypeRef>),
    Array {
        element: Box<TypeRef>,
        length: Option<Box<Expr>>,
    },
    Function {
        params: Vec<TypeRef>,
        result: Box<TypeRef>,
    },
    ForeignFunction {
        abi: String,
        params: Vec<TypeRef>,
        result: Box<TypeRef>,
    },
    Optional(Box<TypeRef>),
    Error {
        error: Box<TypeRef>,
        result: Box<TypeRef>,
    },
    Union(Vec<TypeRef>),
    Dyn {
        path: Vec<String>,
        args: Vec<GenericArg>,
        /// Explicit closed binding/finite slot list; never inferred from uses.
        bindings: Vec<DynBinding>,
    },
    ForLifetime {
        lifetime: String,
        inner: Box<TypeRef>,
    },
    Impl(Vec<TypeRef>),
    MetaEval(Box<Expr>),
    MetaSplice(Box<Expr>),
    MetaBlock(Block),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DynBinding {
    pub kind: DynBindingKind,
    pub span: Span,
}
#[derive(Clone, Debug, PartialEq)]
pub enum DynBindingKind {
    Type {
        member: Vec<String>,
        value: TypeRef,
    },
    Constant {
        member: Vec<String>,
        ty: TypeRef,
        value: Expr,
    },
    Method {
        member: Vec<String>,
        arguments: Vec<GenericArg>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenericArg {
    /// Explicit lifetime actual, resolved against its lexical declaration.
    Lifetime(String),
    Type(TypeRef),
    Const(Expr),
    /// A trait-application premise, never an ordinary Type/Const/Life actual.
    /// Only type equations and explicitly typed constant equations are legal.
    AssociatedBinding(DynBinding),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggregateKind {
    Struct,
    Resource,
    Package,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Aggregate {
    pub kind: AggregateKind,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<Field>,
    pub methods: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: TypeRef,
    pub default: Option<Expr>,
    pub public: bool,
    pub manages: bool,
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: VariantFields,
    pub attributes: Vec<Attribute>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VariantFields {
    Unit,
    Tuple(Vec<VariantTupleField>),
    Record(Vec<Field>),
}

/// Explicit ownership belongs to this enum field, not to a general type name.
#[derive(Clone, Debug, PartialEq)]
pub struct VariantTupleField {
    pub ty: TypeRef,
    pub manages: bool,
    pub span: Span,
}
impl std::ops::Deref for VariantTupleField {
    type Target = TypeRef;
    fn deref(&self) -> &TypeRef {
        &self.ty
    }
}
impl std::ops::DerefMut for VariantTupleField {
    fn deref_mut(&mut self) -> &mut TypeRef {
        &mut self.ty
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraitDecl {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub bounds: Vec<TypeRef>,
    pub members: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImplBlock {
    pub generics: Vec<GenericParam>,
    pub trait_type: Option<TypeRef>,
    pub target: TypeRef,
    pub receiver_default: Option<Param>,
    pub where_clause: Vec<BoundClause>,
    pub members: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeAlias {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub bounds: Vec<TypeRef>,
    pub value: Option<TypeRef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstDecl {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub value: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContractClause {
    pub name: String,
    pub terms: Vec<ContractTerm>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContractTerm {
    pub path: Vec<String>,
    pub type_args: Vec<GenericArg>,
    pub arguments: Vec<ContractTerm>,
    pub index: Option<Box<ContractTerm>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub path: Vec<String>,
    pub arguments: Vec<Expr>,
    pub string_value: Option<String>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
    /// Request-local production token; absent in parsed/original syntax.
    pub semantic: SemanticContextMarker,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Binding {
        mutable: bool,
        pattern: Pattern,
        ty: Option<TypeRef>,
        value: Expr,
    },
    Return(Option<Expr>),
    Yield(Expr),
    Break,
    Continue,
    Defer(Block),
    If {
        condition: Condition,
        then_body: Block,
        else_body: Option<ElseBranch>,
    },
    While {
        condition: Condition,
        body: Block,
    },
    For {
        pattern: Pattern,
        iterable: Expr,
        body: Block,
    },
    Loop(Block),
    Match {
        value: Expr,
        arms: Vec<MatchArm>,
    },
    With {
        name: String,
        value: Expr,
        body: Block,
    },
    Unsafe(Block),
    Assign {
        target: Expr,
        op: AssignOp,
        value: Expr,
    },
    Expression(Expr),
    Meta(MetaStatement),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Condition {
    Expression(Expr),
    Let { pattern: Pattern, value: Expr },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ElseBranch {
    Block(Block),
    If(Box<Stmt>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: ArrowBody,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    BitAnd,
    BitOr,
    BitXor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatternKind {
    Name(String),
    Wildcard,
    Literal(Literal),
    Reference {
        mutable: bool,
        pattern: Box<Pattern>,
    },
    /// Explicit `.ok(pattern)` / `.err(pattern)` ErrorUnion tag selection.
    /// Ordinary type, active-tag, place and ownership checks still apply.
    ErrorArm {
        is_error: bool,
        pattern: Box<Pattern>,
    },
    Tuple(Vec<Pattern>),
    Constructor {
        path: Vec<String>,
        arguments: Vec<Pattern>,
    },
    RecordConstructor {
        path: Vec<String>,
        fields: Vec<RecordPatternField>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordPatternField {
    pub name: String,
    pub pattern: Pattern,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    /// A linear owner/role event bundle consumed at the real Core emission.
    /// This transport token is never a semantic or Source-fidelity proof.
    pub semantic: SemanticContextMarker,
}

/// General expression/declaration production address. Syntax equality ignores
/// request-local transport, so every consumer must check the raw ledger slot.
#[derive(Clone, Copy, Debug)]
pub struct SemanticContextMarker(u32);
impl Default for SemanticContextMarker {
    fn default() -> Self {
        Self(u32::MAX)
    }
}
impl SemanticContextMarker {
    pub fn untrusted(index: u32) -> Self {
        Self(index)
    }
    pub fn raw_index(self) -> Option<u32> {
        (self.0 != u32::MAX).then_some(self.0)
    }
}
impl PartialEq for SemanticContextMarker {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// A request-local relocation reference, not source syntax or a semantic claim.
/// Only a matching elaboration ledger may consume it. Incoming raw references
/// must be cleared and rebuilt; cloning is not a second context production.
/// Syntax equality deliberately ignores this transport detail. Consumers must
/// compare `raw_index()` and validate the actual ledger, never compare markers.
#[derive(Clone, Copy, Debug)]
pub struct ClosureContextMarker(u32);
impl Default for ClosureContextMarker {
    fn default() -> Self {
        Self(u32::MAX)
    }
}
impl ClosureContextMarker {
    /// MAX is the absent sentinel, never a valid ledger slot. Interpreting an
    /// absent marker grants no authority; all actual contexts are reproduced.
    pub fn untrusted(index: u32) -> Self {
        Self(index)
    }
    pub fn raw_index(self) -> Option<u32> {
        (self.0 != u32::MAX).then_some(self.0)
    }
}
impl PartialEq for ClosureContextMarker {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Literal(Literal),
    Name(String),
    ReceiverMember(String),
    Group(Box<Expr>),
    /// Generated value type context. Source parsing does not create this form;
    /// ordinary expansion and semantic kernels still check the full candidate.
    Ascribed {
        ty: TypeRef,
        value: Box<Expr>,
    },
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Pipeline {
        input: Box<Expr>,
        target: Box<Expr>,
    },
    Member {
        object: Box<Expr>,
        member: MemberName,
    },
    TypeApply {
        callee: Box<Expr>,
        arguments: Vec<GenericArg>,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Argument>,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        object: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Try(Box<Expr>),
    Record {
        ty: Option<Box<Expr>>,
        base: Option<Box<Expr>>,
        fields: Vec<RecordField>,
    },
    Closure {
        context: ClosureContextMarker,
        move_capture: bool,
        params: Vec<ClosureParam>,
        return_type: Option<TypeRef>,
        body: ArrowBody,
    },
    If {
        condition: Box<Expr>,
        then_value: Box<Expr>,
        else_value: Box<Expr>,
    },
    MetaEval(Box<Expr>),
    MetaSplice(Box<Expr>),
    Quote {
        category: SyntaxCategory,
        template: QuoteTemplate,
    },
    MetaBlock(Block),
    SyntaxIsland {
        path: Vec<String>,
        tree: TokenTree,
    },
}

/// CT syntax categories are not ordinary runtime types or verification tags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxCategory {
    Expr,
    Type,
    StmtList,
    ItemList,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteNameNamespace {
    Value,
    Type,
    Member,
    Module,
    Label,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteNameRole {
    Binding,
    Reference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteHoleKind {
    Expr,
    Type,
    StmtList,
    ItemList,
    Ident {
        namespace: QuoteNameNamespace,
        role: QuoteNameRole,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuoteHole {
    pub kind: QuoteHoleKind,
    pub value: Box<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuoteTemplate {
    pub tree: QuoteTokenTree,
    pub holes: Vec<QuoteHole>,
    /// Literal identifier tokens only, excluding this layer's hole expressions.
    /// These are exact source sites, not a scope or binding proof.
    pub name_sites: Vec<QuoteNameSite>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuoteNameSite {
    pub span: Span,
    pub token_index: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuoteTokenTree {
    pub delimiter: Delimiter,
    pub children: Vec<QuoteTokenTreeEntry>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QuoteTokenTreeEntry {
    Token {
        text: String,
        span: Span,
    },
    Tree(QuoteTokenTree),
    /// Index into this quote layer's QuoteTemplate::holes.
    Hole {
        index: u32,
        span: Span,
    },
}

/// Literal spelling is retained verbatim, including quotes and escape sequences.
/// Numeric interpretation and the expected type belong to zil-types.
#[derive(Clone, Debug, PartialEq)]
pub enum Literal {
    Integer(String),
    Float(String),
    String(String),
    Char(String),
    Bool(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Negate,
    BitNot,
    Dereference,
    Borrow,
    BorrowMut,
    Give,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Multiply,
    Divide,
    Remainder,
    MultiplyWrapping,
    Add,
    Subtract,
    AddWrapping,
    SubtractWrapping,
    ShiftLeft,
    ShiftRight,
    BitAnd,
    BitXor,
    BitOr,
    Range,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MemberName {
    Name(String),
    /// Materialized reflected member, never constructed by source parsing.
    /// The owner remains an ordinary untrusted type constraint.
    Constrained {
        name: String,
        owner: Box<TypeRef>,
    },
    TupleIndex(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Argument {
    pub spread: bool,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordField {
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClosureParam {
    pub name: String,
    pub ty: Option<TypeRef>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArrowBody {
    Expression(Box<Expr>),
    Block(Block),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TokenTree {
    pub delimiter: Delimiter,
    pub children: Vec<TokenTreeEntry>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delimiter {
    Parenthesis,
    Bracket,
    Brace,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenTreeEntry {
    Token { text: String, span: Span },
    Tree(TokenTree),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MetaItem {
    If {
        condition: Expr,
        then_items: Vec<Item>,
        else_items: Vec<Item>,
    },
    For {
        binding: String,
        iterable: Expr,
        items: Vec<Item>,
    },
    Emit(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MetaStatement {
    If {
        condition: Expr,
        then_body: Block,
        else_body: Option<Block>,
    },
    For {
        binding: String,
        iterable: Expr,
        body: Block,
    },
    Emit(Expr),
}
