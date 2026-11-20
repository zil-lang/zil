//! The single semantic rule kernel (`spec/semantic-kernel.md` §5).
//!
//! Holds the state domain, its order and join, the transfer of every
//! operation and edge, and exit obligations. It contains no fixpoint
//! scheduling, no diagnostic policy and cannot construct a verified module:
//! `zil-solve` iterates these rules, `zil-check` applies them once per
//! operation and edge to an untrusted certificate.
#![forbid(unsafe_code)]
#![deny(clippy::wildcard_enum_match_arm)]

mod paths;
mod state;
mod transfer;

pub use paths::{MovePaths, PathId, tracked_len};
pub use state::{BitSet, InitState, MustFacts, State};
pub use transfer::{Kernel, Successor, Violation, ViolationKind};

/// Reference to the base crate keeps spans available to violation
/// rendering without a second schema.
pub use zil_base::Span;
