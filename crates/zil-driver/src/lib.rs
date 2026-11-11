#![forbid(unsafe_code)]
//! Shared CLI/editor scheduling. The session reuses only raw phase snapshots;
//! each complete compilation obtains a fresh independent verifier capability.

mod artifacts;
mod contracts;
mod ct_artifact;
pub use ct_artifact::SyntaxLock;
mod ct_program;
mod ct_selection;
mod project;
mod project_manifest;
pub use project_manifest::ProjectManifest;
/// Explicit module import destinations keyed by the source module that imports.
pub type ScopedImports = BTreeMap<(ModuleId, Vec<String>), ModuleId>;
mod semantic_cache;
mod session;
mod stable_source_identity;
pub use project::{ProjectSources, compile_project, discover_tests};
pub use session::CompilationSession;
pub use zil_db::CancellationToken;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
pub use zil_backend_llvm::{
    CodegenOptions, CodegenSummary, FunctionMerging, NativeBackend, Optimization,
    WindowsLinkOptions,
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
    /// Build a verified test entry whose return value becomes process status.
    pub test_harness: bool,
    /// Explicit local project graph. No ambient manifest is loaded by the driver.
    pub project: Option<std::sync::Arc<ProjectManifest>>,
    /// Filesystem project import resolution, keyed by the importing module.
    /// Populated by discovery; explicit ProjectSources keep using `paths`.
    #[doc(hidden)]
    pub scoped_imports: Option<std::sync::Arc<ScopedImports>>,
    pub cache_dir: Option<PathBuf>,
    /// Explicitly trusted local raw TypedProgram cache; never stores Verified.
    pub semantic_cache: Option<PathBuf>,
    /// Research opt-in: stable semantic query owners and batched propagation.
    /// This changes raw computation scheduling only, never Verify authority.
    pub reactive_queries: bool,
    /// Research opt-in: reuse sealed source-produced frontend work products.
    /// Independent from reactive scheduling for A/B/C attribution.
    pub producer_reuse: bool,
    /// Opt-in detailed cost accounting. Disabled requests perform no added
    /// per-function clock reads; performance comparisons keep this disabled.
    pub profile_compilation: bool,
    /// Trusted local machine-object cache; independent of source-candidate cache.
    pub object_cache: Option<PathBuf>,
    pub clang: Option<PathBuf>,
    pub optimization: zil_backend_llvm::Optimization,
    /// Explicit Linux build without generated ZIL symbol exports; default false.
    pub closed_executable: bool,
    /// Explicit O2 bounded complete imports; copy-cost overflow keeps ordinary units.
    pub incremental_o2: bool,
    /// O2 function-merging policy, with address-preserving reference controls.
    pub function_merging: FunctionMerging,
    pub codegen: CodegenOptions,
    pub windows: Option<zil_backend_llvm::WindowsLinkOptions>,
    pub target: TargetProfile,
    pub limits: Limits,
    pub cancellation: CancellationToken,
    /// Explicit non-source build inputs, such as a locked import/configuration
    /// manifest. The scalar driver never silently reads ambient configuration.
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
            reactive_queries: false,
            producer_reuse: false,
            profile_compilation: false,
            object_cache: None,
            clang: None,
            optimization: zil_backend_llvm::Optimization::O0,
            closed_executable: false,
            incremental_o2: false,
            function_merging: FunctionMerging::Disabled,
            codegen: CodegenOptions::default(),
            windows: None,
            target: TargetProfile::host_llvm(),
            limits: Limits::default(),
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
/// One actual pipeline attempt. Rejected raw candidates retain their own
/// diagnostics and costs; they do not replace the final source result.
#[derive(Clone, Debug)]
pub struct CompileAttempt {
    pub retry_reason: Option<String>,
    pub input_fingerprint: Fingerprint,
    pub source_files: BTreeMap<FileId, String>,
    pub status: ResultCategory,
    pub phase: Phase,
    pub diagnostics: Vec<Diagnostic>,
    pub verification_runs: usize,
    pub function_type_work: Option<zil_types::FunctionStats>,
    pub producer_work: Option<zil_expand::ProducerStats>,
    pub initialization_work: Option<zil_analyze::AnalysisStats>,
    pub phase_timings_ns: BTreeMap<Phase, u128>,
    pub driver_timings_ns: BTreeMap<&'static str, u128>,
}
impl CompileAttempt {
    fn to_json(&self) -> String {
        format!(
            "{{\"retry_reason\":{},\"input_fingerprint\":{},\"source_files\":{{{}}},\"status\":{},\"phase\":{},\"diagnostics\":[{}],\"verification_runs\":{},\"frontend_stats\":{},\"phase_timings_ns\":{{{}}},\"driver_timings_ns\":{{{}}}}}",
            self.retry_reason
                .as_ref()
                .map_or_else(|| "null".into(), |reason| json_string(reason)),
            json_string(&self.input_fingerprint.to_hex()),
            self.source_files
                .iter()
                .map(|(id, path)| format!(
                    "{}:{}",
                    json_string(&id.0.to_string()),
                    json_string(path)
                ))
                .collect::<Vec<_>>()
                .join(","),
            json_string(self.status.as_str()),
            json_string(self.phase.as_str()),
            self.diagnostics
                .iter()
                .map(Diagnostic::to_json)
                .collect::<Vec<_>>()
                .join(","),
            self.verification_runs,
            frontend_stats_json(
                self.function_type_work,
                self.initialization_work,
                self.producer_work.as_ref()
            ),
            phase_timings_json(&self.phase_timings_ns),
            named_timings_json(&self.driver_timings_ns),
        )
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
    /// Execution counters sum all attempts in this request; retained gauges
    /// describe the final attempt. A whole-project hit reports None, never old
    /// session work. Per-attempt diagnostics and costs remain in `attempts`.
    pub function_type_work: Option<zil_types::FunctionStats>,
    pub producer_work: Option<zil_expand::ProducerStats>,
    pub initialization_work: Option<zil_analyze::AnalysisStats>,
    /// Wall time of complete phase queries, including digest/publication, and
    /// current Verify/Lower/codegen, summed across all attempts. Input
    /// discovery and final report serialization remain in complete request time.
    pub phase_timings_ns: BTreeMap<Phase, u128>,
    /// Non-overlapping request work outside phase queries. Detailed timers are
    /// explicitly opt-in; the remaining request time is reported unassigned.
    pub driver_timings_ns: BTreeMap<&'static str, u128>,
    pub cost_profiled: bool,
    pub attempts: Vec<CompileAttempt>,
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
            function_type_work: None,
            producer_work: None,
            initialization_work: None,
            phase_timings_ns: BTreeMap::new(),
            driver_timings_ns: BTreeMap::new(),
            cost_profiled: false,
            attempts: Vec::new(),
        }
    }
    pub fn to_json(&self) -> String {
        format!(
            "{{\"status\":{},\"phase\":{},\"diagnostics\":[{}],\"artifacts\":{{{}}},\"input_fingerprint\":{},\"seconds\":{},\"cache_hit\":{},\"commands\":[{}],\"native_codegen\":{},\"disk_cache\":{},\"query_events\":[{}],\"verification_runs\":{},\"source_files\":{{{}}},\"scalar_interfaces\":[{}],\"frontend_stats\":{},\"phase_timings_ns\":{{{}}},\"driver_timings_ns\":{{{}}},\"exclusive_costs\":{},\"attempts\":[{}]}}",
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
            self.scalar_interfaces.iter().map(|interface| format!("{{\"module\":{},\"signature_fingerprint\":{},\"implementation_fingerprint\":{},\"contract_fingerprint\":{},\"result_origins\":{},\"callable_contracts\":{},\"callable_returns\":{},\"exports\":[{}]}}", interface.module.0, json_string(&interface.signature_fingerprint.to_hex()), json_string(&interface.implementation_fingerprint.to_hex()), interface.contract_fingerprint.map_or_else(|| "null".into(), |hash| json_string(&hash.to_hex())), interface.result_origins.as_ref().map_or_else(|| "null".into(), |origins| format!("[{}]", origins.iter().map(|origin| format!("{{\"name\":{},\"parameters\":{:?},\"static_storage\":{}}}", json_string(&origin.name), origin.parameters, origin.static_storage)).collect::<Vec<_>>().join(","))), contracts::callable_json(interface), contracts::callable_return_json(interface), interface.exports.iter().map(|f| json_string(&f.name)).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(","),
            self.frontend_stats_json(),
            phase_timings_json(&self.phase_timings_ns),
            named_timings_json(&self.driver_timings_ns),
            self.exclusive_costs_json(),
            self.attempts.iter().map(CompileAttempt::to_json).collect::<Vec<_>>().join(","),
        )
    }
    fn frontend_stats_json(&self) -> String {
        frontend_stats_json(
            self.function_type_work,
            self.initialization_work,
            self.producer_work.as_ref(),
        )
    }
    fn capture_attempt(
        &self,
        retry_reason: Option<&str>,
        diagnostics: Vec<Diagnostic>,
    ) -> CompileAttempt {
        CompileAttempt {
            retry_reason: retry_reason.map(str::to_owned),
            input_fingerprint: self.input_fingerprint,
            source_files: self.source_files.clone(),
            status: diagnostics
                .first()
                .map_or(self.status, |diagnostic| diagnostic.category),
            phase: diagnostics
                .first()
                .map_or(self.phase, |diagnostic| diagnostic.phase),
            diagnostics,
            verification_runs: self.verification_runs.saturating_sub(
                self.attempts
                    .iter()
                    .map(|attempt| attempt.verification_runs)
                    .sum(),
            ),
            function_type_work: self.function_type_work,
            producer_work: self.producer_work.clone(),
            initialization_work: self.initialization_work,
            phase_timings_ns: self.phase_timings_ns.clone(),
            driver_timings_ns: self.driver_timings_ns.clone(),
        }
    }
    fn restart_attempt(&mut self, mut next: Self, reason: &str, diagnostics: Vec<Diagnostic>) {
        let rejected = self.capture_attempt(Some(reason), diagnostics);
        let mut attempts = std::mem::take(&mut self.attempts);
        attempts.push(rejected);
        next.attempts = attempts;
        next.query_events = std::mem::take(&mut self.query_events);
        next.verification_runs = self.verification_runs;
        next.seconds = self.seconds;
        next.cost_profiled = self.cost_profiled;
        // A checkpoint may contain work done before importing the candidate.
        // That prefix belongs to the rejected attempt and is never billed twice.
        next.function_type_work = None;
        next.producer_work = None;
        next.initialization_work = None;
        next.phase_timings_ns.clear();
        next.driver_timings_ns.clear();
        *self = next;
    }
    fn finish_attempts(&mut self) {
        let final_attempt = self.capture_attempt(None, self.diagnostics.clone());
        self.attempts.push(final_attempt);
        let final_types = self.function_type_work;
        let final_analysis = self.initialization_work;
        self.function_type_work = None;
        self.producer_work = None;
        self.initialization_work = None;
        self.phase_timings_ns.clear();
        self.driver_timings_ns.clear();
        for attempt in &self.attempts {
            if let Some(work) = attempt.function_type_work {
                add_function_work(&mut self.function_type_work, work);
            }
            if let Some(work) = &attempt.producer_work {
                self.producer_work
                    .get_or_insert_with(Default::default)
                    .add(work);
            }
            if let Some(work) = attempt.initialization_work {
                let total = self
                    .initialization_work
                    .get_or_insert_with(Default::default);
                total.executed_functions = total
                    .executed_functions
                    .saturating_add(work.executed_functions);
                total.reused_functions =
                    total.reused_functions.saturating_add(work.reused_functions);
            }
            for (phase, nanos) in &attempt.phase_timings_ns {
                *self.phase_timings_ns.entry(*phase).or_default() += nanos;
            }
            for (name, nanos) in &attempt.driver_timings_ns {
                *self.driver_timings_ns.entry(name).or_default() += nanos;
            }
        }
        if let Some(total) = &mut self.function_type_work {
            total.graph.retained_nodes = final_types.map_or(0, |work| work.graph.retained_nodes);
            total.graph.retained_edges = final_types.map_or(0, |work| work.graph.retained_edges);
        }
        if let Some(total) = &mut self.initialization_work {
            total.retained_queries = final_analysis.map_or(0, |work| work.retained_queries);
        }
    }
    fn record_phase_time(&mut self, phase: Phase, start: std::time::Instant) {
        *self.phase_timings_ns.entry(phase).or_default() += start.elapsed().as_nanos();
    }
    fn record_driver_cost(&mut self, name: &'static str, start: Option<std::time::Instant>) {
        if let Some(start) = start {
            *self.driver_timings_ns.entry(name).or_default() += start.elapsed().as_nanos();
        }
    }
    fn exclusive_costs_json(&self) -> String {
        if !self.cost_profiled {
            return "null".into();
        }
        let mut costs = self.driver_timings_ns.clone();
        let mut overlap_errors = BTreeMap::new();
        for (phase, nanos) in &self.phase_timings_ns {
            costs.insert(phase.as_str(), *nanos);
        }
        if let Some(work) = self.function_type_work.filter(|work| work.cost_profiled) {
            let detail = [
                ("types_shape", work.shape_resolution_ns),
                ("types_context", work.context_construction_ns),
                ("types_projection_context", work.projection_context_ns),
                ("types_core_comparison", work.core_validation_ns),
                ("types_input_inventory", work.inventory_ns),
                ("types_graph_maintenance", work.graph_maintenance_ns),
                ("types_query_evaluation", work.evaluator_ns),
                ("types_cfg_reuse", work.cfg_reuse_ns),
                ("types_cfg_execute", work.cfg_execute_ns),
                ("types_cfg_store", work.cfg_store_ns),
            ];
            let phase = costs.remove("types").unwrap_or(0);
            let measured: u128 = detail.iter().map(|(_, n)| *n as u128).sum();
            for (name, nanos) in detail {
                costs.insert(name, nanos as u128);
            }
            costs.insert("types_other", phase.saturating_sub(measured));
            // Expose inconsistent instrumentation instead of silently accepting it.
            if measured > phase {
                overlap_errors.insert("types", measured - phase);
            }
        }
        if let Some(work) = &self.producer_work {
            let detail = [
                ("expand_context", work.context_ns),
                ("expand_production", work.production_ns),
                ("expand_reuse", work.reuse_ns),
                ("expand_assembly", work.assembly_ns),
            ];
            let phase = costs.remove("expand").unwrap_or(0);
            let measured: u128 = detail.iter().map(|(_, n)| *n as u128).sum();
            for (name, nanos) in detail {
                costs.insert(name, nanos as u128);
            }
            costs.insert("expand_other", phase.saturating_sub(measured));
            if measured > phase {
                overlap_errors.insert("expand", measured - phase);
            }
        }
        let total = (self.seconds * 1_000_000_000.0) as u128;
        let classified: u128 = costs.values().copied().sum();
        let remainder = total.saturating_sub(classified);
        format!(
            "{{\"total_ns\":{total},\"classified_ns\":{classified},\"unassigned_ns\":{remainder},\"overlap_ns\":{},\"overlap_errors_ns\":{{{}}},\"segments_ns\":{{{}}}}}",
            classified.saturating_sub(total),
            named_timings_json(&overlap_errors),
            named_timings_json(&costs)
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
fn phase_timings_json(timings: &BTreeMap<Phase, u128>) -> String {
    timings
        .iter()
        .map(|(phase, nanos)| format!("{}:{}", json_string(phase.as_str()), nanos))
        .collect::<Vec<_>>()
        .join(",")
}
fn cost_start(enabled: bool) -> Option<std::time::Instant> {
    enabled.then(std::time::Instant::now)
}
fn named_timings_json(timings: &BTreeMap<&'static str, u128>) -> String {
    timings
        .iter()
        .map(|(name, nanos)| format!("{}:{}", json_string(name), nanos))
        .collect::<Vec<_>>()
        .join(",")
}
fn frontend_stats_json(
    types: Option<zil_types::FunctionStats>,
    analysis: Option<zil_analyze::AnalysisStats>,
    producer: Option<&zil_expand::ProducerStats>,
) -> String {
    let types = types.map_or_else(|| "null".into(), |stats| stats.to_json());
    let analysis = analysis.map_or_else(
        || "null".into(),
        |stats| {
            format!(
                "{{\"executed_functions\":{},\"reused_functions\":{},\"retained_queries\":{}}}",
                stats.executed_functions, stats.reused_functions, stats.retained_queries,
            )
        },
    );
    let producer = producer.map_or_else(|| "null".into(), |stats| stats.to_json());
    format!("{{\"function_types\":{types},\"initialization\":{analysis},\"producer\":{producer}}}")
}
fn add_function_work(total: &mut Option<zil_types::FunctionStats>, work: zil_types::FunctionStats) {
    let total = total.get_or_insert_with(Default::default);
    total.cost_profiled |= work.cost_profiled;
    macro_rules! add {
        ($($field:ident),+ $(,)?) => { $(total.$field = total.$field.saturating_add(work.$field);)+ };
    }
    add!(
        total_ns,
        shape_resolution_ns,
        context_construction_ns,
        projection_context_ns,
        core_validation_ns,
        inventory_ns,
        graph_maintenance_ns,
        evaluator_ns,
        body_key_ns,
        cfg_reuse_ns,
        cfg_execute_ns,
        cfg_store_ns,
        producer_version_reuses,
        inventory_functions_visited,
        reused_functions,
        executed_functions,
        body_key_executions,
        body_key_reuses,
        core_function_eq_comparisons,
        body_key_visited_nodes,
        graph_edges,
        context_modules_visited,
        context_declarations_hashed,
        context_imports_hashed
    );
    macro_rules! graph_add {
        ($($field:ident),+ $(,)?) => { $(total.graph.$field = total.graph.$field.saturating_add(work.graph.$field);)+ };
    }
    graph_add!(
        executed,
        edge_reads,
        notifications,
        enqueues,
        deduplicated,
        reused_bonds,
        new_bonds,
        removed_bonds,
        rank_visits,
        input_updates,
        inventory_visits,
        equal_results,
        changed_results,
        cycle_check_edges,
        validation_edge_reads,
        rank_edge_reads
    );
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

#[cfg(test)]
mod producer_profile_tests;

#[cfg(test)]
mod report_attempt_tests {
    use super::*;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    #[test]
    fn attempt_totals_add_execution_costs_and_keep_only_final_retained_gauges() {
        let source = SourceUnit::new(
            FileId(3),
            ModuleId(7),
            "attempt-source.zil",
            "fn value()->i32=>1",
        );
        let mut report = CompileReport::new(&source);
        let mut first = zil_types::FunctionStats {
            executed_functions: 3,
            body_key_executions: 4,
            ..Default::default()
        };
        first.graph.executed = 4;
        first.graph.retained_nodes = 100;
        first.graph.retained_edges = 80;
        report.function_type_work = Some(first);
        report.initialization_work = Some(zil_analyze::AnalysisStats {
            executed_functions: 3,
            reused_functions: 2,
            retained_queries: 50,
        });
        report.phase_timings_ns.insert(Phase::Types, 11);
        report.verification_runs = 1;
        let diagnostic = Diagnostic::unsupported(
            Phase::Analyze,
            "rejected raw candidate",
            Some(zil_base::Span::new(FileId(3), 0, 1)),
        );
        report.restart_attempt(
            CompileReport::new(&source),
            "rejected_function_work_product",
            vec![diagnostic.clone()],
        );
        let mut final_work = zil_types::FunctionStats {
            executed_functions: 5,
            body_key_executions: 6,
            ..Default::default()
        };
        final_work.graph.executed = 6;
        final_work.graph.retained_nodes = 7;
        final_work.graph.retained_edges = 8;
        report.function_type_work = Some(final_work);
        report.initialization_work = Some(zil_analyze::AnalysisStats {
            executed_functions: 5,
            reused_functions: 1,
            retained_queries: 4,
        });
        report.phase_timings_ns.insert(Phase::Types, 13);
        report.verification_runs += 1;
        report.finish_attempts();
        let work = report.function_type_work.unwrap();
        assert_eq!(work.executed_functions, 8);
        assert_eq!(work.body_key_executions, 10);
        assert_eq!(work.graph.executed, 10);
        assert_eq!(
            (work.graph.retained_nodes, work.graph.retained_edges),
            (7, 8)
        );
        assert_eq!(
            report.initialization_work.unwrap(),
            zil_analyze::AnalysisStats {
                executed_functions: 8,
                reused_functions: 3,
                retained_queries: 4
            }
        );
        assert_eq!(report.phase_timings_ns[&Phase::Types], 24);
        assert_eq!(report.verification_runs, 2);
        assert_eq!(report.attempts[0].diagnostics, vec![diagnostic]);
        assert_eq!(report.attempts[0].source_files[&FileId(3)], source.path);
        assert_eq!(report.attempts[0].input_fingerprint, source.fingerprint());
        assert_eq!(report.attempts[0].verification_runs, 1);
        assert_eq!(report.attempts[1].verification_runs, 1);
        assert!(report.diagnostics.is_empty());
        assert_eq!(report.status, ResultCategory::Success);
        assert!(report.to_json().contains("\"attempts\":[{"));
    }

    #[test]
    fn query_timing_includes_post_compute_observer_and_publication_boundary() {
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "timing-query.zil",
            "fn value()->i32=>1",
        );
        let options = CompileOptions::default();
        let mut session = CompilationSession::default();
        let mut report = CompileReport::new(&source);
        let key = session::query_key(&source, &options, Phase::Parse, false);
        let raw = session::RawSnapshot::Parse(Arc::new(zil_syntax::parse(&source)));
        let mut observer_nanos = 0;
        let result = session.query(
            key.clone(),
            &[],
            &options,
            &mut report,
            &mut |_| {
                let start = Instant::now();
                std::thread::sleep(Duration::from_millis(2));
                observer_nanos = start.elapsed().as_nanos();
            },
            || Ok(raw),
        );
        assert!(result.is_ok());
        assert!(report.phase_timings_ns[&Phase::Parse] >= observer_nanos);
        assert!(session.database.read(&key).unwrap().is_some());
        assert_eq!(
            report.query_events,
            vec![QueryEvent {
                phase: Phase::Parse,
                disposition: QueryDisposition::Executed
            }]
        );
    }
}
