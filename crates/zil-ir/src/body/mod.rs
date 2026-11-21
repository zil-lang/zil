//! Body IR: the single operation schema consumed by the semantic kernel,
//! the solver, the certificate checker and the machine lowering.
//!
//! Contract: `spec/semantic-kernel.md` §3–§4. Constructing any value in this
//! module grants no safety authority; only the checker may wrap a body as
//! verified.
//!
//! Every enumeration that describes an operation, a place step or a type
//! constructor is matched exhaustively by its consumers. The module denies
//! wildcard arms so that adding a variant is a compile error at every
//! consumer inside `zil-ir`; downstream crates carry the same lint.
#![deny(clippy::wildcard_enum_match_arm)]
#![warn(
    clippy::result_large_err,
    clippy::deref_addrof,
    clippy::large_enum_variant,
    clippy::manual_is_multiple_of
)]

mod cert;
mod ids;
mod inst;
mod opspec;
mod place;
mod source_map;
mod ty;
mod wire;

pub use cert::*;
pub use ids::*;
pub use inst::*;
pub use opspec::*;
pub use place::*;
pub use source_map::*;
pub use ty::*;
pub use wire::*;

#[cfg(test)]
mod tests;
