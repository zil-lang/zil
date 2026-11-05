//! Untrusted Source -> Core context for the finite original free-function
//! life-only Definition profile. Types resolves this against the actual
//! creator header and actual inferred captures, then jointly canonicalizes
//! the existing Typed StorageLifeContract. No source names survive as runtime
//! permissions. Instantiated, generated and member scopes remain unsupported.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreClosureHeaderPosition {
    Borrow(StoragePosition),
    Class(SignatureLifeClass),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreClosureNamedFixed {
    /// Fixed ordinal in the proposed source Parameter/Result relation.
    pub fixed: u32,
    /// Full original generic ordinal, never a filtered life-only index.
    pub parameter: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreClosureInputAnchor {
    /// Real runtime parameter ordinal in the creator; not a capture slot.
    pub parameter: u32,
    /// Full original generic ordinal for named lives. Anonymous/elided input
    /// classes have no invented source generic parameter.
    pub source_parameter: Option<u32>,
    /// Must match the actual creator's checked header relation at position.
    pub header_binder: StorageBinder,
    pub position: CoreClosureHeaderPosition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreClosureLifeContext {
    pub creator: NodeId,
    pub expression: NodeId,
    /// Original source declaration reference, not a nominal type declaration.
    pub declaration: LifeDeclarationId,
    pub identity: LifeDeclarationIdentity,
    /// Complete original generic order. Current consumers require Lifetime
    /// at every position, and reject all other profiles before using a row.
    pub parameters: Vec<LifeParameterKind>,
    /// Actual elaboration event paired with this expression by the request's
    /// consumed ledger, not a Span or a guessed parallel traversal index.
    pub occurrence: u32,
    /// Source Parameter/Result proposal only. Types must infer every actual
    /// Capture and canonicalize the joint relation, never copy this as a full
    /// Typed closure contract or treat its ordinals as resolved capture slots.
    pub signature: StorageLifeContract,
    pub named_fixed: Vec<CoreClosureNamedFixed>,
    pub inputs: Vec<CoreClosureInputAnchor>,
}

/// Untrusted original source bridge for one selected shared Fn helper bound.
/// Its owner is the actual containing CoreFunction, and bound Parameter/Result
/// rows live in the containing CoreCallableParameter.life (never Capture rows).
/// A source producer must preserve this before erasing lexical life names;
/// Types must validate actual header selectors and the concrete environment
/// before jointly forming the existing complete Typed header/role contracts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCallableLifeContext {
    pub declaration: LifeDeclarationId,
    pub identity: LifeDeclarationIdentity,
    /// Complete original declaration order, including the callable Type slot.
    pub parameters: Vec<LifeParameterKind>,
    /// Full ordinal of F, distinct from the role's filtered generic_index.
    pub generic_parameter: u32,
    /// Selected bound position, including intervening noncallable bounds.
    pub bound_ordinal: u32,
    /// Bound Fixed ordinal -> full original source life parameter ordinal.
    pub named_fixed: Vec<CoreClosureNamedFixed>,
    /// Actual helper data-header selectors; no guessed capture slots.
    pub inputs: Vec<CoreClosureInputAnchor>,
}
