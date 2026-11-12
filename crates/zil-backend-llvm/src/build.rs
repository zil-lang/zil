use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use zil_base::{CompileResult, Diagnostic, Phase, ResultCategory};
use zil_ir::{FunctionId, LirModule, LirView};
use zil_verify::VerifiedModule;

use crate::object_cache::{
    Access, ObjectCacheSummary, PendingObject, PendingObjectCache, PreparedCache,
};
use crate::{EmitOptions, EntryPoint, FunctionMerging};

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

/// Machine-code strategy is independent from the checked LIR and language rules.
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

/// Opt-in bounded native code generation. One unit preserves the original path.
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

/// Non-overlapping wall-time buckets for this native build. Object work includes
/// cache preparation and unit scheduling; other includes output/path admission
/// and publication. These are measurements, not estimated work or skipped cost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BackendTimings {
    pub machine_ns: u64,
    pub partition_ns: u64,
    pub diagnostic_ns: u64,
    pub object_ns: u64,
    pub link_key_ns: u64,
    pub link_execution_ns: u64,
    pub total_ns: u64,
    pub other_ns: u64,
}
impl BackendTimings {
    fn finish(&mut self, start: std::time::Instant) {
        self.total_ns = crate::object_cache::elapsed_ns(start);
        self.other_ns = self.total_ns.saturating_sub(
            self.machine_ns
                .saturating_add(self.partition_ns)
                .saturating_add(self.diagnostic_ns)
                .saturating_add(self.object_ns)
                .saturating_add(self.link_key_ns)
                .saturating_add(self.link_execution_ns),
        );
    }
    pub fn to_json(&self) -> String {
        format!(
            "{{\"machine_ns\":{},\"partition_ns\":{},\"diagnostic_ns\":{},\"object_ns\":{},\"link_key_ns\":{},\"link_execution_ns\":{},\"other_ns\":{},\"total_ns\":{}}}",
            self.machine_ns,
            self.partition_ns,
            self.diagnostic_ns,
            self.object_ns,
            self.link_key_ns,
            self.link_execution_ns,
            self.other_ns,
            self.total_ns
        )
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodegenSummary {
    pub backend: NativeBackend,
    /// Actual in-process calls, not invented subprocess commands.
    pub object_reports: Vec<String>,
    pub object_cache: ObjectCacheSummary,
    pub requested_units: usize,
    pub actual_units: usize,
    pub jobs: usize,
    pub fallback: Option<String>,
    pub task_abis: Vec<crate::TaskAbiArtifact>,
    pub timings: BackendTimings,
    pub link_tool_identities: crate::InstalledToolIdentitySummary,
}
impl CodegenSummary {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"backend\":{},\"object_reports\":[{}],\"object_cache\":{},\"requested_units\":{},\"actual_units\":{},\"jobs\":{},\"fallback\":{},\"task_abis\":[{}],\"timings\":{},\"link_tool_identities\":{}}}",
            zil_base::json_string(self.backend.name()),
            self.object_reports.join(","),
            self.object_cache.to_json(),
            self.requested_units,
            self.actual_units,
            self.jobs,
            self.fallback
                .as_ref()
                .map_or_else(|| "null".into(), |s| zil_base::json_string(s)),
            self.task_abis
                .iter()
                .map(crate::TaskAbiArtifact::to_json)
                .collect::<Vec<_>>()
                .join(","),
            self.timings.to_json(),
            self.link_tool_identities.to_json()
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildOptions {
    pub backend: NativeBackend,
    pub clang: PathBuf,
    pub output: PathBuf,
    pub optimization: Optimization,
    /// Explicit Linux executable without generated ZIL symbol exports.
    pub closed_executable: bool,
    /// Explicit stable O2 chunks with complete direct-call implementation copies.
    pub incremental_o2: bool,
    /// Explicit O2 pass policy; default preserves the reference pipeline.
    pub function_merging: FunctionMerging,
    pub codegen: CodegenOptions,
    pub emit: EmitOptions,
    pub windows: Option<WindowsLinkOptions>,
    /// Explicit trusted local cache; None preserves the uncached reference.
    pub object_cache: Option<PathBuf>,
}

impl BuildOptions {
    pub fn new(output: impl Into<PathBuf>) -> Self {
        Self {
            backend: NativeBackend::default(),
            clang: PathBuf::from("clang"),
            output: output.into(),
            optimization: Optimization::O0,
            closed_executable: false,
            incremental_o2: false,
            function_merging: FunctionMerging::Disabled,
            codegen: CodegenOptions::default(),
            windows: None,
            object_cache: None,
            emit: EmitOptions {
                entry: EntryPoint::Main,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildArtifact {
    pub executable: PathBuf,
    pub llvm_ir: PathBuf,
    /// The real process argument vector, recorded without shell interpolation.
    pub command: Vec<String>,
    /// Every actual subprocess, including the separate Win64 linker.
    pub commands: Vec<Vec<String>>,
    pub codegen: CodegenSummary,
    /// Pending bytes confer no validation authority and are discarded on failure.
    pub pending_cache: PendingObjectCache,
}
impl BuildArtifact {
    /// Optional private layout destination, pinned before publication guards.
    pub fn pending_partition_cache_path(&self) -> Option<&Path> {
        self.pending_cache
            .layout
            .as_ref()
            .map(|pending| pending.path())
    }
    /// A cache destination conflict must not reject an otherwise valid request.
    pub fn discard_partition_cache(&mut self) {
        self.pending_cache.layout = None;
    }
    /// Call only after all request validation, cancellation and publication steps.
    pub fn commit_object_cache(&mut self) {
        std::mem::take(&mut self.pending_cache).commit(&mut self.codegen.object_cache);
    }
}

/// Compile LLVM IR directly with the configured clang. Construction of any
/// output happens after capability/LIR checks. A failed compiler process leaves
/// no new executable; an earlier successful output is not treated as evidence
/// for this call and is not deleted. Callers must only run an `Ok` artifact.
pub fn build(
    verified: &VerifiedModule,
    lir: &LirModule,
    options: &BuildOptions,
) -> CompileResult<BuildArtifact> {
    let mut artifact = build_with_cancellation(verified, lir, options, &|| false)?;
    artifact.commit_object_cache();
    Ok(artifact)
}

/// Cooperative cancellation does not unwind inside an LLVM pass or linker.
/// It is checked before/after each unit phase and before publishing any result.
/// Embedding drivers must call artifact.commit_object_cache() only after their
/// own final publication succeeds. Dropping the artifact discards pending writes.
pub fn build_with_cancellation(
    verified: &VerifiedModule,
    lir: &LirModule,
    options: &BuildOptions,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> CompileResult<BuildArtifact> {
    build_view_with_cancellation(verified, &lir.as_view(), options, cancelled)
}

/// Borrowed raw LIR uses the same complete binding and machine preflight as the
/// owned API. Sharing removes allocation, never current-request verification.
pub fn build_view_with_cancellation(
    verified: &VerifiedModule,
    lir: &LirView<'_>,
    options: &BuildOptions,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> CompileResult<BuildArtifact> {
    build_view_with_cancellation_and_stable_names(verified, lir, options, cancelled, None)
}

/// Internal-use cross-crate experiment. The optional current-request spelling
/// hint is not a verification capability; existing public callers use no hint.
#[doc(hidden)]
pub fn build_view_with_cancellation_and_stable_names(
    verified: &VerifiedModule,
    lir: &LirView<'_>,
    options: &BuildOptions,
    cancelled: &(dyn Fn() -> bool + Sync),
    stable_names: Option<&[(FunctionId, String)]>,
) -> CompileResult<BuildArtifact> {
    build_view_with_cancellation_and_session(verified, lir, options, cancelled, stable_names, None)
}

/// Session work products remain raw and require the current sealed verifier and
/// exact LIR binding on every request. The caller may checkpoint by cloning.
#[doc(hidden)]
pub fn build_view_with_cancellation_and_session(
    verified: &VerifiedModule,
    lir: &LirView<'_>,
    options: &BuildOptions,
    cancelled: &(dyn Fn() -> bool + Sync),
    stable_names: Option<&[(FunctionId, String)]>,
    mut session: Option<&mut crate::BackendSession>,
) -> CompileResult<BuildArtifact> {
    let build_started = std::time::Instant::now();
    let mut timings = BackendTimings::default();
    if let Some(session) = session.as_deref_mut() {
        session.begin();
    }
    check_cancelled(cancelled)?;
    if options.emit.entry == EntryPoint::None {
        return Err(vec![Diagnostic::error(
            Phase::Link,
            "E-LINK-ENTRY",
            None,
            "native build requires an explicit executable entry wrapper",
        )]);
    }
    if !(1..=64).contains(&options.codegen.units) || !(1..=64).contains(&options.codegen.jobs) {
        return Err(infra(
            "E-CODEGEN-OPTIONS",
            "codegen units and jobs must be in 1..=64",
        ));
    }
    if options.function_merging != FunctionMerging::Disabled
        && options.optimization != Optimization::O2
    {
        return Err(infra(
            "E-CODEGEN-MERGING-OPTIONS",
            "function merging requires O2",
        ));
    }
    if options.backend == NativeBackend::Clang
        && options.function_merging == FunctionMerging::BeforeAndAfter
    {
        return Err(vec![Diagnostic::new(
            Phase::Codegen,
            ResultCategory::Unsupported,
            "E-CODEGEN-MERGING-BACKEND",
            "before-and-after function merging requires the LLVM API backend",
            None,
        )]);
    }
    let machine_started = std::time::Instant::now();
    let (mut machine, collected_closed, mut task_work) = crate::emit::lower_view_with_session(
        verified,
        lir,
        &options.emit,
        options.optimization == Optimization::O2 && options.incremental_o2,
        options.closed_executable
            && options.backend == NativeBackend::LlvmApi
            && options.codegen.units >= 2,
        options.codegen.jobs,
        session.as_deref_mut().map(|session| &mut session.lower),
    )?;
    if let Some(session) = session.as_deref_mut() {
        session.summary.lowered_functions = session.lower.misses;
        session.summary.reused_functions = session.lower.hits;
        session.summary.reused_machine_module = session.lower.reused_machine_module;
        session.summary.whole_machine_input_functions_compared =
            session.lower.whole_machine_input_functions_compared;
        session.summary.source_projection_traversals = session.lower.source_projection_traversals;
        session.summary.source_projection_instruction_visits =
            session.lower.source_projection_instruction_visits;
        session.summary.source_projection_reuses = session.lower.source_projection_reuses;
    }
    let mut task_abis = crate::task_abi::derive(verified, &machine, &mut task_work)?;
    // The opt-in O2 strategy contracts strict checked-add chains before
    // scalar evaluation. Each nonempty plan validates its complete unchanged
    // input before mutation. Ordinary O2 skips both passes.
    let transform_input = session
        .as_ref()
        .and_then(|session| session.lower.identity());
    let reused_transform = session
        .as_ref()
        .and_then(|session| session.transformed.as_ref())
        .filter(|cached| {
            transform_input
                .as_ref()
                .is_some_and(|input| crate::incremental::same_identity(&cached.input, input))
        })
        .cloned();
    if let Some(cached) = reused_transform {
        let fingerprint = machine.fingerprint.clone();
        machine = cached.machine.clone();
        machine.fingerprint = fingerprint;
        if let Some(session) = session.as_deref_mut() {
            session.summary.reused_machine_transforms = true;
        }
    } else {
        if options.optimization == Optimization::O2 && options.incremental_o2 {
            crate::machine_checked_add_chain::fold_checked_add_chains(&mut machine);
            crate::machine_scalar::fold_known_leaf_calls(&mut machine, lir);
        }
        if let (Some(session), Some(input)) = (session.as_deref_mut(), transform_input) {
            session.transformed = Some(std::sync::Arc::new(crate::incremental::TransformProduct {
                input,
                machine: machine.clone(),
            }));
        }
    }
    timings.machine_ns = crate::object_cache::elapsed_ns(machine_started);
    // Source grouping and call-import policy are dependencies in addition to
    // generated machine values. A changed body with equal machine output can
    // stop here only when those partition dependencies also compare equally.
    if options.backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
        return Err(vec![Diagnostic::new(
            Phase::Codegen,
            ResultCategory::Unsupported,
            "E-LLVM-API-UNAVAILABLE",
            "this compiler was built without the llvm-api feature",
            None,
        )]);
    }
    if options.object_cache.is_some() && options.backend != NativeBackend::LlvmApi {
        return Err(infra(
            "E-OBJECT-CACHE-BACKEND",
            "object cache requires the LLVM API backend",
        ));
    }
    if options.closed_executable && lir.target.triple != "x86_64-unknown-linux-gnu" {
        return Err(vec![Diagnostic::new(
            Phase::Codegen,
            ResultCategory::Unsupported,
            "E-CODEGEN-CLOSED-TARGET",
            "closed-executable code generation requires the Linux executable target",
            None,
        )]);
    }
    let partition_started = std::time::Instant::now();
    let partition_input = session.as_ref().map(|session| {
        crate::incremental::PartitionInput::new(
            options,
            stable_names,
            collected_closed,
            lir,
            &machine,
            session.lower.identity(),
        )
    });
    let cached_partition = session
        .as_ref()
        .and_then(|session| session.partition.as_ref())
        .filter(|cached| {
            std::env::var_os(crate::machine_stable_identity::DIAGNOSTIC_ENV).is_none()
                && Some(&cached.input) == partition_input.as_ref()
        })
        .cloned();
    let (plan, llvm, pending_layout, stable_diagnostic, private_closures) = if let Some(cached) =
        cached_partition
    {
        if let Some(session) = session.as_deref_mut() {
            session.summary.reused_partition = true;
            session.summary.reused_partition_units = cached.units.len();
        }
        (
            crate::machine_cgu::Plan {
                units: cached.units.clone(),
                fallback: cached.fallback,
            },
            cached.diagnostic(&verified.fingerprint().to_hex()),
            None,
            None,
            cached.private_closures,
        )
    } else {
        // Only the already admitted scalar ordinary-O2 domain has no imports,
        // closure internalization, closed linkage or global transform plans.
        // Other domains retain the complete original partition path.
        let ordinary_recipe = session
            .as_ref()
            .is_some_and(|session| session.lower.eligible)
            && options.backend == NativeBackend::LlvmApi
            && options.optimization == Optimization::O2
            && !options.incremental_o2
            && !options.closed_executable;
        let recipe_replay = ordinary_recipe
            .then(|| {
                let old = session.as_ref()?.partition.as_ref()?;
                let input = partition_input.as_ref()?;
                if !old.input.same_recipe_environment(input) {
                    return None;
                }
                old.recipe.as_ref()?.replay(&machine, &old.units)
            })
            .flatten();
        let mut stable = if options.closed_executable
            && options.backend == NativeBackend::LlvmApi
            && options.optimization == Optimization::O2
            && options.incremental_o2
            && options.codegen.units >= 2
            && options.emit.entry == EntryPoint::Main
            && lir.target.triple == "x86_64-unknown-linux-gnu"
        {
            stable_names.and_then(|names| {
                if collected_closed {
                    crate::machine_stable_identity::prepare_collected(
                        &machine,
                        lir,
                        names,
                        options.codegen.units,
                        cancelled,
                    )
                } else {
                    crate::machine_stable_identity::prepare(
                        &machine,
                        lir,
                        names,
                        options.codegen.units,
                        cancelled,
                    )
                }
            })
        } else {
            None
        };
        check_cancelled(cancelled)?;
        if let Some(plan) = &mut stable {
            plan.prune_unreachable_closed_definitions(&mut machine);
            plan.rename(&mut machine);
        }
        // Diagnostic/reference output only. Each nonempty transform plan requires
        // complete original-model preflight. Existing per-unit API preflight still runs
        // immediately before its LLVMBuilder calls and cannot be bypassed.
        let diagnostic_started = std::time::Instant::now();
        let llvm = if let Some(session) = session.as_deref_mut() {
            session.render(&machine)
        } else {
            crate::incremental::DiagnosticText::from(machine.to_text())
        };
        timings.diagnostic_ns += crate::object_cache::elapsed_ns(diagnostic_started);
        let mut pending_layout = None;
        let (mut plan, stable_candidates, stable_diagnostic, recipe) =
            if let Some((plan, recipe, rebuilt)) = recipe_replay {
                if let Some(session) = session.as_deref_mut() {
                    session.summary.reused_partition_recipe = true;
                    session.summary.rebuilt_partition_units = rebuilt;
                    session.summary.reused_partition_units =
                        plan.units.len().saturating_sub(rebuilt);
                }
                (plan, None, None, Some(recipe))
            } else if let Some(stable) = stable {
                let trace = std::env::var_os(crate::machine_stable_identity::DIAGNOSTIC_ENV)
                    .map(PathBuf::from);
                let (plan, candidates, diagnostic) = stable.partition(machine, trace);
                (plan, Some(candidates), diagnostic, None)
            } else if collected_closed {
                (
                    crate::machine_cgu::Plan {
                        units: vec![machine],
                        fallback: None,
                    },
                    None,
                    None,
                    None,
                )
            } else if ordinary_recipe {
                let (plan, pending, recipe) = crate::machine_cgu::partition_view_cached_with_recipe(
                    machine,
                    lir,
                    options.emit.entry,
                    options.codegen.units,
                    false,
                    crate::machine_cgu::CallImportPolicy::Disabled,
                    options.object_cache.as_deref(),
                );
                pending_layout = pending;
                (plan, None, None, recipe)
            } else {
                let (plan, pending) = crate::machine_cgu::partition_view_cached(
                    machine,
                    lir,
                    options.emit.entry,
                    options.codegen.units,
                    options.optimization == Optimization::O0 || options.incremental_o2,
                    crate::machine_cgu::CallImportPolicy::for_incremental_o2(
                        options.optimization == Optimization::O2 && options.incremental_o2,
                        options.function_merging,
                    ),
                    (options.backend == NativeBackend::LlvmApi)
                        .then_some(options.object_cache.as_deref())
                        .flatten(),
                );
                pending_layout = pending;
                (plan, None, None, None)
            };
        if let Some(session) = session.as_deref_mut()
            && !session.summary.reused_partition_recipe
        {
            session.summary.rebuilt_partition_units = plan.units.len();
        }
        if options.closed_executable {
            if let Some(candidates) = stable_candidates {
                crate::machine_cgu::internalize_closed_executable_names(
                    &mut plan.units,
                    &candidates,
                );
            } else {
                crate::machine_cgu::internalize_closed_executable(&mut plan.units, lir);
            }
        }
        // Anonymous closure bodies are not named source exports. Decide only after
        // partition so a cross-unit caller/address retains its canonical symbol.
        let private_closures =
            crate::machine_closure_linkage::internalize(&mut plan.units, lir.closures);
        if let (Some(session), Some(input)) = (session.as_deref_mut(), partition_input) {
            session.partition = Some(std::sync::Arc::new(crate::incremental::PartitionProduct {
                input,
                units: plan.units.clone(),
                fallback: plan.fallback,
                diagnostic: llvm.clone(),
                private_closures,
                recipe,
            }));
        }
        (
            plan,
            llvm,
            pending_layout,
            stable_diagnostic,
            private_closures,
        )
    };
    timings.partition_ns =
        crate::object_cache::elapsed_ns(partition_started).saturating_sub(timings.diagnostic_ns);
    let object_setup_started = std::time::Instant::now();
    let mut cache_summary = ObjectCacheSummary {
        requested: options.object_cache.is_some(),
        ..ObjectCacheSummary::default()
    };
    let cache = options.object_cache.as_ref().and_then(|path| {
        match PreparedCache::new(
            path,
            lir.target,
            options.optimization,
            options.function_merging,
            options.incremental_o2
                && options.optimization == Optimization::O2
                && !options.closed_executable
                && lir.target.triple == "x86_64-unknown-linux-gnu",
        ) {
            Ok(cache) => {
                cache_summary.enabled = true;
                Some(cache)
            }
            Err(reason) => {
                cache_summary.disabled_reason = Some(reason);
                None
            }
        }
    });
    if let Some(diagnostic) = stable_diagnostic {
        diagnostic.write(&plan.units, cache.as_ref());
    }
    let mut pending_cache = PendingObjectCache {
        directory: cache.as_ref().map(|c| c.directory.clone()),
        layout: cache
            .as_ref()
            .and(pending_layout)
            .and_then(|pending| pending.pin()),
        ..PendingObjectCache::default()
    };
    let actual_units = plan.units.len().max(1);
    crate::task_abi::bind_codegen(&mut task_abis, &plan.units, options, &mut task_work)?;
    let mut codegen = CodegenSummary {
        backend: options.backend,
        object_reports: Vec::new(),
        object_cache: cache_summary,
        requested_units: options.codegen.units,
        actual_units,
        jobs: options.codegen.jobs.min(actual_units),
        fallback: plan.fallback.map(str::to_owned),
        task_abis,
        timings: BackendTimings::default(),
        link_tool_identities: crate::InstalledToolIdentitySummary::default(),
    };
    timings.object_ns += crate::object_cache::elapsed_ns(object_setup_started);
    let windows = lir.target.triple == "x86_64-pc-windows-msvc";
    if windows != options.windows.is_some() {
        return Err(infra(
            "E-LINK-TARGET-CONFIG",
            "Win64 requires explicit WindowsLinkOptions; Linux must not carry them",
        ));
    }
    let imports = options
        .windows
        .as_ref()
        .map(|config| {
            config
                .import_libraries
                .iter()
                .map(|path| {
                    fs::canonicalize(path)
                        .map_err(|error| io_error("resolve import library", path, error))
                })
                .collect::<CompileResult<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let parent = options
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = options
        .output
        .file_name()
        .ok_or_else(|| infra("E-LINK-OUTPUT", "output must name an executable file"))?;
    fs::create_dir_all(parent)
        .map_err(|error| io_error("create output directory", parent, error))?;
    // Absolute staging arguments cannot be mistaken for clang options, and an
    // absolute artifact path executes the produced file instead of PATH lookup.
    let parent = fs::canonicalize(parent)
        .map_err(|error| io_error("resolve output directory", parent, error))?;
    let final_executable = parent.join(name);
    let staging = Staging::create(&parent)
        .map_err(|error| io_error("create build staging directory", &parent, error))?;
    let ir_path = staging.0.join("module.ll");
    let executable_path = staging.0.join("program");
    let object_path = staging.0.join("module.obj");
    let diagnostic_started = std::time::Instant::now();
    llvm.write(&ir_path)
        .map_err(|error| io_error("write LLVM IR", &ir_path, error))?;
    timings.diagnostic_ns += crate::object_cache::elapsed_ns(diagnostic_started);

    let optimization = match options.optimization {
        Optimization::O0 => "-O0",
        Optimization::O2 => "-O2",
    };
    let mut commands = Vec::new();
    let scan_started = std::time::Instant::now();
    let scoped_threads = lir
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
        .any(|i| matches!(i.kind, zil_ir::InstructionKind::WithScopedThread { .. }));
    timings.machine_ns += crate::object_cache::elapsed_ns(scan_started);
    let object_started = std::time::Instant::now();
    let split = plan.units.len() > 1;
    let objects = if !split && options.backend == NativeBackend::Clang {
        // Keep one clang invocation, but linkage-transformed builds must consume
        // the actual unit rather than the public pre-partition diagnostic text.
        let input = if options.closed_executable || private_closures != 0 {
            let input = staging.0.join("unit-0.ll");
            fs::write(&input, plan.units[0].to_text())
                .map_err(|e| io_error("write transformed codegen unit", &input, e))?;
            input
        } else {
            ir_path.clone()
        };
        drop(plan.units);
        let mut command = Command::new(&options.clang);
        command
            .arg(format!("--target={}", lir.target.triple))
            .arg("-x")
            .arg("ir")
            .arg(optimization);
        clang_function_merging(&mut command, options.function_merging);
        if scoped_threads {
            command.arg("-pthread");
        }
        if windows {
            command.arg("-c");
        }
        command.arg(&input).arg("-o").arg(if windows {
            &object_path
        } else {
            &executable_path
        });
        commands.push(run_command(&mut command, "E-LLVM-COMPILE")?);
        vec![object_path]
    } else {
        let units = plan.units;
        let mut inputs = units
            .into_iter()
            .enumerate()
            .map(|(id, module)| {
                let input = staging.0.join(format!("unit-{id}.ll"));
                let object = staging.0.join(format!("unit-{id}.o"));
                // Only the reference backend serializes individual codegen units.
                if options.backend == NativeBackend::Clang {
                    fs::write(&input, module.to_text())
                        .map_err(|e| io_error("write codegen unit", &input, e))?;
                }
                Ok(UnitInput {
                    input,
                    object,
                    module,
                    key: None,
                    preflight: None,
                    retain_preflight: session.is_some(),
                })
            })
            .collect::<CompileResult<Vec<_>>>()?;
        if let (Some(session), Some(cache)) = (session.as_deref_mut(), cache.as_ref()) {
            let mut next = Vec::with_capacity(inputs.len());
            let same_partition = session
                .key_partition
                .as_ref()
                .zip(session.partition.as_ref())
                .is_some_and(|(previous, current)| std::sync::Arc::ptr_eq(previous, current));
            for (index, unit) in inputs.iter_mut().enumerate() {
                let old = if same_partition {
                    session
                        .keys
                        .get(index)
                        .filter(|entry| entry.directory == cache.directory)
                } else {
                    session.keys.iter().find(|entry| {
                        entry.directory == cache.directory
                            && entry.module.triple == unit.module.triple
                            && entry.module.items == unit.module.items
                    })
                };
                let product = if let Some(old) = old {
                    session.summary.reused_object_keys += 1;
                    old.clone()
                } else {
                    std::sync::Arc::new(crate::incremental::KeyProduct {
                        directory: cache.directory.clone(),
                        module: unit.module.clone(),
                        key: cache.key(&unit.module),
                    })
                };
                unit.key = Some(product.key);
                next.push(product);
            }
            session.keys = next;
            session.key_partition = session.partition.clone();
        }
        if let Some(session) = session.as_ref() {
            for unit in &mut inputs {
                if !unit.retain_preflight {
                    continue;
                }
                unit.preflight = session
                    .preflights
                    .iter()
                    .find(|product| {
                        product.module.triple == unit.module.triple
                            && product.module.items == unit.module.items
                    })
                    .map(|product| product.receipt.clone());
            }
        }
        let compiled = compile_units(
            &inputs,
            &lir.target.triple,
            optimization,
            options,
            cache.as_ref(),
            cancelled,
        )?;
        let mut next_preflights = Vec::new();
        for (unit, result) in inputs.iter().zip(compiled) {
            if let Some(receipt) = result.preflight.or_else(|| unit.preflight.clone()) {
                next_preflights.push(std::sync::Arc::new(crate::incremental::PreflightProduct {
                    module: unit.module.clone(),
                    receipt,
                }));
            }
            commands.extend(result.commands);
            if let Some(receipt) = result.receipt {
                codegen.object_reports.push(receipt);
            }
            if let Some(access) = result.access {
                codegen.object_cache.add(&access);
            }
            if let Some(pending) = result.pending {
                pending_cache.objects.push(pending);
            }
        }
        if let Some(session) = session.as_deref_mut() {
            session.preflights = next_preflights;
        }
        inputs
            .into_iter()
            .map(|unit| unit.object)
            .collect::<Vec<_>>()
    };
    check_cancelled(cancelled)?;
    timings.object_ns += crate::object_cache::elapsed_ns(object_started);
    let link_key_started = std::time::Instant::now();
    let link_key = session
        .as_deref_mut()
        .filter(|_| options.backend == NativeBackend::LlvmApi && options.windows.is_none())
        .and_then(|session| {
            crate::link_cache::key(
                &options.clang,
                &lir.target.triple,
                optimization,
                &objects,
                &executable_path,
                scoped_threads,
                &mut crate::link_cache::DiscoveryContext {
                    commands: &mut commands,
                    tools: &mut session.link_tools,
                },
            )
        });
    timings.link_key_ns += crate::object_cache::elapsed_ns(link_key_started);
    let previous_link = session
        .as_ref()
        .and_then(|session| session.link.as_ref())
        .filter(|product| Some(product.key) == link_key && codegen.object_cache.enabled)
        .cloned();
    let restoring_link = previous_link.is_some();
    let link_started = std::time::Instant::now();
    if let Some(product) = previous_link {
        check_cancelled(cancelled)?;
        product
            .restore(&executable_path)
            .map_err(|error| io_error("restore final link result", &executable_path, error))?;
        if let Some(session) = session.as_deref_mut() {
            session.summary.reused_link = true;
        }
    } else {
        if let Some(config) = &options.windows {
            let mut link = Command::new(&config.linker);
            link.args([
                "/machine:x64",
                "/subsystem:windows",
                "/entry:zil.pe.entry",
                "/nodefaultlib",
                "/timestamp:0",
            ])
            .arg(format!("/out:{}", executable_path.display()))
            .args(&objects)
            .args(imports);
            commands.push(run_command(&mut link, "E-WIN64-LINK")?);
        } else if split || options.backend == NativeBackend::LlvmApi {
            let mut link = Command::new(&options.clang);
            link.arg(format!("--target={}", lir.target.triple))
                .arg(optimization)
                .args(&objects)
                .arg("-o")
                .arg(&executable_path);
            if scoped_threads {
                link.arg("-pthread");
            }
            commands.push(run_command(&mut link, "E-LLVM-COMPILE")?);
        }

        timings.link_execution_ns += crate::object_cache::elapsed_ns(link_started);
        if let (Some(session), Some(key)) = (session.as_deref_mut(), link_key) {
            let link_key_started = std::time::Instant::now();
            // Recheck discovered external values after the subprocess before
            // retaining its product; a concurrently changed dependency is red.
            let current_key = crate::link_cache::key(
                &options.clang,
                &lir.target.triple,
                optimization,
                &objects,
                &executable_path,
                scoped_threads,
                &mut crate::link_cache::DiscoveryContext {
                    commands: &mut commands,
                    tools: &mut session.link_tools,
                },
            );
            timings.link_key_ns += crate::object_cache::elapsed_ns(link_key_started);
            if current_key == Some(key) {
                let capture_started = std::time::Instant::now();
                session.link = crate::link_cache::Product::capture(key, &executable_path)
                    .map(std::sync::Arc::new);
                timings.link_execution_ns += crate::object_cache::elapsed_ns(capture_started);
            }
        }
    }
    if restoring_link {
        timings.link_execution_ns = crate::object_cache::elapsed_ns(link_started);
    }
    check_cancelled(cancelled)?;
    let recorded_command = commands.first().cloned().unwrap_or_default();
    if !executable_path.is_file() {
        return Err(infra(
            "E-LINK-MISSING-OUTPUT",
            "LLVM clang reported success without producing an executable",
        ));
    }
    check_cancelled(cancelled)?;
    let mut ir_name = name.to_os_string();
    ir_name.push(".ll");
    let final_ir = parent.join(ir_name);
    fs::rename(&ir_path, &final_ir)
        .map_err(|error| io_error("publish LLVM IR", &final_ir, error))?;
    fs::rename(&executable_path, &final_executable)
        .map_err(|error| io_error("publish executable", &final_executable, error))?;
    drop(staging);
    timings.finish(build_started);
    codegen.timings = timings;
    if let Some(session) = session {
        codegen.link_tool_identities = session.link_tools.summary.clone();
    }
    Ok(BuildArtifact {
        executable: final_executable,
        llvm_ir: final_ir,
        command: recorded_command,
        commands,
        codegen,
        pending_cache,
    })
}

/// Largest input first reduces tail latency. Workers return results by unit id,
/// so errors/command records are deterministic rather than completion-ordered.
/// All started processes finish before staging is removed or output published.
struct UnitInput {
    input: PathBuf,
    object: PathBuf,
    module: zil_llvm_api::model::Module,
    key: Option<zil_base::Fingerprint>,
    preflight: Option<std::sync::Arc<zil_llvm_api::PreflightReceipt>>,
    retain_preflight: bool,
}
struct UnitResult {
    commands: Vec<Vec<String>>,
    receipt: Option<String>,
    access: Option<Access>,
    pending: Option<PendingObject>,
    preflight: Option<std::sync::Arc<zil_llvm_api::PreflightReceipt>>,
}

fn compile_units(
    inputs: &[UnitInput],
    target: &str,
    optimization: &str,
    options: &BuildOptions,
    cache: Option<&PreparedCache>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> CompileResult<Vec<UnitResult>> {
    let mut schedule = (0..inputs.len()).collect::<Vec<_>>();
    schedule.sort_by_key(|id| {
        (
            std::cmp::Reverse(
                inputs[*id]
                    .module
                    .items
                    .iter()
                    .map(|item| match item {
                        zil_llvm_api::model::Item::Function(f) => f.cost(),
                        _ => 1,
                    })
                    .sum::<usize>(),
            ),
            *id,
        )
    });
    let next = AtomicUsize::new(0);
    let results = std::thread::scope(|scope| {
        let workers = (0..options.codegen.jobs.min(inputs.len()))
            .map(|_| {
                let schedule = &schedule;
                let next = &next;
                scope.spawn(move || {
                    let mut results = Vec::new();
                    loop {
                        let slot = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&id) = schedule.get(slot) else {
                            break;
                        };
                        let result = check_cancelled(cancelled).and_then(|()| {
                            let unit = &inputs[id];
                            match options.backend {
                                NativeBackend::Clang => {
                                    let mut command = Command::new(&options.clang);
                                    command
                                        .arg(format!("--target={target}"))
                                        .args(["-x", "ir", optimization, "-c"])
                                        .arg(&unit.input)
                                        .arg("-o")
                                        .arg(&unit.object);
                                    clang_function_merging(&mut command, options.function_merging);
                                    let command = run_command(&mut command, "E-LLVM-COMPILE")?;
                                    if !unit.object.is_file() {
                                        return Err(infra(
                                            "E-LINK-MISSING-OUTPUT",
                                            "LLVM did not produce a codegen object",
                                        ));
                                    }
                                    Ok(UnitResult {
                                        commands: vec![command],
                                        receipt: None,
                                        access: None,
                                        pending: None,
                                        preflight: None,
                                    })
                                }
                                NativeBackend::LlvmApi => compile_api_unit(
                                    unit,
                                    target,
                                    options.optimization,
                                    options.function_merging,
                                    cache,
                                    cancelled,
                                ),
                            }
                        });
                        results.push((id, result));
                    }
                    results
                })
            })
            .collect::<Vec<_>>();
        let mut results = Vec::new();
        let mut panicked = false;
        for worker in workers {
            match worker.join() {
                Ok(rows) => results.extend(rows),
                Err(_) => panicked = true,
            }
        }
        if panicked {
            Err(infra("E-CODEGEN-WORKER", "native codegen worker panicked"))
        } else {
            Ok(results)
        }
    })?;
    let mut results = results;
    results.sort_by_key(|(id, _)| *id);
    results.into_iter().map(|(_, result)| result).collect()
}

fn check_cancelled(cancelled: &(dyn Fn() -> bool + Sync)) -> CompileResult<()> {
    if cancelled() {
        Err(vec![Diagnostic::new(
            Phase::Link,
            ResultCategory::Cancelled,
            "E-LLVM-API-CANCELLED",
            "native build cancelled before publication",
            None,
        )])
    } else {
        Ok(())
    }
}

#[cfg(feature = "llvm-api")]
fn api_error(error: zil_llvm_api::Error) -> Vec<Diagnostic> {
    use zil_llvm_api::ErrorKind;
    let (code, category) = match error.kind {
        ErrorKind::Cancelled => ("E-LLVM-API-CANCELLED", ResultCategory::Cancelled),
        ErrorKind::Unavailable => ("E-LLVM-API-UNAVAILABLE", ResultCategory::Unsupported),
        ErrorKind::Version => ("E-LLVM-API-VERSION", ResultCategory::InfraError),
        ErrorKind::Target => ("E-LLVM-API-TARGET", ResultCategory::InfraError),
        ErrorKind::Parse => ("E-LLVM-API-PARSE", ResultCategory::InfraError),
        ErrorKind::Verify => ("E-LLVM-API-VERIFY", ResultCategory::InfraError),
        ErrorKind::Optimize => ("E-LLVM-API-OPTIMIZE", ResultCategory::InfraError),
        ErrorKind::Emit => ("E-LLVM-API-EMIT", ResultCategory::InfraError),
    };
    vec![Diagnostic::new(
        Phase::Link,
        category,
        code,
        error.to_string(),
        None,
    )]
}

#[cfg(feature = "llvm-api")]
fn compile_api_unit(
    unit: &UnitInput,
    target: &str,
    opt: Optimization,
    function_merging: FunctionMerging,
    cache: Option<&PreparedCache>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> CompileResult<UnitResult> {
    use zil_llvm_api::{Config, Optimization as ApiOpt};
    let config = Config {
        triple: target,
        optimization: match opt {
            Optimization::O0 => ApiOpt::O0,
            Optimization::O2 => ApiOpt::O2,
        },
    };
    let mut access = None;
    if let Some(cache) = cache {
        let (mut lookup, bytes) = cache.lookup_with_key(&unit.module, unit.key);
        check_cancelled(cancelled)?;
        if let Some(bytes) = bytes {
            // A hit is not a native Builder invocation. The independent
            // checker either checks this current model or binds its own prior
            // proof to the complete current immutable input. Object keys do
            // not confer that authority.
            let t = std::time::Instant::now();
            let (proof, reused) =
                zil_llvm_api::validate_module_with_receipt(&unit.module, unit.preflight.as_deref())
                    .map_err(api_error)?;
            let preflight = std::sync::Arc::new(proof);
            lookup.preflight_reused = reused;
            lookup.preflight_ns = crate::object_cache::elapsed_ns(t);
            check_cancelled(cancelled)?;
            fs::write(&unit.object, &bytes)
                .map_err(|e| io_error("write reused object", &unit.object, e))?;
            let q = zil_base::json_string;
            let receipt = format!(
                "{{\"backend\":\"llvm-api\",\"input_kind\":\"object-cache\",\"llvm_version\":{},\"triple\":{},\"pipeline\":{},\"object_bytes\":{},\"cache\":{},\"llvm_verification_this_request\":false,\"producer_verified_before_after\":true,\"current_machine_preflight_reused\":{},\"preflight_ns\":{},\"builder_ns\":0,\"parse_ns\":0,\"optimize_ns\":0,\"emit_ns\":0,\"total_ns\":{}}}",
                q(&cache.llvm_version),
                q(target),
                q(zil_llvm_api::pipeline_identity(
                    config.optimization,
                    function_merging
                )),
                bytes.len(),
                lookup.to_json(),
                reused,
                lookup.preflight_ns,
                lookup.key_ns + lookup.lookup_ns + lookup.preflight_ns
            );
            return Ok(UnitResult {
                commands: Vec::new(),
                receipt: Some(receipt),
                access: Some(lookup),
                pending: None,
                preflight: Some(preflight),
            });
        }
        access = Some(lookup);
    }
    let snapshot = cache
        .filter(|cache| cache.patch_policy.is_some())
        .and_then(|_| crate::machine_object_patch::Snapshot::new(&unit.module));
    let mut commands = Vec::new();
    let mut patch_fallback = None;
    if let (Some(cache), Some(snapshot), Some(lookup)) = (cache, snapshot.as_ref(), access.as_ref())
        && let Some(record) = cache.patch_record(snapshot.index_key)
        && let Some(plan) = snapshot.plan(&unit.module, &record)
        && let Some(base) = cache.base_object(plan.base_key)
    {
        // The old object is raw cached data. Check the entire current model,
        // including unchanged definitions, before deriving any current product.
        let t = std::time::Instant::now();
        let (preflight, preflight_reused) = if unit.retain_preflight {
            let (receipt, reused) =
                zil_llvm_api::validate_module_with_receipt(&unit.module, unit.preflight.as_deref())
                    .map_err(api_error)?;
            (Some(std::sync::Arc::new(receipt)), reused)
        } else {
            zil_llvm_api::validate_module(&unit.module).map_err(api_error)?;
            (None, false)
        };
        let current_preflight = crate::object_cache::elapsed_ns(t);
        check_cancelled(cancelled)?;
        match zil_llvm_api::compile_module_with_function_merging(
            &plan.module,
            &config,
            function_merging,
            cancelled,
        ) {
            Ok(patch) => {
                check_cancelled(cancelled)?;
                let t = std::time::Instant::now();
                let attempt = plan.derive(
                    cache.patch_policy.as_ref().expect("eligible patch policy"),
                    &base,
                    &patch.bytes,
                    &unit.object,
                    lookup.key,
                );
                let derive_ns = crate::object_cache::elapsed_ns(t);
                commands.extend(attempt.commands);
                check_cancelled(cancelled)?;
                if let Some(bytes) = attempt.bytes {
                    let mut lookup = access.take().expect("current cache action");
                    lookup.preflight_ns = current_preflight;
                    let q = zil_base::json_string;
                    let r = &patch.receipt;
                    let receipt = format!(
                        "{{\"backend\":\"llvm-api\",\"input_kind\":\"object-patch\",\"llvm_version\":{},\"triple\":{},\"pipeline\":{},\"object_bytes\":{},\"patched_functions\":{},\"reused_functions\":{},\"base_action_key\":{},\"llvm_verification_scope\":\"current-patch\",\"base_producer_verified_before_after\":true,\"current_full_machine_preflight\":true,\"current_full_machine_preflight_reused\":{},\"verify_before_after\":true,\"cache\":{},\"preflight_ns\":{},\"builder_ns\":{},\"parse_ns\":0,\"optimize_ns\":{},\"emit_ns\":{},\"derive_ns\":{},\"total_ns\":{}}}",
                        q(&r.llvm_version),
                        q(target),
                        q(&r.pipeline),
                        bytes.len(),
                        plan.replacements.len(),
                        plan.reused_functions,
                        q(&plan.base_key.to_hex()),
                        preflight_reused,
                        lookup.to_json(),
                        current_preflight + r.preflight_ns,
                        r.builder_ns,
                        r.optimize_ns,
                        r.emit_ns,
                        derive_ns,
                        current_preflight + r.total_ns + derive_ns
                    );
                    let pending = PendingObject {
                        key: lookup.key,
                        bytes,
                        base: None,
                    };
                    return Ok(UnitResult {
                        commands,
                        receipt: Some(receipt),
                        access: Some(lookup),
                        pending: Some(pending),
                        preflight,
                    });
                }
                patch_fallback = attempt.reason;
            }
            Err(error) => {
                check_cancelled(cancelled)?;
                patch_fallback = Some(error.to_string());
            }
        }
    }
    // The opt-in receipt entry point signs the same successful native
    // preflight that constructs the Builder schedule. It adds no second check,
    // and only returns a proof together with a fully verified emitted object.
    let (object, preflight) = if unit.retain_preflight {
        let (object, receipt) = zil_llvm_api::compile_module_with_preflight_receipt(
            &unit.module,
            &config,
            function_merging,
            cancelled,
        )
        .map_err(api_error)?;
        (object, Some(std::sync::Arc::new(receipt)))
    } else {
        let object = zil_llvm_api::compile_module_with_function_merging(
            &unit.module,
            &config,
            function_merging,
            cancelled,
        )
        .map_err(api_error)?;
        (object, None)
    };
    check_cancelled(cancelled)?;
    fs::write(&unit.object, &object.bytes)
        .map_err(|e| io_error("write LLVM API object", &unit.object, e))?;
    let r = &object.receipt;
    let q = zil_base::json_string;
    let mut receipt = format!(
        "{{\"backend\":\"llvm-api\",\"llvm_version\":{},\"triple\":{},\"cpu\":{},\"features\":{},\"data_layout\":{},\"pipeline\":{},\"relocation\":{},\"code_model\":{},\"verify_before_after\":{},\"object_bytes\":{},\"input_kind\":{},\"preflight_ns\":{},\"builder_ns\":{},\"parse_ns\":{},\"optimize_ns\":{},\"emit_ns\":{},\"total_ns\":{}}}",
        q(&r.llvm_version),
        q(&r.triple),
        q(&r.cpu),
        q(&r.features),
        q(&r.data_layout),
        q(&r.pipeline),
        q(&r.relocation),
        q(&r.code_model),
        r.verify_before_after,
        object.bytes.len(),
        q(&r.input_kind),
        r.preflight_ns,
        r.builder_ns,
        r.parse_ns,
        r.optimize_ns,
        r.emit_ns,
        r.total_ns
    );

    if let Some(access) = &access {
        receipt.pop();
        receipt.push_str(",\"cache\":");
        receipt.push_str(&access.to_json());
        receipt.push('}');
    }
    if let Some(reason) = patch_fallback {
        receipt.pop();
        receipt.push_str(",\"patch_fallback\":");
        receipt.push_str(&q(&reason));
        receipt.push('}');
    }
    let pending = access.as_ref().map(|a| {
        let base = snapshot.map(|s| {
            (
                s.index_key,
                s.encode_base(a.key, zil_base::Fingerprint::of(&object.bytes)),
            )
        });
        PendingObject {
            key: a.key,
            bytes: object.bytes,
            base,
        }
    });
    Ok(UnitResult {
        commands,
        receipt: Some(receipt),
        access,
        pending,
        preflight,
    })
}
#[cfg(not(feature = "llvm-api"))]
fn compile_api_unit(
    _unit: &UnitInput,
    _target: &str,
    _opt: Optimization,
    _function_merging: FunctionMerging,
    _cache: Option<&PreparedCache>,
    _cancelled: &(dyn Fn() -> bool + Sync),
) -> CompileResult<UnitResult> {
    Err(infra(
        "E-LLVM-API-UNAVAILABLE",
        "LLVM API support was not compiled in",
    ))
}

fn clang_function_merging(command: &mut Command, policy: FunctionMerging) {
    if policy == FunctionMerging::AfterOptimization {
        command.args([
            "-mllvm",
            "-enable-merge-functions=true",
            "-mllvm",
            "-mergefunc-use-aliases=false",
        ]);
    }
}

fn run_command(command: &mut Command, code: &str) -> CompileResult<Vec<String>> {
    let recorded = std::iter::once(command.get_program().to_string_lossy().into_owned())
        .chain(
            command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned()),
        )
        .collect();
    let output = command.output().map_err(|error| {
        io_error(
            "run native build tool",
            Path::new(command.get_program()),
            error,
        )
    })?;
    if !output.status.success() {
        let excerpt = String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(16_384)
            .collect::<String>();
        return Err(vec![
            Diagnostic::new(
                Phase::Link,
                ResultCategory::InfraError,
                code,
                format!("native build tool failed with {}", output.status),
                None,
            )
            .with_note(excerpt),
        ]);
    }
    Ok(recorded)
}

static BUILD_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Staging(PathBuf);

impl Staging {
    fn create(parent: &Path) -> io::Result<Self> {
        for _ in 0..100 {
            let sequence = BUILD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".zil-build-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cannot allocate a unique build staging directory",
        ))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn infra(code: &str, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::new(
        Phase::Link,
        ResultCategory::InfraError,
        code,
        message,
        None,
    )]
}
fn io_error(operation: &str, path: &Path, error: io::Error) -> Vec<Diagnostic> {
    infra(
        "E-LINK-IO",
        format!("{operation} ({}): {error}", path.display()),
    )
}
