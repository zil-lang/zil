//! Types-owned consumers of the untrusted original semantic program.
//! Transport/name resolution is distinct from class and executable proofs.
pub(super) mod abi;
pub(super) mod creator_headers;
pub(super) mod emission;
pub(super) mod finish;
pub(super) mod frozen_context;
pub(super) mod ghost_capture;
pub(super) mod headers;
pub(super) mod instantiate;
pub(super) mod lower;
pub(super) mod materialize;
pub(super) mod nominal_catalog;
pub(super) mod original;
pub(super) mod original_context;
pub(super) mod original_nodes;
pub(super) mod original_walk;
pub(super) mod proof;
pub(super) mod rewrite;
pub(super) mod shared_body;
#[cfg(test)]
mod source_probe;
pub(super) mod well_formed;
pub(super) mod world;

/// Attribute a failure to the innermost available source operation. Raw IR and
/// normalized original bodies can carry the default empty location.
fn locate_original(mut diagnostic: zil_base::Diagnostic, span: zil_base::Span) -> zil_base::Diagnostic {
    if diagnostic.span.is_none() || diagnostic.span == Some(zil_base::Span::default()) {
        diagnostic.span = (span.start < span.end).then_some(span);
    }
    diagnostic
}
