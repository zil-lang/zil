//! Untrusted lexical semantic operands. These records form a ghost program,
//! not certificates of provenance, capability, ownership or Source fidelity.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticModule {
    pub definitions: Vec<SemanticDefinition>,
    pub instances: Vec<SemanticInstance>,
    pub sites: Vec<SemanticSite>,
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
    pub signature: SemanticSignatureRecipe,
    pub requirements: Vec<SemanticRequirementRecipe>,
    pub sites: Vec<SemanticSiteRecipe>,
    /// Real free semantic variables; these are never physical capture slots.
    pub environment: Vec<SemanticEnvironmentFormal>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticDefinitionKind {
    Function,
    Method,
    Closure,
    TaskEntry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticFormal {
    Type { requirements: Vec<SemanticRequirementRecipe> },
    Const { carrier: SemanticTypeRecipe },
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
    pub kind: SemanticFormal,
    pub source: SemanticActualRecipe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticInstance {
    pub id: SemanticInstanceId,
    pub definition: SemanticDefinitionId,
    pub function: SemanticFunctionTarget,
    pub parent: Option<SemanticInstanceId>,
    /// Original full formal order. A definition's lifetime actuals remain
    /// rigid; caller-specific substitution belongs to the actual Call site.
    pub actuals: Vec<SemanticActual>,
    pub header_embedding: Vec<SemanticHeaderBinding>,
    pub environment: Option<SemanticEnvironmentInstance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticFunctionTarget {
    Core { module: ModuleId, function: NodeId },
    CoreClosure {
        module: ModuleId,
        creator: NodeId,
        expression: NodeId,
        occurrence: u32,
    },
    Typed(FunctionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticClosureTarget {
    Core { creator: NodeId, expression: NodeId, occurrence: u32 },
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
    pub actuals: Vec<SemanticActual>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticActual {
    Type(SemanticTypeValue),
    Const { ty: SemanticTypeValue, value: Constant },
    Life(SemanticClassExpr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticTypeValue {
    pub ty: Type,
    pub life: StorageLifeShape,
    /// Every intrinsic class, including phantom and inactive alternatives.
    pub classes: Vec<SemanticClassExpr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticScopeRef {
    /// Zero is self; otherwise follow this many real lexical parent links.
    Ancestor(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticClassExpr {
    Formal { scope: SemanticScopeRef, ordinal: u32 },
    Implicit { scope: SemanticScopeRef, ordinal: u32 },
    Bound { depth: u32, index: u32 },
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticOperand {
    Parameter(u32),
    Capture(u32),
    Receiver,
    Value(ValueId),
    /// The actual initialized occupant at this CFG point, not its type alone.
    Local(LocalId),
    TypeArgument { scope: SemanticScopeRef, ordinal: u32 },
    Environment(u32),
    CoreExpression { expression: NodeId, occurrence: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticSiteRecipe {
    pub id: SemanticSiteId,
    pub occurrence_path: Vec<u32>,
    pub kind: SemanticSiteRecipeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticSiteRecipeKind {
    Construct { result: SemanticTypeRecipe },
    Call { target: SemanticDefinitionId, actuals: Vec<SemanticActualRecipe> },
    TypeUse { expected: SemanticTypeRecipe },
    Closure { definition: SemanticDefinitionId, environment: Vec<SemanticActualRecipe> },
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
    Core { expression: NodeId, occurrence: u32, role: SemanticEmissionRole },
    Typed { block: BlockId, instruction: u32, edge: SemanticSiteEdge },
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
    Construct { result: SemanticOperand, value: SemanticTypeValue },
    Call { target: SemanticFunctionTarget, actuals: Vec<SemanticActual> },
    TypeUse { subject: SemanticOperand, expected: SemanticTypeValue },
    Closure {
        result: SemanticOperand,
        closure: SemanticClosureTarget,
        environment: Vec<SemanticActual>,
    },
}
