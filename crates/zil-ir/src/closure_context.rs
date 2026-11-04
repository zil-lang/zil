//! Untrusted source identity and relocation data for captured life contexts.
//! These records do not establish a loan, class equality, trait or call right.
//! Every runtime Fixed fact still comes from the actual MakeClosure operands.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClosureLifeScopeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopedClosureLife {
    pub scope: ClosureLifeScopeId,
    /// Full original generic ordinal, including every Type/Const position.
    pub parameter: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopedClosureTypeClass {
    pub path: Vec<LifeTypeProjection>,
    pub class: u32,
    pub source: ScopedClosureLife,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopedClosureType {
    pub ty: Type,
    /// Actual ambient substitution of all free classes in the Type argument.
    /// Equal local Type bytes do not identify the enclosing class instances.
    pub classes: Vec<ScopedClosureTypeClass>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosureScopeActual {
    Type(ScopedClosureType),
    Const(Constant),
    Lifetime(ScopedClosureLife),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosureScopeInstance {
    Definition,
    /// Complete original order. Definition is not Instantiated([]).
    Instantiated(Vec<ClosureScopeActual>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureLifeScope {
    /// Original source declaration reference; not a nominal type declaration.
    pub declaration: LifeDeclarationId,
    pub identity: LifeDeclarationIdentity,
    pub parent: Option<ClosureLifeScopeId>,
    pub binder_site: Vec<u32>,
    pub parameters: Vec<LifeParameterKind>,
    pub instance: ClosureScopeInstance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureSourceContext {
    pub scopes: Vec<ClosureLifeScope>,
    pub scope: ClosureLifeScopeId,
    /// Original actual closure occurrence within the real declaration lineage.
    /// This is not a diagnostic Span or an index guessed from a second walk.
    pub site: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosureHeaderPosition {
    Borrow(StoragePosition),
    Class(SignatureLifeClass),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureHeaderLifeBinding {
    pub source: ScopedClosureLife,
    pub position: ClosureHeaderPosition,
    /// Must match the real creator's complete checked header relation.
    pub binder: StorageBinder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureNamedFixed {
    pub fixed: u32,
    pub source: ScopedClosureLife,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosurePatternPosition {
    Tuple(u32),
    Reference,
    Variant(u32),
    ErrorArm { is_error: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreClosureCaptureSource {
    Parameter(u32),
    Binding {
        statement: NodeId,
        position: Vec<ClosurePatternPosition>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClosureCaptureLifePosition {
    OuterAddress,
    Class {
        path: Vec<LifeTypeProjection>,
        class: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreClosureCaptureAnchor {
    pub source: ScopedClosureLife,
    pub capture: CoreClosureCaptureSource,
    pub position: ClosureCaptureLifePosition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreClosureLifeContext {
    pub creator: NodeId,
    pub expression: NodeId,
    pub context: ClosureSourceContext,
    pub header_bindings: Vec<ClosureHeaderLifeBinding>,
    /// A source Parameter/Result proposal. Types must add every actual inferred
    /// Capture and canonicalize the joint relation; this is not a full Typed
    /// closure contract and its numbers are not already resolved capture slots.
    pub signature: StorageLifeContract,
    pub named_fixed: Vec<ClosureNamedFixed>,
    pub capture_anchors: Vec<CoreClosureCaptureAnchor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosureLifeContextIdentity {
    pub closure: ClosureId,
    pub creator: FunctionId,
    /// Actual MakeClosure result in creator, not the global ClosureId alone.
    pub creation: ValueId,
    pub context: ClosureSourceContext,
    pub header_bindings: Vec<ClosureHeaderLifeBinding>,
    /// Indices refer to the final jointly canonicalized Typed Fixed domain.
    /// Anonymous captured borrows have no invented original generic ordinal.
    pub named_fixed: Vec<ClosureNamedFixed>,
}
