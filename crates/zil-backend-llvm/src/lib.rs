#![forbid(unsafe_code)]

//! Native LLVM 23 backend consuming `zil_check::VerifiedModule` directly.
//!
//! Only a sealed `VerifiedModule` constructed by `zil_check::check_module`
//! enters this crate. There is no legacy LIR path.

mod body_build;
mod body_emit;
mod body_structured;

pub use body_build::{
    BodyBuildError, BodyBuildOptions, BodyBuildOutput, build_verified_body, default_clang_path,
};
pub use body_emit::{
    BodyEmitError, emit_verified_body, emit_verified_body_for_target, emit_verified_body_units,
    emit_verified_body_units_for_target,
};
pub use body_structured::{StructuredBuildError, build_structured_module};
pub use zil_llvm_api::FunctionMerging;

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Optimization {
    #[default]
    O0,
    O2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsLinkOptions {
    pub linker: PathBuf,
    /// Explicit import archives. No ambient SDK search or automatic C host.
    pub import_libraries: Vec<PathBuf>,
}

impl Default for WindowsLinkOptions {
    fn default() -> Self {
        Self {
            linker: "lld-link".into(),
            import_libraries: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NativeBackend {
    #[default]
    Clang,
    LlvmApi,
}

impl NativeBackend {
    pub fn name(self) -> &'static str {
        match self {
            Self::Clang => "clang",
            Self::LlvmApi => "llvm-api",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodegenOptions {
    pub units: usize,
    pub jobs: usize,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        Self { units: 1, jobs: 1 }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodegenSummary {
    pub backend: NativeBackend,
    pub object_reports: Vec<String>,
    pub requested_units: usize,
    pub actual_units: usize,
    pub jobs: usize,
    pub fallback: Option<String>,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub cache_evicted_entries: usize,
    pub cache_evicted_bytes: u64,
    pub cache_evict_ns: u64,
    pub emit_ns: u64,
    pub link_ns: u64,
}

impl CodegenSummary {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"backend\":{},\"object_reports\":[{}],\"requested_units\":{},\"actual_units\":{},\"jobs\":{},\"fallback\":{},\"cache_hits\":{},\"cache_misses\":{},\"cache_evicted_entries\":{},\"cache_evicted_bytes\":{},\"cache_evict_ns\":{},\"emit_ns\":{},\"link_ns\":{}}}",
            zil_base::json_string(self.backend.name()),
            self.object_reports.join(","),
            self.requested_units,
            self.actual_units,
            self.jobs,
            self.fallback
                .as_ref()
                .map_or_else(|| "null".into(), |s| zil_base::json_string(s)),
            self.cache_hits,
            self.cache_misses,
            self.cache_evicted_entries,
            self.cache_evicted_bytes,
            self.cache_evict_ns,
            self.emit_ns,
            self.link_ns,
        )
    }
}
