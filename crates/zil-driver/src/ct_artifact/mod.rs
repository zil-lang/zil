//! Project-owned compile-time artifact inputs. None of these values grants
//! verification authority; the verifier checks them against frozen source.
mod authority;
mod compiler;
mod execute;
mod hygiene;
mod input;
pub mod lock;
mod materialize;
mod ordinary_type;
mod read_view;
mod render;
mod snapshot;
mod source_index;
mod type_ref;

pub use lock::SyntaxLock;
pub(crate) use materialize::Preview;
pub(super) use materialize::materialize;
