#![forbid(unsafe_code)]

//! Native LLVM backend. Emission accepts a sealed verifier result and its exact
//! LIR, including finite Copy/address/resource closure environments. It never interprets
//! source or delegates ZIL safety to a host
//! language compiler. `clang` receives LLVM IR directly.
//!
//! A closure is its named LLVM struct value. Capture order is field order; Unit
//! captures occupy an `i8` field containing zero, and other fields use `i1`,
//! `i32`, `i64`, or `ptr`. A borrowed scalar capture stores its real local stack
//! address; each capture read loads through it. Address-taken Unit locals have
//! one-byte storage, while Unit value reads remain erased. Empty environments
//! are empty structs. Copy Shared environments pass the hidden struct by value;
//! owning Shared, Exclusive and Consume entries receive an actual environment
//! pointer. Managed fields have appended live flags; local carrier flags and
//! fixed entry-block Consume scratch preserve transfer and normal cleanup.
//! Ordinary Unit parameters and results
//! retain the scalar ABI's erasure (`void` return); a Unit capture read likewise
//! has no runtime scalar value. Closure locals use ordinary aggregate storage,
//! aligned to the largest capture field (one byte for an empty environment).
//! No closure allocation, shared box, indirect dispatch table, or source-body
//! substitution participates in this ABI.

mod build;
mod emit;

use zil_ir::FunctionId;

pub use build::{BuildArtifact, BuildOptions, Optimization, WindowsLinkOptions, build};
pub use emit::{emit, emit_with_options};

/// Wrappers are explicit. `ReturnValue` is a test/CLI harness which calls the
/// selected ZIL function and prints its full signed result; a shell exit code is
/// never substituted for that result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EntryPoint {
    #[default]
    None,
    Main,
    ReturnValue(FunctionId),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmitOptions {
    pub entry: EntryPoint,
}

#[cfg(test)]
mod tests;
