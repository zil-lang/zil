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

/// A bound-local Fixed class selected from the real creator header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCallableReturnFixed {
    pub fixed: u32,
    pub input: CoreClosureInputAnchor,
}

/// Untrusted original declaration bridge for an opaque callable result.
/// Fixed indices are local to the source bound; Types must resolve each actual
/// header selector and map it into the complete inferred environment domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreCallableReturnLifeContext {
    pub declaration: LifeDeclarationId,
    pub identity: LifeDeclarationIdentity,
    /// Complete original mixed formal order, including unused parameters.
    pub parameters: Vec<LifeParameterKind>,
    pub fixed: Vec<CoreCallableReturnFixed>,
}

/// Structural work reservation only. This function checks sizes and paths;
/// it neither resolves declarations nor validates lifetime permissions.
pub fn storage_relation_metadata_work(
    life: &StorageLifeContract,
    max_nodes: usize,
    max_depth: usize,
) -> Option<usize> {
    let rows = life.bindings.len().checked_add(life.classes.len())?;
    if rows > max_nodes
        || life.fixed_binder_count as usize > max_nodes
        || life.invocation_binder_count as usize > max_nodes
    {
        return None;
    }
    let mut work = rows.checked_mul(8)?.checked_add(4)?;
    work = work.checked_add(life.fixed_binder_count as usize)?;
    work = work.checked_add(life.invocation_binder_count as usize)?;
    for depth in life
        .bindings
        .iter()
        .map(|r| r.position.path.len())
        .chain(life.classes.iter().map(|r| r.position.path.len()))
    {
        if depth > max_depth {
            return None;
        }
        work = work.checked_add(depth.checked_mul(2)?)?;
    }
    Some(work)
}

/// Size reservation for new return-bound metadata before cloning. Const
/// carriers are deliberately not traversed here: each caller must run its own
/// bounded type visitor over every `LifeParameterKind::Const` before copying.
/// No declaration, binder, selector or permission is accepted by this walk.
pub fn callable_return_metadata_work(
    bound: &CoreCallableReturn,
    max_nodes: usize,
    max_depth: usize,
) -> Option<usize> {
    let mut work = 0usize;
    if let Some(life) = &bound.life {
        work = storage_relation_metadata_work(life, max_nodes, max_depth)?;
    }
    if let Some(context) = &bound.source_life {
        for count in [
            context.parameters.len(),
            context.fixed.len(),
            context.identity.module_path.len(),
            context.identity.declaration_path.len(),
            context.identity.generation_path.len(),
        ] {
            if count > max_nodes {
                return None;
            }
            work = work.checked_add(count.checked_mul(8)?.checked_add(1)?)?;
        }
        work = work
            .checked_add(context.identity.package.len())?
            .checked_add(8)?;
        for text in context
            .identity
            .module_path
            .iter()
            .chain(&context.identity.declaration_path)
        {
            work = work.checked_add(text.len())?.checked_add(1)?;
        }
        for row in &context.fixed {
            let depth = match &row.input.position {
                CoreClosureHeaderPosition::Borrow(position) => position.path.len(),
                CoreClosureHeaderPosition::Class(position) => position.path.len(),
            };
            if depth > max_depth {
                return None;
            }
            work = work.checked_add(depth.checked_mul(2)?)?;
        }
    }
    Some(work)
}
