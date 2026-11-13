//! Untrusted lexical semantic operands. These records form a ghost program,
//! not certificates of provenance, capability, ownership or Source fidelity.
use crate::*;

/// Core side tables can refer to imported original declarations before the
/// program is unified. Stable cross-request identity is stored separately.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticDefinitionId {
    pub module: ModuleId,
    pub index: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SemanticInstanceId {
    pub module: ModuleId,
    pub index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticModule {
    pub definitions: Vec<SemanticDefinition>,
    pub instances: Vec<SemanticInstance>,
    pub sites: Vec<SemanticSite>,
    /// Exact original Dyn owners; lookup starts at the actual Dyn node,
    /// never at a bridge's asserted scope or a same-shaped declaration.
    pub dyn_scopes: Vec<SemanticDynScopeBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticDynScopeBinding {
    pub owner: SemanticDynOwner,
    pub scope: SemanticDefinitionId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticDynOwner {
    Declaration(LifeDeclarationId),
    MethodSignature {
        declaration: LifeDeclarationId,
        member: u32,
    },
    MethodDefinition(DynDefinitionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticDefinition {
    pub id: SemanticDefinitionId,
    pub identity: LifeDeclarationIdentity,
    pub parent: Option<SemanticDefinitionId>,
    pub kind: SemanticDefinitionKind,
    /// Complete original mixed order, including unused formals.
    pub parameters: Vec<SemanticFormal>,
    pub implicit_lifetimes: Vec<SemanticImplicitLifetime>,
    pub requirements: Vec<SemanticRequirementRecipe>,
    /// Real free semantic variables; these are never physical capture slots.
    pub environment: Vec<SemanticEnvironmentFormal>,
    pub body: SemanticDefinitionBody,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticDefinitionKind {
    Function,
    Method,
    Closure,
    TaskEntry,
    Nominal,
    Scope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticDefinitionBody {
    Function {
        signature: SemanticSignatureRecipe,
        sites: Vec<SemanticSiteRecipe>,
    },
    Nominal {
        declaration: LifeDeclarationId,
        kind: LifeNominalKind,
        body: SemanticNominalBody,
    },
    Scope(SemanticScopeRecipe),
    /// Complete original operations, including unused and unspecialized arms.
    FunctionWithBody {
        signature: SemanticSignatureRecipe,
        sites: Vec<SemanticSiteRecipe>,
        /// Original ordered value binders. Their stable identities are this
        /// definition plus ordinal, never names guessed from Typed locals.
        parameters: Vec<SemanticOriginalClosureParameter>,
        /// An actual source parameter, not a compiler-added closure receiver.
        receiver_parameter: Option<u32>,
        /// Explicit source-site to original Core node correspondence. Source
        /// occurrence paths are not reinterpreted after desugaring.
        site_nodes: Vec<SemanticOriginalSiteNode>,
        body: SemanticOriginalFunctionBody,
    },
}

// Share the Core operation grammar. Only annotation slots change; this is not
// a second operation language or a list of conclusions produced by Types.
pub type SemanticOriginalClosureParameter = CoreClosureParameter<SemanticTypeRecipe>;
pub type SemanticOriginalPattern = CorePattern<SemanticTypeRecipe>;
pub type SemanticOriginalPatternKind = CorePatternKind<SemanticTypeRecipe>;
pub type SemanticOriginalMatchArm = CoreMatchArm<SemanticTypeRecipe>;
pub type SemanticOriginalBlock = CoreBlock<SemanticTypeRecipe>;
pub type SemanticOriginalStatement = CoreStatement<SemanticTypeRecipe>;
pub type SemanticOriginalStatementKind = CoreStatementKind<SemanticTypeRecipe>;
pub type SemanticOriginalExpr = CoreExpr<SemanticTypeRecipe>;
pub type SemanticOriginalExprKind = CoreExprKind<SemanticTypeRecipe>;
pub type SemanticOriginalNamedPattern = (String, SemanticOriginalPattern);
pub type SemanticOriginalNamedExpr = (String, SemanticOriginalExpr);
pub type SemanticOriginalQuoteHole = CoreQuoteHole<SemanticTypeRecipe>;
pub type SemanticOriginalQuoteTemplate = CoreQuoteTemplate<SemanticTypeRecipe>;
pub type SemanticOriginalGenericArgument = CoreGenericArgument<SemanticTypeRecipe>;
pub type SemanticOriginalSyntaxCategory = CoreSyntaxCategory;
pub type SemanticOriginalSyntaxDelimiter = CoreSyntaxDelimiter;
pub type SemanticOriginalQuoteNameNamespace = CoreQuoteNameNamespace;
pub type SemanticOriginalQuoteNameRole = CoreQuoteNameRole;
pub type SemanticOriginalQuoteHoleKind = CoreQuoteHoleKind;
pub type SemanticOriginalQuoteNameSite = CoreQuoteNameSite;
pub type SemanticOriginalSyntaxTree = CoreSyntaxTree;
pub type SemanticOriginalSyntaxTreeEntry = CoreSyntaxTreeEntry;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticOriginalFunctionBody {
    Defined(SemanticOriginalBlock),
    /// Must correspond to the actual foreign declaration and complete ABI.
    External,
    /// Only an actual abstract trait member may omit an implementation body.
    Declaration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticOriginalTypeSource {
    Expression(NodeId),
    Binding(NodeId),
    Parameter(u32),
    Result,
    /// Carrier of an explicit const argument, after the same original call's
    /// target formals have been solved in the caller's original domain.
    CallConstCarrier {
        call: NodeId,
        formal: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticOriginalNode {
    Expression(NodeId),
    Statement(NodeId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticOriginalSiteNode {
    pub site: SemanticSiteId,
    pub node: SemanticOriginalNode,
}

/// Distinguish an absent implemented trait from an omitted required recipe.
/// Trait Self is an implicit abstract type, never mixed formal ordinal zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticScopeRecipe {
    Trait {
        declaration: LifeDeclarationId,
    },
    InherentImpl {
        declaration: LifeDeclarationId,
        self_type: SemanticTypeRecipe,
    },
    TraitImpl {
        declaration: LifeDeclarationId,
        self_type: SemanticTypeRecipe,
        trait_application: SemanticTraitRecipe,
    },
    /// Compiler-sealed identity, never a fabricated user nominal declaration.
    /// An impl still needs the actual permitted provider and original body.
    BuiltinImpl {
        self_type: SemanticTypeRecipe,
        trait_kind: SemanticBuiltinTrait,
    },
}

/// Original sealed names, not permissions or certificates. Alias spelling is
/// retained; consumers check applicability and any normalization themselves.
/// Callable bounds use their complete signature-bearing recipe instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticBuiltinTrait {
    Drop,
    Copy,
    Value,
    SyncDrop,
    StoreElement,
    Transfer,
    Share,
    ThreadCallable,
    ClosedEnvironment,
    Send,
    Sync,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticNominalBody {
    Product {
        fields: Vec<SemanticFieldRecipe>,
    },
    Enum {
        variants: Vec<SemanticVariantRecipe>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticFieldRecipe {
    pub name: String,
    pub ty: SemanticTypeRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticVariantRecipe {
    pub name: String,
    pub fields: Vec<SemanticTypeRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticFormal {
    Type {
        requirements: Vec<SemanticRequirementRecipe>,
    },
    Const {
        carrier: SemanticTypeRecipe,
    },
    Lifetime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticImplicitLifetime {
    pub ordinal: u32,
    /// Actual structural occurrence in the original definition's syntax.
    pub occurrence_path: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticEnvironmentFormal {
    /// Its carrier and requirements use the same real parent/creator domain
    /// as `source`. Copying a more distant formal requires explicit rebasing.
    pub kind: SemanticFormal,
    /// Evaluated in the real lexical parent/creator definition's domain.
    /// Ancestor(0) names that parent, not this child definition.
    pub source: SemanticActualRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticInstance {
    pub id: SemanticInstanceId,
    pub definition: SemanticDefinitionId,
    pub target: SemanticInstanceTarget,
    /// A partition target never substitutes for checking the original rigid
    /// body. Both targets remain real, independently checked typed programs.
    pub lifetime_instantiation: SemanticLifetimeInstantiation,
    pub parent: Option<SemanticInstanceId>,
    /// Original full formal order. A definition's lifetime actuals remain
    /// rigid; caller-specific substitution belongs to the actual Call site.
    pub actuals: Vec<SemanticActual>,
    pub header_embedding: Vec<SemanticHeaderBinding>,
    pub environment: Option<SemanticEnvironmentInstance>,
    /// Actual lexical Self, independently checked against the original scope
    /// and the real method/receiver. It is not an extra source formal.
    pub self_type: Option<SemanticTypeValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticLifetimeInstantiation {
    Rigid,
    Partition { rigid: SemanticInstanceId },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticFunctionTarget {
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
pub enum SemanticInstanceTarget {
    Function(SemanticFunctionTarget),
    /// A real lexical nominal/trait/impl instance without a runtime function.
    /// Its header embedding is empty; its lexical domain need not be empty.
    Scope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticClosureTarget {
    Core {
        module: ModuleId,
        creator: NodeId,
        expression: NodeId,
        occurrence: u32,
    },
    Typed(ClosureId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticHeaderBinding {
    pub header: StorageBinder,
    pub actual: SemanticClassExpr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticEnvironmentInstance {
    pub closure: SemanticClosureTarget,
    /// Evaluated at the real closure-creation site in the actual parent/creator
    /// instance, then closed over for the child. Parent SSA/local IDs never
    /// select the child's same-numbered operands. Every real creation must
    /// match; a first caller cannot authenticate the environment.
    pub actuals: Vec<SemanticActual>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticActual {
    Type(SemanticTypeValue),
    Const {
        ty: SemanticTypeValue,
        value: Constant,
    },
    Life(SemanticClassExpr),
    /// Core-only whole actual, including a const whose substituted carrier is
    /// not yet known. Types evaluates it in the exact source instance before
    /// emitting Typed IR; executable consumers reject every residual row.
    CoreRecipe {
        scope: SemanticInstanceId,
        recipe: Box<SemanticActualRecipe>,
    },
    /// Core-only inference obligation for one real original call/formal slot.
    /// The original caller body determines its solution, never claimed actuals.
    CoreInferred {
        scope: SemanticInstanceId,
        call: SemanticSiteId,
        formal: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticCoreTypeSource {
    /// A complete original recipe resolvable in this exact lexical instance
    /// without guessing any expression's inferred type or closure identity.
    OriginalRecipe(Box<SemanticTypeRecipe>),
    /// The actual Core emission occurrence supplies the inferred whole type.
    /// Resolving it also checks the recipe in `SemanticTypeValue::Core.scope`; the
    /// recipe is not permission to select a same-shaped expression or closure.
    Expression {
        module: ModuleId,
        function: NodeId,
        expression: NodeId,
        occurrence: u32,
        recipe: Box<SemanticTypeRecipe>,
    },
    /// The original body supplies the symbolic type; the separate actual Core
    /// occurrence supplies the concrete emission. Neither may replace the other.
    OriginalExpression {
        module: ModuleId,
        function: NodeId,
        expression: NodeId,
        occurrence: u32,
        source: SemanticOriginalTypeSource,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticTypeValue {
    Resolved {
        ty: Type,
        life: StorageLifeShape,
        /// Every intrinsic class, including phantom and inactive alternatives.
        classes: Vec<SemanticClassExpr>,
    },
    /// Core-only complete type operand, shared by actuals, Self and all site
    /// actions. Types must materialize all of these before emitting Typed IR.
    /// An absent Self or an empty class vector cannot stand in for this row.
    Core {
        scope: SemanticInstanceId,
        source: SemanticCoreTypeSource,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticScopeRef {
    /// Zero is self; otherwise follow this many real lexical parent links.
    Ancestor(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticClassExpr {
    Formal {
        scope: SemanticScopeRef,
        ordinal: u32,
    },
    Implicit {
        scope: SemanticScopeRef,
        ordinal: u32,
    },
    Bound {
        depth: u32,
        index: u32,
    },
    OuterOccurrence {
        operand: SemanticOperand,
        path: Vec<LifeTypeProjection>,
        mode: StoredBorrowMode,
    },
    IntrinsicClass {
        operand: SemanticOperand,
        path: Vec<LifeTypeProjection>,
        class: u32,
    },
    /// Read an already checked and sealed Life environment slot in the real
    /// lexical instance. A Life value is not a Type with an invented class 0.
    Environment {
        scope: SemanticScopeRef,
        ordinal: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticOperand {
    Parameter(u32),
    Capture(u32),
    Receiver,
    Value(ValueId),
    /// The actual initialized occupant at this CFG point, not its type alone.
    Local(LocalId),
    TypeArgument {
        scope: SemanticScopeRef,
        ordinal: u32,
    },
    Environment(u32),
    CoreExpression {
        expression: NodeId,
        occurrence: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticSiteRecipe {
    pub id: SemanticSiteId,
    pub occurrence_path: Vec<u32>,
    pub kind: SemanticSiteRecipeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticSiteRecipeKind {
    Construct {
        result: SemanticTypeRecipe,
    },
    Call {
        target: SemanticDefinitionId,
        actuals: Vec<SemanticActualRecipe>,
    },
    /// Original callable whole type, including an abstract F. The actual
    /// function/closure identity is obtained from the real callee operand,
    /// never invented by Source for an indirect call.
    CallableCall {
        callee: SemanticTypeRecipe,
    },
    TypeUse {
        expected: SemanticTypeRecipe,
    },
    /// Establish a local's declared class constraint without reading an
    /// uninitialized occupant. Every later Store must satisfy this constraint.
    DeclareLocal {
        expected: SemanticTypeRecipe,
    },
    Closure {
        definition: SemanticDefinitionId,
        environment: Vec<SemanticActualRecipe>,
    },
    /// The full original body resolves callee/member/builtin and written mixed
    /// arguments. Source must not run a second typeof pass to invent a target.
    OriginalCall,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticSite {
    pub instance: SemanticInstanceId,
    pub recipe: SemanticSiteId,
    pub at: SemanticSiteAddress,
    pub action: SemanticSiteAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticSiteAddress {
    Core {
        expression: NodeId,
        occurrence: u32,
        role: SemanticEmissionRole,
    },
    Typed {
        block: BlockId,
        /// An ordinal below len names the actual instruction. The sole
        /// len/Before boundary names the actual terminator and is legal only
        /// for ghost checks, never a constructor/call/closure opcode claim.
        instruction: u32,
        edge: SemanticSiteEdge,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticEmissionRole {
    Construct,
    Call,
    Ascription,
    Binding,
    Closure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticSiteEdge {
    Before,
    After,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticSiteAction {
    Construct {
        result: SemanticOperand,
        value: SemanticTypeValue,
    },
    Call {
        target: SemanticFunctionTarget,
        actuals: Vec<SemanticActual>,
    },
    CallableCall {
        callee: SemanticOperand,
    },
    TypeUse {
        subject: SemanticOperand,
        expected: SemanticTypeValue,
    },
    DeclareLocal {
        local: SemanticLocalTarget,
        expected: SemanticTypeValue,
    },
    Closure {
        result: SemanticOperand,
        closure: SemanticClosureTarget,
        environment: Vec<SemanticActual>,
    },
    /// Core-only pending call whose original scheme is recovered from its
    /// unique site_nodes entry. Not an empty-actuals or no-generic assertion.
    CoreBodyCall,
    /// All mixed actuals are materialized; the actual callee/receiver/entry is
    /// selected by the real instruction, and compared with original resolution.
    BodyCall {
        actuals: Vec<SemanticActual>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticLocalTarget {
    Core { statement: NodeId, occurrence: u32 },
    Typed(LocalId),
}

/// Recipe operands cannot select runtime SSA or a mutable local occupant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticRecipeOperand {
    TypeFormal(SemanticFormalRef),
    Environment {
        scope: SemanticScopeRef,
        ordinal: u32,
    },
    Parameter(u32),
    Receiver,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticFormalRef {
    pub scope: SemanticScopeRef,
    pub ordinal: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticLifeRecipe {
    Formal(SemanticFormalRef),
    Implicit {
        scope: SemanticScopeRef,
        ordinal: u32,
    },
    Environment {
        scope: SemanticScopeRef,
        ordinal: u32,
    },
    Bound {
        depth: u32,
        index: u32,
    },
    Invocation(u32),
    OuterOccurrence {
        operand: SemanticRecipeOperand,
        path: Vec<LifeTypeProjection>,
        mode: StoredBorrowMode,
    },
    IntrinsicClass {
        operand: SemanticRecipeOperand,
        path: Vec<LifeTypeProjection>,
        class: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticOuterLifeRecipe {
    Class(SemanticLifeRecipe),
    /// An actual constant-source obligation; never a binder or proof.
    Static,
}

/// Complete original type expressions. Atom is restricted to true atomic
/// leaves. Exact retains the full domain, and cannot erase a generic subtree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticTypeRecipe {
    Atom(Type),
    Exact(Box<SemanticExactTypeRecipe>),
    Formal(SemanticFormalRef),
    Environment {
        scope: SemanticScopeRef,
        ordinal: u32,
    },
    SelfType {
        scope: SemanticScopeRef,
    },
    Nominal {
        definition: SemanticDefinitionId,
        arguments: Vec<SemanticActualRecipe>,
    },
    AssociatedType {
        receiver: Box<SemanticTypeRecipe>,
        application: Box<SemanticTraitRecipe>,
        member: u32,
    },
    Tuple(Vec<SemanticTypeRecipe>),
    Array {
        element: Box<SemanticTypeRecipe>,
        length: Box<SemanticConstRecipe>,
    },
    Slice(Box<SemanticTypeRecipe>),
    Address {
        mutable: bool,
        lifetime: SemanticOuterLifeRecipe,
        inner: Box<SemanticTypeRecipe>,
    },
    Desc {
        lifetime: SemanticOuterLifeRecipe,
        inner: Box<SemanticTypeRecipe>,
    },
    Managed(Box<SemanticTypeRecipe>),
    OwnedStore(Box<SemanticTypeRecipe>),
    Mutex(Box<SemanticTypeRecipe>),
    MutexGuard {
        payload: Box<SemanticTypeRecipe>,
        retained: Box<SemanticRetainedShapeRecipe>,
    },
    Registration(Box<SemanticTypeRecipe>),
    AsyncRegistration(Box<SemanticTypeRecipe>),
    Task {
        result: Box<SemanticTypeRecipe>,
        retained: Box<SemanticRetainedShapeRecipe>,
    },
    ErrorUnion {
        error: Box<SemanticTypeRecipe>,
        result: Box<SemanticTypeRecipe>,
    },
    Qualified {
        storage: Box<SemanticTypeRecipe>,
        life: Box<SemanticStorageShapeRecipe>,
    },
    Code {
        kind: SemanticCodeKind,
        signature: Box<SemanticSignatureRecipe>,
    },
    Closure {
        definition: SemanticDefinitionId,
        arguments: Vec<SemanticActualRecipe>,
        environment: Vec<SemanticActualRecipe>,
    },
    Dyn {
        owned: bool,
        interface: Box<SemanticDynInterfaceRecipe>,
    },
    Opaque {
        definition: SemanticDefinitionId,
        arguments: Vec<SemanticActualRecipe>,
        hidden: Box<SemanticTypeRecipe>,
    },
    AnonymousSum(Vec<SemanticTypeRecipe>),
    /// Transport-only forms; executable semantic programs reject these.
    UnresolvedNamed(String),
    Unsupported(String),
    /// Reads a checked original body node in its real definition domain, never
    /// a runtime local occupant or SSA value in a same-numbered child scope.
    OriginalType {
        scope: SemanticScopeRef,
        source: SemanticOriginalTypeSource,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticExactTypeRecipe {
    pub ty: Type,
    pub life: StorageLifeShape,
    pub classes: Vec<SemanticLifeRecipe>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticCodeKind {
    FunctionPointer,
    BorrowedFunctionPointer,
    ForeignCallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticActualRecipe {
    Type(SemanticTypeRecipe),
    Const {
        carrier: SemanticTypeRecipe,
        value: SemanticConstRecipe,
    },
    Life(SemanticLifeRecipe),
    /// Legal only in an original Call's mixed actual vector. The target formal
    /// determines the kind/carrier. All other recipe positions must reject it.
    Infer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticClassSequenceRecipe {
    pub parts: Vec<SemanticClassSequencePart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticClassSequencePart {
    One(SemanticLifeRecipe),
    /// Expand the actual complete child domain; never deduplicate its images.
    AllClasses {
        ty: Box<SemanticTypeRecipe>,
        path: Vec<LifeTypeProjection>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticStorageShapeRecipe {
    pub domain: SemanticClassSequenceRecipe,
    pub slots: Vec<SemanticSlotRecipe>,
    pub retained: Vec<SemanticRetainedPart>,
    pub nominal: Vec<SemanticNominalPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticSlotRecipe {
    pub path: Vec<PlaceProjection>,
    pub mode: StoredBorrowMode,
    pub class: SemanticLifeRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticRetainedPart {
    One {
        path: Vec<LifeTypeProjection>,
        child_class: u32,
        parent: SemanticLifeRecipe,
    },
    All {
        path: Vec<LifeTypeProjection>,
        parents: SemanticClassSequenceRecipe,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticNominalPart {
    One {
        path: Vec<LifeTypeProjection>,
        declaration: LifeDeclarationId,
        argument: u32,
        class: SemanticLifeRecipe,
    },
    FromType {
        path: Vec<LifeTypeProjection>,
        ty: Box<SemanticTypeRecipe>,
        classes: SemanticClassSequenceRecipe,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticRetainedShapeRecipe {
    pub domain: SemanticClassSequenceRecipe,
    pub ports: Vec<SemanticPortPart>,
    pub retained: Vec<SemanticRetainedPart>,
    pub nominal: Vec<SemanticNominalPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticPortPart {
    One {
        leaf: Box<SemanticTypeRecipe>,
        mode: StoredBorrowMode,
        class: SemanticLifeRecipe,
    },
    FromType {
        ty: Box<SemanticTypeRecipe>,
        classes: SemanticClassSequenceRecipe,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticConstRecipe {
    Value(Constant),
    IntegerLiteral(String),
    FloatLiteral(String),
    Formal(SemanticFormalRef),
    Environment {
        scope: SemanticScopeRef,
        ordinal: u32,
    },
    Unary {
        op: UnaryOp,
        operand_type: Box<SemanticTypeRecipe>,
        value: Box<SemanticConstRecipe>,
    },
    Binary {
        op: BinaryOp,
        operand_type: Box<SemanticTypeRecipe>,
        left: Box<SemanticConstRecipe>,
        right: Box<SemanticConstRecipe>,
    },
    Convert {
        source_type: Box<SemanticTypeRecipe>,
        target_type: Box<SemanticTypeRecipe>,
        value: Box<SemanticConstRecipe>,
    },
    AssociatedConst {
        receiver: Box<SemanticTypeRecipe>,
        application: Box<SemanticTraitRecipe>,
        member: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticTraitRecipe {
    pub declaration: LifeDeclarationId,
    pub arguments: Vec<SemanticActualRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticRequirementRecipe {
    Structural {
        ty: SemanticTypeRecipe,
        bound: DynStructuralBound,
    },
    Trait {
        ty: SemanticTypeRecipe,
        application: SemanticTraitRecipe,
    },
    Callable {
        ty: SemanticTypeRecipe,
        kind: CallableKind,
        signature: Box<SemanticSignatureRecipe>,
    },
    TypeEqual {
        left: SemanticTypeRecipe,
        right: SemanticTypeRecipe,
    },
    ConstEqual {
        carrier: SemanticTypeRecipe,
        left: SemanticConstRecipe,
        right: SemanticConstRecipe,
    },
    LifeEqual {
        left: SemanticLifeRecipe,
        right: SemanticLifeRecipe,
    },
    Outlives {
        longer: SemanticLifeRecipe,
        shorter: SemanticLifeRecipe,
    },
    ForAll {
        lifetimes: u32,
        requirements: Vec<SemanticRequirementRecipe>,
    },
    /// A complete sealed predicate obligation in the original lexical world.
    Builtin {
        ty: SemanticTypeRecipe,
        trait_kind: SemanticBuiltinTrait,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticSignatureRecipe {
    pub bound_lifetimes: u32,
    pub invocation_lifetimes: u32,
    pub parameters: Vec<SemanticParameterRecipe>,
    pub result: SemanticTypeRecipe,
    pub result_mode: PassingMode,
    pub call_kind: CallKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticParameterRecipe {
    pub ty: SemanticTypeRecipe,
    pub mode: PassingMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticDynContextRecipe {
    Closed,
    Type(Box<SemanticTypeRecipe>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticDynInterfaceRecipe {
    pub application: SemanticTraitRecipe,
    pub context: SemanticDynContextRecipe,
    pub associated: Vec<SemanticAssociatedBindingRecipe>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticAssociatedBindingRecipe {
    pub application: SemanticTraitRecipe,
    pub member: u32,
    pub value: SemanticAssociatedRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticAssociatedRecipe {
    Type(SemanticTypeRecipe),
    Const {
        carrier: SemanticTypeRecipe,
        value: SemanticConstRecipe,
    },
}
