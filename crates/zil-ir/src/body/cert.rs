//! Certificate schema. Produced by the solver, always untrusted.
//!
//! Claims are stated over IR places, not over any kernel-internal numbering,
//! so the checker builds its own tracking table and rejects unknown places.
//! A place the certificate does not mention is read as the weakest state;
//! that is sound, never more permissive.

use super::ids::{BlockId, FnId, Local};
use super::place::Place;
use super::ty::{Mutability, Region};

/// Initialization claim of one place at a block entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InitClaim {
    Init,
    Uninit,
    /// Initialized on some paths only. Any use requiring either state fails.
    Maybe,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceInit {
    pub place: Place,
    pub state: InitClaim,
}

/// A live address window claimed at a block entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoanClaim {
    pub place: Place,
    pub mutability: Mutability,
    pub region: Region,
}

/// The active variant (Enum) or arm (error union: 1 = success) of a place
/// is known on every path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantClaim {
    pub place: Place,
    pub variant: u32,
}

/// `local` holds the tag of `place` and neither changed since it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagClaim {
    pub local: Local,
    pub place: Place,
}

/// Entry invariant of one block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockInvariant {
    pub block: BlockId,
    pub init: Vec<PlaceInit>,
    pub variants: Vec<VariantClaim>,
    pub tags: Vec<TagClaim>,
    pub loans: Vec<LoanClaim>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionCertificate {
    pub function: FnId,
    /// By default join points and loop heads; any block may be listed.
    /// Blocks without an invariant derive their state from their unique
    /// predecessor; a join block without one is a missing claim.
    pub invariants: Vec<BlockInvariant>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Certificate {
    pub functions: Vec<FunctionCertificate>,
}
