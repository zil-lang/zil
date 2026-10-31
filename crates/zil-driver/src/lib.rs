#![forbid(unsafe_code)]
//! Shared CLI/editor scheduling. The session reuses only raw phase snapshots;
//! each complete compilation obtains a fresh independent verifier capability.

mod artifacts;
mod contracts;
mod ct_program;
mod ct_selection;
mod project;
mod session;
pub use project::{ProjectSources, compile_project};
pub use session::CompilationSession;
pub use zil_db::CancellationToken;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
pub use zil_backend_llvm::{
    CodegenOptions, CodegenSummary, NativeBackend, Optimization, WindowsLinkOptions,
};
use zil_base::{
    CompileResult, Diagnostic, FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory,
    SourceUnit, TargetProfile, json_string,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    Parse,
    Core,
    Types,
    Analyze,
    Verify,
    Lower,
    Build,
}
impl Stage {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "parse" => Self::Parse,
            "core" | "expand" => Self::Core,
            "types" => Self::Types,
            "analyze" => Self::Analyze,
            "verify" => Self::Verify,
            "lower" => Self::Lower,
            "build" => Self::Build,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExecutionProfile {
    #[default]
    Native,
    Managed,
}
#[derive(Clone, Debug)]
pub struct CompileOptions {
    pub backend: NativeBackend,
    pub stop_after: Stage,
    pub emit_dir: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub entry: Option<String>,
    pub cache_dir: Option<PathBuf>,
    /// Trusted local machine-object cache; independent of source-candidate cache.
    pub object_cache: Option<PathBuf>,
    pub clang: Option<PathBuf>,
    pub optimization: zil_backend_llvm::Optimization,
    /// Explicit Linux build without generated ZIL symbol exports; default false.
    pub closed_executable: bool,
    pub codegen: CodegenOptions,
    pub windows: Option<zil_backend_llvm::WindowsLinkOptions>,
    pub target: TargetProfile,
    pub limits: Limits,
    pub cancellation: CancellationToken,
    /// Explicit non-source build inputs, such as a locked import/configuration
    /// manifest. The scalar driver never silently reads ambient configuration.
    pub build_inputs: Fingerprint,
    pub profile: ExecutionProfile,
}
impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            backend: NativeBackend::default(),
            stop_after: Stage::Verify,
            emit_dir: None,
            output: None,
            entry: None,
            cache_dir: None,
            object_cache: None,
            clang: None,
            optimization: zil_backend_llvm::Optimization::O0,
            closed_executable: false,
            codegen: CodegenOptions::default(),
            windows: None,
            target: TargetProfile::host_llvm(),
            limits: Limits::default(),
            cancellation: CancellationToken::default(),
            build_inputs: Fingerprint::ZERO,
            profile: ExecutionProfile::Native,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryDisposition {
    Executed,
    Reused,
}
impl QueryDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::Reused => "reused",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryEvent {
    pub phase: Phase,
    pub disposition: QueryDisposition,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiskCacheStatus {
    #[default]
    Disabled,
    Missing,
    Hit,
    Rebuilt,
}
impl DiskCacheStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Missing => "missing",
            Self::Hit => "hit",
            Self::Rebuilt => "rebuilt",
        }
    }
}
#[derive(Clone, Debug)]
pub struct CompileReport {
    pub status: ResultCategory,
    pub phase: Phase,
    pub diagnostics: Vec<Diagnostic>,
    pub artifacts: BTreeMap<String, PathBuf>,
    pub input_fingerprint: Fingerprint,
    pub seconds: f64,
    /// Compatibility summary; inspect query_events and disk_cache to determine
    /// what was reused. This flag never indicates a cached Verified object.
    pub cache_hit: bool,
    pub commands: Vec<Vec<String>>,
    pub native_codegen: Option<CodegenSummary>,
    pub disk_cache: DiskCacheStatus,
    pub query_events: Vec<QueryEvent>,
    pub verification_runs: usize,
    pub source_files: BTreeMap<FileId, String>,
    pub scalar_interfaces: Vec<zil_ir::ScalarInterface>,
}
impl CompileReport {
    fn new(source: &SourceUnit) -> Self {
        Self {
            status: ResultCategory::Success,
            phase: Phase::Parse,
            diagnostics: vec![],
            artifacts: BTreeMap::new(),
            input_fingerprint: source.fingerprint(),
            seconds: 0.0,
            cache_hit: false,
            commands: vec![],
            native_codegen: None,
            disk_cache: DiskCacheStatus::Disabled,
            query_events: vec![],
            verification_runs: 0,
            source_files: BTreeMap::from([(source.file_id, source.path.clone())]),
            scalar_interfaces: Vec::new(),
        }
    }
    pub fn to_json(&self) -> String {
        format!(
            "{{\"status\":{},\"phase\":{},\"diagnostics\":[{}],\"artifacts\":{{{}}},\"input_fingerprint\":{},\"seconds\":{},\"cache_hit\":{},\"commands\":[{}],\"native_codegen\":{},\"disk_cache\":{},\"query_events\":[{}],\"verification_runs\":{},\"source_files\":{{{}}},\"scalar_interfaces\":[{}]}}",
            json_string(self.status.as_str()),
            json_string(self.phase.as_str()),
            self.diagnostics
                .iter()
                .map(Diagnostic::to_json)
                .collect::<Vec<_>>()
                .join(","),
            self.artifacts
                .iter()
                .map(|(name, path)| format!(
                    "{}:{}",
                    json_string(name),
                    json_string(&path.to_string_lossy())
                ))
                .collect::<Vec<_>>()
                .join(","),
            json_string(&self.input_fingerprint.to_hex()),
            self.seconds,
            self.cache_hit,
            self.commands
                .iter()
                .map(|command| format!(
                    "[{}]",
                    command
                        .iter()
                        .map(|argument| json_string(argument))
                        .collect::<Vec<_>>()
                        .join(",")
                ))
                .collect::<Vec<_>>()
                .join(","),
            self.native_codegen.as_ref().map_or_else(|| "null".into(), CodegenSummary::to_json),
            json_string(self.disk_cache.as_str()),
            self.query_events
                .iter()
                .map(|event| format!(
                    "{{\"phase\":{},\"disposition\":{}}}",
                    json_string(event.phase.as_str()),
                    json_string(event.disposition.as_str())
                ))
                .collect::<Vec<_>>()
                .join(","),
            self.verification_runs,
            self.source_files.iter().map(|(id, path)| format!("{}:{}", json_string(&id.0.to_string()), json_string(path))).collect::<Vec<_>>().join(","),
            self.scalar_interfaces.iter().map(|interface| format!("{{\"module\":{},\"signature_fingerprint\":{},\"implementation_fingerprint\":{},\"contract_fingerprint\":{},\"result_origins\":{},\"callable_contracts\":{},\"callable_returns\":{},\"exports\":[{}]}}", interface.module.0, json_string(&interface.signature_fingerprint.to_hex()), json_string(&interface.implementation_fingerprint.to_hex()), interface.contract_fingerprint.map_or_else(|| "null".into(), |hash| json_string(&hash.to_hex())), interface.result_origins.as_ref().map_or_else(|| "null".into(), |origins| format!("[{}]", origins.iter().map(|origin| format!("{{\"name\":{},\"parameters\":{:?}}}", json_string(&origin.name), origin.parameters)).collect::<Vec<_>>().join(","))), contracts::callable_json(interface), contracts::callable_return_json(interface), interface.exports.iter().map(|f| json_string(&f.name)).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(","),
        )
    }
    fn fail(&mut self, diagnostics: Vec<Diagnostic>) {
        self.status = diagnostics
            .first()
            .map_or(ResultCategory::InfraError, |diagnostic| diagnostic.category);
        self.phase = diagnostics
            .first()
            .map_or(Phase::Driver, |diagnostic| diagnostic.phase);
        self.diagnostics = diagnostics;
    }
}
fn infra(code: &str, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::new(
        Phase::Driver,
        ResultCategory::InfraError,
        code,
        message,
        None,
    )]
}

/// A one-shot request has no persistent in-memory session. Editors and build
/// servers can explicitly keep a CompilationSession to reuse raw snapshots.
pub fn compile_source(source: &SourceUnit, options: &CompileOptions) -> CompileReport {
    CompilationSession::default().compile_source(source, options)
}
pub fn compile_path(path: &Path, options: &CompileOptions) -> CompileReport {
    CompilationSession::default().compile_path(path, options)
}
fn read_source(path: &Path, options: &CompileOptions) -> CompileResult<SourceUnit> {
    options
        .cancellation
        .check(Phase::Driver)
        .map_err(|error| vec![error])?;
    let file = File::open(path)
        .map_err(|error| infra("E-SOURCE-READ", format!("{}: {error}", path.display())))?;
    let limit = options.limits.max_source_bytes as u64;
    if file
        .metadata()
        .map_err(|error| infra("E-SOURCE-READ", error.to_string()))?
        .len()
        > limit
    {
        return Err(vec![Diagnostic::limit(
            Phase::Parse,
            "source byte budget exceeded",
            None,
        )]);
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| infra("E-SOURCE-READ", error.to_string()))?;
    if bytes.len() > options.limits.max_source_bytes {
        return Err(vec![Diagnostic::limit(
            Phase::Parse,
            "source byte budget exceeded",
            None,
        )]);
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        vec![Diagnostic::error(
            Phase::Lex,
            "E-UTF8",
            None,
            "source is not valid UTF-8",
        )]
    })?;
    Ok(SourceUnit::new(
        FileId(0),
        ModuleId(0),
        path.to_string_lossy(),
        text,
    ))
}
#[cfg(test)]
mod tests;
