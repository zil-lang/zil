//! Project-owned compile-time artifact inputs. None of these values grants
//! verification authority; the verifier checks them against frozen source.
mod authority;
mod compiler;
mod contextual_key;
mod dynamic_catalogue;
pub use dynamic_catalogue::from_core as reify_frozen_core_dynamic_metadata;
mod execute;
mod hygiene;
mod input;
mod invocation_context;
pub mod lock;
mod materialize;
mod ordinary_type;
mod read_view;
mod render;
mod scope_metadata;
mod snapshot;
mod source_index;
mod stage_inventory;
mod type_ref;

pub use lock::SyntaxLock;
pub(crate) use materialize::Preview;
pub(super) use materialize::materialize_scoped;

#[cfg(test)]
#[path = "compiler_runtime_type_actual_tests.rs"]
mod runtime_type_actual_tests;
