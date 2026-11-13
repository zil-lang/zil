//! Types-owned consumers of the untrusted original semantic program.
//! Transport/name resolution is distinct from class and executable proofs.
pub(super) mod emission;
pub(super) mod finish;
pub(super) mod headers;
pub(super) mod lower;
pub(super) mod materialize;
pub(super) mod proof;
pub(super) mod rewrite;
#[cfg(test)]
mod source_probe;
pub(super) mod well_formed;
