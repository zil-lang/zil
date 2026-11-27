#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]
//! Single-pipeline ZIL compiler driver.
//!
//! Every source compilation runs through the sole verified pipeline:
//! `Syntax -> ct_original -> Typeck -> SourceFidelity -> Solve -> Check (VerifiedModule) -> Backend-LLVM`.
//! Session caching reuses only unverified parse snapshots; every verification
//! obtains a fresh `VerifiedModule` from `zil-check`.

pub mod body_pipeline;
#[allow(dead_code)]
mod project_manifest;
mod semantic_cache;

pub use body_pipeline::{BodyCompileError, build_body_source_native, verify_body_program};
pub use project_manifest::ProjectManifest;
pub use zil_backend_llvm::{
    CodegenOptions, CodegenSummary, FunctionMerging, NativeBackend, Optimization,
    WindowsLinkOptions,
};
pub use zil_db::CancellationToken;

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use zil_backend_llvm::{BodyBuildOptions, build_verified_body};
use zil_base::{
    CompileResult, Diagnostic, FileId, Fingerprint, Limits, ModuleId, Phase, ResultCategory,
    SourceUnit, Span, TargetProfile, json_string,
};
use zil_check::{CheckBudget, check_fidelity, check_module};
use zil_solve::{Budget as SolveBudget, CertCacheKey, solve_module, solve_module_with_cache};
use zil_syntax::parse_with_limits;
use zil_typeck::{Limits as TypeckLimits, ModuleInput, lower_program};

#[allow(dead_code)]
pub(crate) mod project {
    use super::*;

    pub(crate) fn valid_segment(s: &str) -> bool {
        let mut chars = s.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first.is_ascii_alphabetic() || first == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    pub(crate) fn resolve_file_module(
        root: &Path,
        logical: &[String],
        span: Span,
    ) -> CompileResult<PathBuf> {
        if logical.is_empty() || logical.iter().any(|seg| !valid_segment(seg)) {
            return Err(vec![Diagnostic::error(
                Phase::Driver,
                "E-IMPORT-PATH",
                Some(span),
                "invalid module import path",
            )]);
        }
        let mut path = root.to_path_buf();
        for seg in logical {
            path.push(seg);
        }
        let file = path.with_extension("zil");
        if file.is_file() {
            return Ok(file);
        }
        let mod_file = path.join("mod.zil");
        if mod_file.is_file() {
            return Ok(mod_file);
        }
        Err(vec![Diagnostic::error(
            Phase::Driver,
            "E-IMPORT-NOT-FOUND",
            Some(span),
            format!(
                "module `{}` not found under {}",
                logical.join("."),
                root.display()
            ),
        )])
    }
}

/// Explicit module import destinations keyed by the source module that imports.
pub type ScopedImports = BTreeMap<(ModuleId, Vec<String>), ModuleId>;

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

/// Locked syntax-provider registry placeholder (parsed & validated on demand).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyntaxLock {
    pub raw: String,
}

