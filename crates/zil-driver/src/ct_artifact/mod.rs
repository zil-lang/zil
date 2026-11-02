//! Project-owned compile-time artifact inputs. None of these values grants
//! verification authority; the verifier checks them against frozen source.
pub mod lock;
mod compiler;
mod source_index;
mod authority;

pub use lock::SyntaxLock;
