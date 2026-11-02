//! Project-owned compile-time artifact inputs. None of these values grants
//! verification authority; the verifier checks them against frozen source.
pub mod lock;
mod compiler;
mod source_index;
mod authority;
mod snapshot;
mod type_ref;
mod read_view;
mod render;
mod input;
mod execute;
mod ordinary_type;

pub use lock::SyntaxLock;