impl SyntaxLock {
    pub fn parse(text: &str) -> Result<Self, Vec<Diagnostic>> {
        if text.trim().is_empty() {
            return Err(vec![Diagnostic::error(
                Phase::Driver,
                "E-SYNTAX-LOCK",
                None,
                "syntax lock file is empty",
            )]);
        }
        Ok(Self {
            raw: text.to_string(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ProjectSources {
    pub entry: ModuleId,
    pub sources: Vec<SourceUnit>,
    pub paths: BTreeMap<Vec<String>, ModuleId>,
}

#[derive(Clone, Debug)]
pub struct CompileOptions {
    pub backend: NativeBackend,
    pub stop_after: Stage,
    pub emit_dir: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub entry: Option<String>,
    pub test_harness: bool,
    pub project: Option<Arc<ProjectManifest>>,
    #[doc(hidden)]
    pub scoped_imports: Option<Arc<ScopedImports>>,
    pub cache_dir: Option<PathBuf>,
    pub semantic_cache: Option<PathBuf>,
    pub object_cache: Option<PathBuf>,
    /// Best-effort soft byte budget for `object_cache` (`None` = unlimited; default 32 MiB).
    /// Active objects required by the current build are never evicted, so cache size may exceed
    /// the budget when the active working set itself is larger than the budget.
    pub object_cache_max_bytes: Option<u64>,
    pub clang: Option<PathBuf>,
    pub extra_c_sources: Vec<PathBuf>,
    pub optimization: Optimization,
    pub closed_executable: bool,
    pub incremental_o2: bool,
    pub function_merging: FunctionMerging,
    pub codegen: CodegenOptions,
    pub windows: Option<WindowsLinkOptions>,
    pub target: TargetProfile,
    pub limits: Limits,
    pub max_expansion_bytes: usize,
    pub cancellation: CancellationToken,
    pub build_inputs: Fingerprint,
    pub syntax_lock: Option<SyntaxLock>,
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
            test_harness: false,
            project: None,
            scoped_imports: None,
            cache_dir: None,
            semantic_cache: None,
            object_cache: None,
            object_cache_max_bytes: Some(32 * 1024 * 1024),
            clang: None,
            extra_c_sources: Vec::new(),
            optimization: Optimization::O0,
            closed_executable: false,
            incremental_o2: false,
            function_merging: FunctionMerging::Disabled,
            codegen: CodegenOptions::default(),
            windows: None,
            target: TargetProfile::host_llvm(),
            limits: Limits::default(),
            max_expansion_bytes: Limits::default().max_source_bytes,
            cancellation: CancellationToken::default(),
            build_inputs: Fingerprint::ZERO,
            syntax_lock: None,
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
    pub cache_hit: bool,
    pub commands: Vec<Vec<String>>,
    pub native_codegen: Option<CodegenSummary>,
    pub disk_cache: DiskCacheStatus,
    pub query_events: Vec<QueryEvent>,
    pub verification_runs: usize,
    pub source_files: BTreeMap<FileId, String>,
}

impl CompileReport {
    fn new(source: &SourceUnit) -> Self {
        Self {
            status: ResultCategory::Success,
            phase: Phase::Parse,
            diagnostics: Vec::new(),
            artifacts: BTreeMap::new(),
            input_fingerprint: source.fingerprint(),
            seconds: 0.0,
            cache_hit: false,
            commands: Vec::new(),
            native_codegen: None,
            disk_cache: DiskCacheStatus::Disabled,
            query_events: Vec::new(),
            verification_runs: 0,
            source_files: BTreeMap::from([(source.file_id, source.path.clone())]),
        }
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"status\":{},\"phase\":{},\"diagnostics\":[{}],\"artifacts\":{{{}}},\"input_fingerprint\":{},\"seconds\":{},\"cache_hit\":{},\"commands\":[{}],\"native_codegen\":{},\"disk_cache\":{},\"query_events\":[{}],\"verification_runs\":{},\"source_files\":{{{}}}}}",
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
                        .map(|arg| json_string(arg))
                        .collect::<Vec<_>>()
                        .join(",")
                ))
                .collect::<Vec<_>>()
                .join(","),
            self.native_codegen
                .as_ref()
                .map_or_else(|| "null".into(), CodegenSummary::to_json),
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
            self.source_files
                .iter()
                .map(|(file, path)| format!(
                    "{}:{}",
                    json_string(&file.0.to_string()),
                    json_string(path)
                ))
                .collect::<Vec<_>>()
                .join(","),
        )
    }
}

struct SemanticSnapshot {
    key: Fingerprint,
    module: zil_ir::body::Module,
    source_map: zil_ir::body::SourceMap,
    cert: zil_ir::body::Certificate,
}

/// Bounded in-memory session caching only unverified parse snapshots and
/// untrusted semantic/certificate candidates. Verification always executes
/// fresh through `zil-check` (`verification_runs == 1`).
pub struct CompilationSession {
    capacity: usize,
    parse_cache: HashMap<(Fingerprint, usize), Result<zil_syntax::ast::Module, Vec<Diagnostic>>>,
    parse_order: VecDeque<(Fingerprint, usize)>,
    semantic_snapshots: VecDeque<SemanticSnapshot>,
    fn_cert_cache: RwLock<HashMap<CertCacheKey, zil_ir::body::FunctionCertificate>>,
}

impl Default for CompilationSession {
    fn default() -> Self {
        Self::with_capacity(32)
    }
}

impl CompilationSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            parse_cache: HashMap::new(),
            parse_order: VecDeque::new(),
            semantic_snapshots: VecDeque::new(),
            fn_cert_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Ephemeral one-shot session (`capacity = 0`) that skips retaining parsed
    /// ASTs and in-RAM semantic snapshots after compilation returns.
    fn ephemeral() -> Self {
        Self {
            capacity: 0,
            parse_cache: HashMap::new(),
            parse_order: VecDeque::new(),
            semantic_snapshots: VecDeque::new(),
            fn_cert_cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn clear(&mut self) {
        self.parse_cache.clear();
        self.parse_order.clear();
        self.semantic_snapshots.clear();
        if let Ok(mut guard) = self.fn_cert_cache.write() {
            guard.clear();
        }
    }

    pub fn cached_query_count(&self) -> usize {
        self.parse_cache.len()
    }

    pub fn active_query_count(&self) -> usize {
        0
    }

    fn insert_parse_cache(
        &mut self,
        key: (Fingerprint, usize),
        value: Result<zil_syntax::ast::Module, Vec<Diagnostic>>,
    ) {
        if self.capacity == 0 {
            return;
        }
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.parse_cache.entry(key) {
            let _ = e.insert(value);
            return;
        }
        while self.parse_cache.len() >= self.capacity {
            if let Some(oldest) = self.parse_order.pop_front() {
                self.parse_cache.remove(&oldest);
            } else {
                self.parse_cache.clear();
                break;
            }
        }
        self.parse_cache.insert(key, value);
        self.parse_order.push_back(key);
    }

    fn store_semantic_snapshot(
        &mut self,
        key: Fingerprint,
        module: zil_ir::body::Module,
        source_map: zil_ir::body::SourceMap,
        cert: zil_ir::body::Certificate,
    ) {
        self.semantic_snapshots.retain(|s| s.key != key);
        while self.semantic_snapshots.len() >= 4 {
            self.semantic_snapshots.pop_front();
        }
        self.semantic_snapshots.push_back(SemanticSnapshot {
            key,
            module,
            source_map,
            cert,
        });
    }

    pub fn compile_path(&mut self, path: &Path, options: &CompileOptions) -> CompileReport {
        let entry = match read_source_unit(path, FileId(0), ModuleId(0), &options.limits) {
            Ok(source) => source,
            Err(diag) => {
                return CompileReport {
                    status: diag.category,
                    phase: diag.phase,
                    diagnostics: vec![diag],
                    artifacts: BTreeMap::new(),
                    input_fingerprint: Fingerprint::ZERO,
                    seconds: 0.0,
                    cache_hit: false,
                    commands: Vec::new(),
                    native_codegen: None,
                    disk_cache: DiskCacheStatus::Disabled,
                    query_events: Vec::new(),
                    verification_runs: 0,
                    source_files: BTreeMap::from([(
                        FileId(0),
                        path.to_string_lossy().into_owned(),
                    )]),
                };
            }
        };
        let entry_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        self.compile_units_with_observer(&entry, Some(&entry_dir), options, &mut |_| {})
    }

    pub fn compile_source_with_observer(
        &mut self,
        source: &SourceUnit,
        options: &CompileOptions,
        observer: &mut dyn FnMut(&QueryEvent),
    ) -> CompileReport {
        self.compile_units_with_observer(source, None, options, observer)
    }

    fn parse_cached(
        &mut self,
        source: &SourceUnit,
        options: &CompileOptions,
        report: &mut CompileReport,
        observer: &mut dyn FnMut(&QueryEvent),
    ) -> Result<zil_syntax::ast::Module, Vec<Diagnostic>> {
        let cache_key = (source.fingerprint(), options.limits.max_tokens);
        if let Some(cached) = self.parse_cache.get(&cache_key) {
            let ev = QueryEvent {
                phase: Phase::Parse,
                disposition: QueryDisposition::Reused,
            };
            observer(&ev);
            report.query_events.push(ev);
            report.cache_hit = true;
            cached.clone()
        } else {
            let parsed = parse_with_limits(source, &options.limits).into_result();
            let ev = QueryEvent {
                phase: Phase::Parse,
                disposition: QueryDisposition::Executed,
            };
            observer(&ev);
            report.query_events.push(ev);
            if self.capacity > 0 {
                self.insert_parse_cache(cache_key, parsed.clone());
            }
            parsed
        }
    }

    fn compile_units_with_observer(
        &mut self,
        source: &SourceUnit,
        search_root: Option<&Path>,
        options: &CompileOptions,
        observer: &mut dyn FnMut(&QueryEvent),
    ) -> CompileReport {
        let start = Instant::now();
        let mut report = CompileReport::new(source);

        if options.profile == ExecutionProfile::Managed {
            report.status = ResultCategory::Unsupported;
            report.phase = Phase::Driver;
            report.diagnostics.push(Diagnostic::unsupported(
                Phase::Driver,
                "managed execution profile",
                None,
            ));
            report.seconds = start.elapsed().as_secs_f64();
            return report;
        }

        if options.cancellation.is_cancelled() {
            report.status = ResultCategory::Cancelled;
            report.phase = Phase::Driver;
            report.diagnostics.push(Diagnostic::new(
                Phase::Driver,
                ResultCategory::Cancelled,
                "E-CANCELLED",
                "compilation cancelled",
                None,
            ));
            report.seconds = start.elapsed().as_secs_f64();
            return report;
        }

        if source.text.len() > options.limits.max_source_bytes {
            report.status = ResultCategory::ResourceLimit;
            report.phase = Phase::Parse;
            report.diagnostics.push(Diagnostic::limit(
                Phase::Parse,
                "source exceeds max_source_bytes limit",
                None,
            ));
            report.seconds = start.elapsed().as_secs_f64();
            return report;
        }

        let entry_ast = match self.parse_cached(source, options, &mut report, observer) {
            Ok(ast) => ast,
            Err(diags) => {
                report.phase = Phase::Parse;
                report.status = highest_category(&diags);
                report.diagnostics = diags;
                report.seconds = start.elapsed().as_secs_f64();
                return report;
            }
        };

        if options.stop_after <= Stage::Parse {
            report.phase = Phase::Parse;
            report.seconds = start.elapsed().as_secs_f64();
            return report;
        }

        if options.cancellation.is_cancelled() {
            report.status = ResultCategory::Cancelled;
            report.phase = Phase::Types;
            report.seconds = start.elapsed().as_secs_f64();
            return report;
        }

        // Recursively discover imported modules when compiling from a filesystem path.
        struct LoadedMod {
            logical_path: String,
            fingerprint: Fingerprint,
            ast: zil_syntax::ast::Module,
            imports: Vec<(String, zil_check::ct_original::CtOriginalImportTarget)>,
        }
        let mut loaded: Vec<LoadedMod> = vec![LoadedMod {
            logical_path: String::new(),
            fingerprint: source.fingerprint(),
            ast: entry_ast,
            imports: Vec::new(),
        }];
        let mut by_logical: HashMap<String, ModuleId> = HashMap::new();
        by_logical.insert(String::new(), ModuleId(0));

        enum PendingSource {
            Embedded(SourceUnit),
            File {
                path: PathBuf,
                file_id: FileId,
                module_id: ModuleId,
            },
        }
        struct PendingLoad {
            logical: String,
            source: PendingSource,
        }

        let parse_workers = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .clamp(1, 16);

        let mut frontier_start = 0usize;
        while frontier_start < loaded.len() {
            let frontier_end = loaded.len();
            let mut pending: Vec<PendingLoad> = Vec::new();
            for idx in frontier_start..frontier_end {
                let raw_imports: Vec<(Vec<String>, String, Span)> = loaded[idx]
                    .ast
                    .items
                    .iter()
                    .filter_map(|item| {
                        if let zil_syntax::ast::ItemKind::Import(imp) = &item.kind {
                            let alias = imp
                                .alias
                                .clone()
                                .or_else(|| imp.path.last().cloned())
                                .unwrap_or_default();
                            Some((imp.path.clone(), alias, item.span))
                        } else {
                            None
                        }
                    })
                    .collect();
                let mut edges = Vec::with_capacity(raw_imports.len());
                for (imp_path, alias, imp_span) in raw_imports {
                    if imp_path.len() == 2 && imp_path[0] == "std" && imp_path[1] == "meta" {
                        edges.push((alias, zil_check::ct_original::CtOriginalImportTarget::Meta));
                        continue;
                    }
                    if imp_path.len() == 2 && imp_path[0] == "std" && imp_path[1] == "syntax" {
                        edges.push((
                            alias,
                            zil_check::ct_original::CtOriginalImportTarget::Syntax,
                        ));
                        continue;
                    }
                    let logical = imp_path.join(".");
                    if let Some(&mid) = by_logical.get(&logical) {
                        edges.push((
                            alias,
                            zil_check::ct_original::CtOriginalImportTarget::Module(mid),
                        ));
                        continue;
                    }
                    let next_id = (loaded.len() + pending.len()) as u32;
                    let pending_src = if imp_path.first().map(String::as_str) == Some("std") {
                        if let Some((virtual_path, text)) =
                            embedded_std_source(&imp_path[1..], &options.target)
                        {
                            if text.len() > options.limits.max_source_bytes {
                                report.phase = Phase::Parse;
                                report.status = ResultCategory::ResourceLimit;
                                report.diagnostics = vec![Diagnostic::limit(
                                    Phase::Parse,
                                    "source exceeds max_source_bytes limit",
                                    None,
                                )];
                                report.seconds = start.elapsed().as_secs_f64();
                                return report;
                            }
                            PendingSource::Embedded(SourceUnit::new(
                                FileId(next_id),
                                ModuleId(next_id),
                                virtual_path,
                                text.to_string(),
                            ))
                        } else {
                            // Leave unresolved for typeck to report a clean E-NAME-UNRESOLVED.
                            continue;
                        }
                    } else if let Some(root_dir) = search_root {
                        let Some(mod_file) =
                            project::resolve_file_module(root_dir, &imp_path, imp_span).ok()
                        else {
                            // Leave unresolved for typeck to report a clean E-NAME-UNRESOLVED.
                            continue;
                        };
                        PendingSource::File {
                            path: mod_file,
                            file_id: FileId(next_id),
                            module_id: ModuleId(next_id),
                        }
                    } else {
                        continue;
                    };
                    let mid = ModuleId(next_id);
                    by_logical.insert(logical.clone(), mid);
                    pending.push(PendingLoad {
                        logical,
                        source: pending_src,
                    });
                    edges.push((
                        alias,
                        zil_check::ct_original::CtOriginalImportTarget::Module(mid),
                    ));
                }
                loaded[idx].imports = edges;
            }
            frontier_start = frontier_end;

            if pending.is_empty() {
                break;
            }

            // Phase A: read source units (parallelized when batch is large).
            let mut read_results: Vec<Result<SourceUnit, Diagnostic>> =
                Vec::with_capacity(pending.len());
            if parse_workers <= 1 || pending.len() < 8 {
                for item in &pending {
                    let res = match &item.source {
                        PendingSource::Embedded(u) => Ok(u.clone()),
                        PendingSource::File {
                            path,
                            file_id,
                            module_id,
                        } => read_source_unit(path, *file_id, *module_id, &options.limits),
                    };
                    read_results.push(res);
                }
            } else {
                use std::sync::Mutex;
                use std::sync::atomic::{AtomicUsize, Ordering};

                let n = pending.len();
                let next = AtomicUsize::new(0);
                let slots: Vec<Mutex<Option<Result<SourceUnit, Diagnostic>>>> =
                    (0..n).map(|_| Mutex::new(None)).collect();
                let w = parse_workers.min(n);
                std::thread::scope(|scope| {
                    for _ in 0..w {
                        scope.spawn(|| {
                            loop {
                                let i = next.fetch_add(1, Ordering::Relaxed);
                                if i >= n {
                                    break;
                                }
                                let res = match &pending[i].source {
                                    PendingSource::Embedded(u) => Ok(u.clone()),
                                    PendingSource::File {
                                        path,
                                        file_id,
                                        module_id,
                                    } => read_source_unit(
                                        path,
                                        *file_id,
                                        *module_id,
                                        &options.limits,
                                    ),
                                };
                                if let Ok(mut guard) = slots[i].lock() {
                                    *guard = Some(res);
                                }
                            }
                        });
                    }
                });
                for slot in slots {
                    if let Some(res) = slot.into_inner().ok().flatten() {
                        read_results.push(res);
                    }
                }
            }

            let mut units: Vec<(String, SourceUnit)> = Vec::with_capacity(pending.len());
            for (item, res) in pending.into_iter().zip(read_results) {
                match res {
                    Ok(u) => units.push((item.logical, u)),
                    Err(diag) => {
                        report.phase = diag.phase;
                        report.status = diag.category;
                        report.diagnostics = vec![diag];
                        report.seconds = start.elapsed().as_secs_f64();
                        return report;
                    }
                }
            }

            // Phase B: parse all uncached source units (parallelized when batch is large).
            if parse_workers <= 1 || units.len() < 8 {
                for (logical, mod_unit) in units {
                    report
                        .source_files
                        .insert(mod_unit.file_id, mod_unit.path.clone());
                    let mod_ast = match self.parse_cached(&mod_unit, options, &mut report, observer)
                    {
                        Ok(ast) => ast,
                        Err(diags) => {
                            report.phase = Phase::Parse;
                            report.status = highest_category(&diags);
                            report.diagnostics = diags;
                            report.seconds = start.elapsed().as_secs_f64();
                            return report;
                        }
                    };
                    loaded.push(LoadedMod {
                        logical_path: logical,
                        fingerprint: mod_unit.fingerprint(),
                        ast: mod_ast,
                        imports: Vec::new(),
                    });
                }
            } else {
                use std::sync::Mutex;
                use std::sync::atomic::{AtomicUsize, Ordering};

                type ParsedModResult = Result<zil_syntax::ast::Module, Vec<Diagnostic>>;

                let n = units.len();
                let mut keys: Vec<(Fingerprint, usize)> = Vec::with_capacity(n);
                let mut cached_hits: Vec<Option<ParsedModResult>> = Vec::with_capacity(n);
                let mut miss_indices: Vec<usize> = Vec::new();
                for (i, (_, u)) in units.iter().enumerate() {
                    let key = (u.fingerprint(), options.limits.max_tokens);
                    keys.push(key);
                    if let Some(hit) = self.parse_cache.get(&key) {
                        cached_hits.push(Some(hit.clone()));
                    } else {
                        cached_hits.push(None);
                        miss_indices.push(i);
                    }
                }

                let mut parsed_misses: Vec<Option<ParsedModResult>> =
                    vec![None; miss_indices.len()];
                if !miss_indices.is_empty() {
                    let m = miss_indices.len();
                    let next = AtomicUsize::new(0);
                    let slots: Vec<Mutex<Option<ParsedModResult>>> =
                        (0..m).map(|_| Mutex::new(None)).collect();
                    let w = parse_workers.min(m);
                    std::thread::scope(|scope| {
                        for _ in 0..w {
                            scope.spawn(|| {
                                loop {
                                    let pos = next.fetch_add(1, Ordering::Relaxed);
                                    if pos >= m {
                                        break;
                                    }
                                    let u = &units[miss_indices[pos]].1;
                                    let parsed =
                                        parse_with_limits(u, &options.limits).into_result();
                                    if let Ok(mut guard) = slots[pos].lock() {
                                        *guard = Some(parsed);
                                    }
                                }
                            });
                        }
                    });
                    for (pos, slot) in slots.into_iter().enumerate() {
                        parsed_misses[pos] = slot.into_inner().ok().flatten();
                    }
                }

                let mut miss_cursor = 0usize;
                for (i, (logical, mod_unit)) in units.into_iter().enumerate() {
                    report
                        .source_files
                        .insert(mod_unit.file_id, mod_unit.path.clone());
                    let parsed = if let Some(hit) = cached_hits[i].take() {
                        let ev = QueryEvent {
                            phase: Phase::Parse,
                            disposition: QueryDisposition::Reused,
                        };
                        observer(&ev);
                        report.query_events.push(ev);
                        report.cache_hit = true;
                        hit
                    } else {
                        let res = parsed_misses[miss_cursor].take().unwrap_or_else(|| {
                            parse_with_limits(&mod_unit, &options.limits).into_result()
                        });
                        miss_cursor += 1;
                        let ev = QueryEvent {
                            phase: Phase::Parse,
                            disposition: QueryDisposition::Executed,
                        };
                        observer(&ev);
                        report.query_events.push(ev);
                        if self.capacity > 0 {
                            self.insert_parse_cache(keys[i], res.clone());
                        }
                        res
                    };
                    let mod_ast = match parsed {
                        Ok(ast) => ast,
                        Err(diags) => {
                            report.phase = Phase::Parse;
                            report.status = highest_category(&diags);
                            report.diagnostics = diags;
                            report.seconds = start.elapsed().as_secs_f64();
                            return report;
                        }
                    };
                    loaded.push(LoadedMod {
                        logical_path: logical,
                        fingerprint: keys[i].0,
                        ast: mod_ast,
                        imports: Vec::new(),
                    });
                }
            }
        }

        // Reject user declarations of reserved built-in type family `Option`.
        for m in &loaded {
            for item in &m.ast.items {
                let reserved_name = match &item.kind {
                    zil_syntax::ast::ItemKind::Enum(e) => Some(e.name.as_str()),
                    zil_syntax::ast::ItemKind::Aggregate(a) => Some(a.name.as_str()),
                    zil_syntax::ast::ItemKind::TypeAlias(t) => Some(t.name.as_str()),
                    _ => None,
                };
                if reserved_name == Some("Option") {
                    report.phase = Phase::Expand;
                    report.status = ResultCategory::SafeReject;
                    report.diagnostics.push(Diagnostic::error(
                        Phase::Expand,
                        "E-GENERIC-ARGUMENT",
                        Some(item.span),
                        "Option is a reserved built-in type family",
                    ));
                    report.seconds = start.elapsed().as_secs_f64();
                    return report;
                }
            }
        }

        let ast_refs: Vec<&zil_syntax::ast::Module> = loaded.iter().map(|m| &m.ast).collect();
        let ct_import_vecs: Vec<Vec<zil_check::ct_original::CtOriginalImport<'_>>> = loaded
            .iter()
            .map(|m| {
                m.imports
                    .iter()
                    .map(|(alias, target)| zil_check::ct_original::CtOriginalImport {
                        alias: alias.as_str(),
                        target: *target,
                    })
                    .collect()
            })
            .collect();
        let ct_mods: Vec<zil_check::ct_original::CtOriginalModule<'_>> = loaded
            .iter()
            .zip(ct_import_vecs.iter())
            .map(|(m, imps)| zil_check::ct_original::CtOriginalModule {
                syntax: &m.ast,
                imports: imps.as_slice(),
            })
            .collect();
        let mut ct_meter = zil_check::ct_original::CtVerificationMeter::new(1_000_000);
        if let Err(ct_diags) = zil_check::ct_original::verify_ct_original_definitions(
            &ct_mods,
            &[],
            &options.target,
            &options.limits,
            &mut ct_meter,
        ) {
            report.phase = Phase::Ct;
            report.status = highest_category(&ct_diags);
            report.diagnostics = ct_diags;
            report.seconds = start.elapsed().as_secs_f64();
            return report;
        }

        let mod_fps: Vec<(&str, Fingerprint)> = loaded
            .iter()
            .map(|m| (m.logical_path.as_str(), m.fingerprint))
            .collect();
        let program_sem_key =
            semantic_cache::program_semantic_key(&mod_fps, &options.target, &options.limits);

        // Fast-Path A: consult untrusted semantic candidate cache (in-memory session
        // first, then explicit `--semantic-cache DIR`). Any cached candidate MUST
        // pass fresh `verify_ct_original_definitions` (above), fresh `check_fidelity`,
        // AND fresh `check_module` (`verification_runs == 1`); on any corruption or
        // verification mismatch, purge and fall through to fresh build.
        let mut verified_from_semantic_cache: Option<zil_check::VerifiedModule> = None;
        if options.stop_after >= Stage::Verify {
            let mem_candidate = self
                .semantic_snapshots
                .iter()
                .find(|s| s.key == program_sem_key)
                .map(|s| (s.module.clone(), s.source_map.clone(), s.cert.clone()));
            let from_disk = mem_candidate.is_none();
            let candidate = mem_candidate.or_else(|| {
                options.semantic_cache.as_deref().and_then(|dir| {
                    semantic_cache::read_disk_semantic_candidate(dir, program_sem_key)
                })
            });
            if let Some((cand_mod, cand_smap, cand_cert)) = candidate {
                let fidelity_ok = check_fidelity(&cand_mod, &cand_smap, &ast_refs).is_ok();
                if fidelity_ok {
                    match check_module(cand_mod, &cand_cert, CheckBudget { steps: 1_000_000 }) {
                        Ok(v) => {
                            if from_disk && self.capacity > 0 {
                                self.store_semantic_snapshot(
                                    program_sem_key,
                                    v.module().clone(),
                                    cand_smap,
                                    cand_cert,
                                );
                            }
                            let types_ev = QueryEvent {
                                phase: Phase::Types,
                                disposition: QueryDisposition::Reused,
                            };
                            observer(&types_ev);
                            report.query_events.push(types_ev);
                            let analyze_ev = QueryEvent {
                                phase: Phase::Analyze,
                                disposition: QueryDisposition::Reused,
                            };
                            observer(&analyze_ev);
                            report.query_events.push(analyze_ev);
                            report.cache_hit = true;
                            verified_from_semantic_cache = Some(v);
                        }
                        Err(_) => {
                            self.semantic_snapshots.retain(|s| s.key != program_sem_key);
                            if let Some(dir) = options.semantic_cache.as_deref() {
                                semantic_cache::remove_disk_semantic_candidate(
                                    dir,
                                    program_sem_key,
                                );
                            }
                        }
                    }
                } else {
                    self.semantic_snapshots.retain(|s| s.key != program_sem_key);
                    if let Some(dir) = options.semantic_cache.as_deref() {
                        semantic_cache::remove_disk_semantic_candidate(dir, program_sem_key);
                    }
                }
            }
        }

        let verified = if let Some(v) = verified_from_semantic_cache {
            v
        } else {
            let mut ct = body_pipeline::InterpCtEvaluator;
            let mod_inputs: Vec<ModuleInput<'_>> = loaded
                .iter()
                .map(|m| ModuleInput {
                    path: m.logical_path.clone(),
                    ast: &m.ast,
                })
                .collect();
            let lowered = match lower_program(&mod_inputs, TypeckLimits::default(), Some(&mut ct)) {
                Ok(l) => l,
                Err(diags) => {
                    report.phase = Phase::Types;
                    report.status = highest_category(&diags);
                    report.diagnostics = diags;
                    report.seconds = start.elapsed().as_secs_f64();
                    return report;
                }
            };

            if options.stop_after <= Stage::Types {
                report.phase = Phase::Types;
                report.seconds = start.elapsed().as_secs_f64();
                return report;
            }

            if let Err(e) = check_fidelity(&lowered.module, &lowered.source_map, &ast_refs) {
                report.phase = Phase::Verify;
                report.status = ResultCategory::SafeReject;
                report.diagnostics.push(Diagnostic::error(
                    Phase::Verify,
                    "E-SOURCE-FIDELITY",
                    None,
                    format!("fidelity check failed in {:?}: {}", e.function, e.reason),
                ));
                report.seconds = start.elapsed().as_secs_f64();
                return report;
            }

            // Fast-Path B: in resident/reused sessions, consult in-RAM per-function
            // candidate certificate cache during solving. Cached certificates are
            // untrusted; if fresh `check_module` rejects when any cached cert was
            // used, purge and re-solve cleanly without cache.
            let cert_cache = semantic_cache::SessionCertCache::new(&self.fn_cert_cache);
            let use_cert_cache = !self.semantic_snapshots.is_empty();
            if use_cert_cache
                && self.fn_cert_cache.read().is_ok_and(|g| g.is_empty())
                && let Some(prev) = self.semantic_snapshots.back()
            {
                zil_solve::seed_certificate_cache(&prev.module, &prev.cert, &cert_cache);
            }
            let (mut cert, cert_hits) = match solve_module_with_cache(
                &lowered.module,
                SolveBudget { steps: 1_000_000 },
                if use_cert_cache {
                    Some(&cert_cache)
                } else {
                    None
                },
            ) {
                Ok(pair) => pair,
                Err(e) => {
                    report.phase = Phase::Analyze;
                    report.status = ResultCategory::SafeReject;
                    report.diagnostics.push(Diagnostic::error(
                        Phase::Analyze,
                        "E-SAFETY-SOLVE",
                        None,
                        format!("solver rejected: {e:?}"),
                    ));
                    report.seconds = start.elapsed().as_secs_f64();
                    return report;
                }
            };

            if options.stop_after <= Stage::Analyze {
                report.phase = Phase::Analyze;
                report.seconds = start.elapsed().as_secs_f64();
                return report;
            }

            let lowered_for_fallback = if cert_hits > 0 {
                Some(lowered.module.clone())
            } else {
                None
            };
            let verified =
                match check_module(lowered.module, &cert, CheckBudget { steps: 1_000_000 }) {
                    Ok(v) => v,
                    Err(e) => {
                        if let Some(fallback_mod) = lowered_for_fallback {
                            cert_cache.purge_all();
                            match solve_module(&fallback_mod, SolveBudget { steps: 1_000_000 }) {
                                Ok(fresh_cert) => {
                                    cert = fresh_cert;
                                    match check_module(
                                        fallback_mod,
                                        &cert,
                                        CheckBudget { steps: 1_000_000 },
                                    ) {
                                        Ok(v) => v,
                                        Err(e2) => {
                                            report.phase = Phase::Verify;
                                            report.status = ResultCategory::SafeReject;
                                            report.diagnostics.push(Diagnostic::error(
                                                Phase::Verify,
                                                "E-SAFETY-CHECK",
                                                None,
                                                format!("checker rejected: {e2:?}"),
                                            ));
                                            report.seconds = start.elapsed().as_secs_f64();
                                            return report;
                                        }
                                    }
                                }
                                Err(se) => {
                                    report.phase = Phase::Analyze;
                                    report.status = ResultCategory::SafeReject;
                                    report.diagnostics.push(Diagnostic::error(
                                        Phase::Analyze,
                                        "E-SAFETY-SOLVE",
                                        None,
                                        format!("solver rejected: {se:?}"),
                                    ));
                                    report.seconds = start.elapsed().as_secs_f64();
                                    return report;
                                }
                            }
                        } else {
                            report.phase = Phase::Verify;
                            report.status = ResultCategory::SafeReject;
                            report.diagnostics.push(Diagnostic::error(
                                Phase::Verify,
                                "E-SAFETY-CHECK",
                                None,
                                format!("checker rejected: {e:?}"),
                            ));
                            report.seconds = start.elapsed().as_secs_f64();
                            return report;
                        }
                    }
                };

            if let Some(dir) = options.semantic_cache.as_deref() {
                semantic_cache::write_disk_semantic_candidate(
                    dir,
                    program_sem_key,
                    verified.module(),
                    &lowered.source_map,
                    &cert,
                );
            }
            // Populate in-memory session snapshot & function certs for subsequent
            // resident/session compilations.
            if self.capacity > 0 {
                self.store_semantic_snapshot(
                    program_sem_key,
                    verified.module().clone(),
                    lowered.source_map,
                    cert,
                );
            }
            verified
        };
        report.verification_runs = 1;
        let verify_ev = QueryEvent {
            phase: Phase::Verify,
            disposition: QueryDisposition::Executed,
        };
        observer(&verify_ev);
        report.query_events.push(verify_ev);

        if options.stop_after <= Stage::Lower {
            report.phase = Phase::Verify;
            report.seconds = start.elapsed().as_secs_f64();
            return report;
        }

        if options.backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            report.phase = Phase::Codegen;
            report.status = ResultCategory::Unsupported;
            report.diagnostics.push(Diagnostic::new(
                Phase::Codegen,
                ResultCategory::Unsupported,
                "E-LLVM-API-UNAVAILABLE",
                "llvm-api backend is not enabled in this build",
                None,
            ));
            report.seconds = start.elapsed().as_secs_f64();
            return report;
        }
        let out_path = options.output.clone().unwrap_or_else(|| {
            PathBuf::from(if options.target.triple.contains("windows") {
                "a.exe"
            } else {
                "a.out"
            })
        });
        match build_verified_body(
            &verified,
            &BodyBuildOptions {
                output: out_path.clone(),
                optimization: options.optimization,
                backend: options.backend,
                target: options.target.clone(),
                windows: options.windows.clone(),
                clang: options.clang.clone(),
                extra_c_sources: options.extra_c_sources.clone(),
                emit_c_main: true,
                entry: options.entry.clone(),
                codegen_units: options.codegen.units,
                codegen_jobs: options.codegen.jobs,
                object_cache: options.object_cache.clone(),
                object_cache_max_bytes: options.object_cache_max_bytes,
                emit_ir_file: options.emit_dir.is_some() || options.object_cache.is_none(),
            },
        ) {
            Ok(built) => {
                report.phase = Phase::Codegen;
                report.commands = built.commands;
                if options.emit_dir.is_some()
                    || options.object_cache.is_none()
                    || options.backend == NativeBackend::Clang
                {
                    report.artifacts.insert("llvm".into(), built.ir_path);
                }
                report.artifacts.insert("executable".into(), out_path);
                if options.object_cache.is_some() {
                    report.disk_cache = if built.cache_misses == 0 && built.cache_hits > 0 {
                        DiskCacheStatus::Hit
                    } else {
                        DiskCacheStatus::Rebuilt
                    };
                }
                report.native_codegen = Some(CodegenSummary {
                    backend: options.backend,
                    object_reports: built.object_reports,
                    requested_units: options.codegen.units,
                    actual_units: built.actual_units,
                    jobs: options.codegen.jobs,
                    fallback: None,
                    cache_hits: built.cache_hits,
                    cache_misses: built.cache_misses,
                    cache_evicted_entries: built.cache_evicted_entries,
                    cache_evicted_bytes: built.cache_evicted_bytes,
                    cache_evict_ns: built.cache_evict_ns,
                    emit_ns: built.emit_ns,
                    link_ns: built.link_ns,
                    link_skipped: built.link_skipped,
                });
            }
            Err(e) => {
                report.phase = Phase::Codegen;
                if e.unsupported {
                    report.status = ResultCategory::Unsupported;
                    report.diagnostics.push(Diagnostic::new(
                        Phase::Codegen,
                        ResultCategory::Unsupported,
                        "E-TARGET-UNSUPPORTED",
                        e.message,
                        None,
                    ));
                } else {
                    report.status = ResultCategory::SafeReject;
                    report.diagnostics.push(Diagnostic::error(
                        Phase::Codegen,
                        "E-CODEGEN",
                        None,
                        e.message,
                    ));
                }
            }
        }
        report.seconds = start.elapsed().as_secs_f64();
        report
    }
}

const EMBEDDED_STD_MODULES: &[(&str, &str)] = &[
    (
        "buffered_io",
        include_str!("../../../lib/std/buffered_io.zil"),
    ),
    (
        "collections",
        include_str!("../../../lib/std/collections.zil"),
    ),
    ("env", include_str!("../../../lib/std/env.zil")),
    ("float", include_str!("../../../lib/std/float.zil")),
    ("fs", include_str!("../../../lib/std/fs.zil")),
    ("graph", include_str!("../../../lib/std/graph.zil")),
    (
        "i64_hash_map",
        include_str!("../../../lib/std/i64_hash_map.zil"),
    ),
    (
        "i64_hash_set",
        include_str!("../../../lib/std/i64_hash_set.zil"),
    ),
    ("int", include_str!("../../../lib/std/int.zil")),
    ("io", include_str!("../../../lib/std/io.zil")),
    ("iter", include_str!("../../../lib/std/iter.zil")),
    ("net", include_str!("../../../lib/std/net.zil")),
    (
        "owning_entries",
        include_str!("../../../lib/std/owning_entries.zil"),
    ),
    (
        "owning_hash_map",
        include_str!("../../../lib/std/owning_hash_map.zil"),
    ),
    (
        "owning_hash_set",
        include_str!("../../../lib/std/owning_hash_set.zil"),
    ),
    (
        "owning_map",
        include_str!("../../../lib/std/owning_map.zil"),
    ),
    ("path", include_str!("../../../lib/std/path.zil")),
    (
        "pod_hash_map",
        include_str!("../../../lib/std/pod_hash_map.zil"),
    ),
    (
        "pod_hash_set",
        include_str!("../../../lib/std/pod_hash_set.zil"),
    ),
    ("store", include_str!("../../../lib/std/store.zil")),
    ("string", include_str!("../../../lib/std/string.zil")),
    (
        "string_map",
        include_str!("../../../lib/std/string_map.zil"),
    ),
    ("text_edit", include_str!("../../../lib/std/text_edit.zil")),
    ("time", include_str!("../../../lib/std/time.zil")),
    ("ui_input", include_str!("../../../lib/std/ui_input.zil")),
    ("ui_layout", include_str!("../../../lib/std/ui_layout.zil")),
    ("ui_raster", include_str!("../../../lib/std/ui_raster.zil")),
    ("utf8", include_str!("../../../lib/std/utf8.zil")),
];

const EMBEDDED_WINDOWS_STD_MODULES: &[(&str, &str)] = &[
    ("env", include_str!("../../../lib/std/windows/env.zil")),
    ("float", include_str!("../../../lib/std/windows/float.zil")),
    ("fs", include_str!("../../../lib/std/windows/fs.zil")),
    ("io", include_str!("../../../lib/std/windows/io.zil")),
    ("net", include_str!("../../../lib/std/windows/net.zil")),
    ("path", include_str!("../../../lib/std/windows/path.zil")),
    ("time", include_str!("../../../lib/std/windows/time.zil")),
];

fn embedded_std_source(
    subpath: &[String],
    target: &TargetProfile,
) -> Option<(String, &'static str)> {
    if subpath.len() != 1 {
        return None;
    }
    let name = subpath[0].as_str();
    if target.triple.contains("windows")
        && let Some((mod_name, text)) = EMBEDDED_WINDOWS_STD_MODULES
            .iter()
            .find(|(mod_name, _)| *mod_name == name)
    {
        return Some((format!("lib/std/{mod_name}.zil"), *text));
    }
    EMBEDDED_STD_MODULES
        .iter()
        .find(|(mod_name, _)| *mod_name == name)
        .map(|(mod_name, text)| (format!("lib/std/{mod_name}.zil"), *text))
}

fn validate_project_std_modules(
    project: &ProjectSources,
    target: &TargetProfile,
) -> Result<(), Diagnostic> {
    for (segments, mid) in &project.paths {
        if segments.first().map(String::as_str) != Some("std") {
            continue;
        }
        let unit = project.sources.iter().find(|s| s.module_id == *mid);
        let canonical = embedded_std_source(&segments[1..], target).map(|(_, text)| text);
        let matches = match (unit, canonical) {
            (Some(u), Some(c)) => u.text == c,
            _ => false,
        };
        if !matches {
            return Err(Diagnostic::error(
                Phase::Driver,
                "E-STD-RESERVED",
                None,
                format!(
                    "module `{}` is in the reserved `std` namespace and cannot be forged or overridden",
                    segments.join(".")
                ),
            ));
        }
    }
    for unit in &project.sources {
        let sub = if let Some(rest) = unit.path.strip_prefix("std.") {
            Some(rest.strip_suffix(".zil").unwrap_or(rest))
        } else if let Some(rest) = unit.path.strip_prefix("std/") {
            Some(rest.strip_suffix(".zil").unwrap_or(rest))
        } else if let Some(rest) = unit.path.strip_prefix("lib/std/") {
            Some(rest.strip_suffix(".zil").unwrap_or(rest))
        } else {
            None
        };
        if let Some(sub) = sub {
            let segs: Vec<String> = sub.split(['.', '/']).map(str::to_string).collect();
            let canonical = embedded_std_source(&segs, target).map(|(_, text)| text);
            if canonical != Some(unit.text.as_str()) {
                return Err(Diagnostic::error(
                    Phase::Driver,
                    "E-STD-RESERVED",
                    None,
                    format!(
                        "module `{}` is in the reserved `std` namespace and cannot be forged or overridden",
                        unit.path
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn highest_category(diags: &[Diagnostic]) -> ResultCategory {
    if diags
        .iter()
        .any(|d| d.category == ResultCategory::ResourceLimit)
    {
        ResultCategory::ResourceLimit
    } else if diags
        .iter()
        .any(|d| d.category == ResultCategory::Unsupported)
    {
        ResultCategory::Unsupported
    } else if diags
        .iter()
        .any(|d| d.category == ResultCategory::Cancelled)
    {
        ResultCategory::Cancelled
    } else {
        ResultCategory::SafeReject
    }
}

fn read_source_unit(
    path: &Path,
    file_id: FileId,
    module_id: ModuleId,
    limits: &Limits,
) -> Result<SourceUnit, Diagnostic> {
    let mut file = File::open(path).map_err(|e| {
        Diagnostic::error(
            Phase::Driver,
            "E-IO",
            None,
            format!("failed to open {}: {e}", path.display()),
        )
    })?;
    let mut bytes = Vec::new();
    let max = limits.max_source_bytes.saturating_add(1);
    file.by_ref()
        .take(max as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            Diagnostic::error(
                Phase::Driver,
                "E-IO",
                None,
                format!("failed to read {}: {e}", path.display()),
            )
        })?;
    if bytes.len() > limits.max_source_bytes {
        return Err(Diagnostic::limit(
            Phase::Parse,
            "source exceeds max_source_bytes limit",
            None,
        ));
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        Diagnostic::error(
            Phase::Parse,
            "E-UTF8",
            None,
            format!("{} is not valid UTF-8", path.display()),
        )
    })?;
    Ok(SourceUnit::new(
        file_id,
        module_id,
        path.to_string_lossy().into_owned(),
        text,
    ))
}

pub fn compile_source(source: &SourceUnit, options: &CompileOptions) -> CompileReport {
    CompilationSession::ephemeral().compile_source_with_observer(source, options, &mut |_| {})
}

pub fn compile_path(path: &Path, options: &CompileOptions) -> CompileReport {
    CompilationSession::ephemeral().compile_path(path, options)
}

pub fn compile_project(project: &ProjectSources, options: &CompileOptions) -> CompileReport {
    let Some(entry) = project
        .sources
        .iter()
        .find(|s| s.module_id == project.entry)
    else {
        return CompileReport {
            status: ResultCategory::SafeReject,
            phase: Phase::Driver,
            diagnostics: vec![Diagnostic::error(
                Phase::Driver,
                "E-PROJECT-ENTRY",
                None,
                "project entry module missing",
            )],
            artifacts: BTreeMap::new(),
            input_fingerprint: Fingerprint::ZERO,
            seconds: 0.0,
            cache_hit: false,
            commands: Vec::new(),
            native_codegen: None,
            disk_cache: DiskCacheStatus::Disabled,
            query_events: Vec::new(),
            verification_runs: 0,
            source_files: BTreeMap::new(),
        };
    };
    let start = Instant::now();
    let mut report = CompileReport::new(entry);
    for s in &project.sources {
        report.source_files.insert(s.file_id, s.path.clone());
    }
    if let Err(diag) = validate_project_std_modules(project, &options.target) {
        report.status = diag.category;
        report.phase = diag.phase;
        report.diagnostics = vec![diag];
        report.seconds = start.elapsed().as_secs_f64();
        return report;
    }
    let inputs: Vec<(&str, &str)> = project
        .sources
        .iter()
        .map(|s| (s.path.as_str(), s.text.as_str()))
        .collect();
    match verify_body_program(&inputs) {
        Ok(verified) => {
            report.verification_runs = 1;
            report.phase = Phase::Verify;
            if options.stop_after >= Stage::Build {
                let out_path = options.output.clone().unwrap_or_else(|| {
                    PathBuf::from(if options.target.triple.contains("windows") {
                        "a.exe"
                    } else {
                        "a.out"
                    })
                });
                match build_verified_body(
                    &verified,
                    &BodyBuildOptions {
                        output: out_path.clone(),
                        optimization: options.optimization,
                        backend: options.backend,
                        target: options.target.clone(),
                        windows: options.windows.clone(),
                        clang: options.clang.clone(),
                        extra_c_sources: options.extra_c_sources.clone(),
                        emit_c_main: true,
                        entry: options.entry.clone(),
                        codegen_units: options.codegen.units,
                        codegen_jobs: options.codegen.jobs,
                        object_cache: options.object_cache.clone(),
                        object_cache_max_bytes: options.object_cache_max_bytes,
                        emit_ir_file: options.emit_dir.is_some() || options.object_cache.is_none(),
                    },
                ) {
                    Ok(built) => {
                        report.phase = Phase::Codegen;
                        report.commands = built.commands;
                        if options.emit_dir.is_some()
                            || options.object_cache.is_none()
                            || options.backend == NativeBackend::Clang
                        {
                            report.artifacts.insert("llvm".into(), built.ir_path);
                        }
                        report.artifacts.insert("executable".into(), out_path);
                        if options.object_cache.is_some() {
                            report.disk_cache = if built.cache_misses == 0 && built.cache_hits > 0 {
                                DiskCacheStatus::Hit
                            } else {
                                DiskCacheStatus::Rebuilt
                            };
                        }
                        report.native_codegen = Some(CodegenSummary {
                            backend: options.backend,
                            object_reports: built.object_reports,
                            requested_units: options.codegen.units,
                            actual_units: built.actual_units,
                            jobs: options.codegen.jobs,
                            fallback: None,
                            cache_hits: built.cache_hits,
                            cache_misses: built.cache_misses,
                            cache_evicted_entries: built.cache_evicted_entries,
                            cache_evicted_bytes: built.cache_evicted_bytes,
                            cache_evict_ns: built.cache_evict_ns,
                            emit_ns: built.emit_ns,
                            link_ns: built.link_ns,
                            link_skipped: built.link_skipped,
                        });
                    }
                    Err(e) => {
                        report.phase = Phase::Codegen;
                        if e.unsupported {
                            report.status = ResultCategory::Unsupported;
                            report.diagnostics.push(Diagnostic::new(
                                Phase::Codegen,
                                ResultCategory::Unsupported,
                                "E-TARGET-UNSUPPORTED",
                                e.message,
                                None,
                            ));
                        } else {
                            report.status = ResultCategory::SafeReject;
                            report.diagnostics.push(Diagnostic::error(
                                Phase::Codegen,
                                "E-CODEGEN",
                                None,
                                e.message,
                            ));
                        }
                    }
                }
            }
        }
        Err(e) => {
            report.phase = e.phase;
            report.status = if e.diagnostics.is_empty() {
                ResultCategory::SafeReject
            } else {
                highest_category(&e.diagnostics)
            };
            report.diagnostics = if e.diagnostics.is_empty() {
                vec![Diagnostic::error(e.phase, "E-VERIFY", None, e.message)]
            } else {
                e.diagnostics
            };
        }
    }
    report.seconds = start.elapsed().as_secs_f64();
    report
}

/// Discovers zero-argument `test_*` functions in `path`.
pub fn discover_tests(path: &Path, options: &CompileOptions) -> CompileResult<Vec<String>> {
    let source = read_source_unit(path, FileId(0), ModuleId(0), &options.limits)
        .map_err(|diag| vec![diag])?;
    let ast = parse_with_limits(&source, &options.limits).into_result()?;
    let mut names = Vec::new();
    for item in &ast.items {
        if let zil_syntax::ast::ItemKind::Function(f) = &item.kind
            && f.name.starts_with("test_")
            && f.params.is_empty()
        {
            names.push(f.name.clone());
        }
    }
    Ok(names)
}
