use super::artifacts::ArtifactPlan;
use super::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use zil_backend_llvm::{BuildOptions, EmitOptions, EntryPoint};
use zil_base::{
    CompileResult, Diagnostic, Fingerprint, Limits, Phase, RULE_VERSION, ResultCategory,
    SCHEMA_VERSION, SourceUnit,
};
use zil_db::{
    ArtifactCache, CacheRead, CacheWriteError, Database, Dependency, FingerprintKind, QueryKey,
    QuerySnapshot, ResultFingerprints,
};
use zil_ir::{CandidatePlan, CoreModule, Type, TypedModule, TypedProgram};
use zil_syntax::ParseResult;

/// Only unverified, immutable data can be retained across requests. There is no
/// VerifiedModule/VerifiedCTIR variant and no disk deserializer for this enum.
#[derive(Clone, Debug)]
pub(super) enum RawSnapshot {
    Parse(Arc<ParseResult>),
    Expanded(Arc<ExpandedSource>),
    Core(Arc<CoreModule>),
    Typed(Arc<TypedModule>),
    Candidate(Arc<CandidatePlan>),
    Project(Arc<super::project::ExpandedProject>),
    ProjectCore(Arc<zil_ir::CoreProgram>),
    ProjectTyped(Arc<zil_ir::TypedProgram>),
}
/// Shared raw ownership never carries verification authority. Each request
/// independently checks the embedded module with its current limits and target.
pub(super) enum TypedInput {
    Module(Arc<TypedModule>),
    Program(Arc<TypedProgram>),
}
impl TypedInput {
    fn module(&self) -> &TypedModule {
        match self {
            Self::Module(m) => m,
            Self::Program(p) => &p.module,
        }
    }
    fn program(&self) -> Option<&TypedProgram> {
        match self {
            Self::Module(_) => None,
            Self::Program(p) => Some(p),
        }
    }
    fn verify(
        &self,
        candidate: Arc<CandidatePlan>,
        options: &CompileOptions,
    ) -> CompileResult<zil_verify::VerifiedModule> {
        match self {
            Self::Module(m) => zil_verify::verify_shared_module(
                m.clone(),
                candidate,
                &options.target,
                &options.limits,
            ),
            Self::Program(p) => zil_verify::verify_shared_program_module(
                p.clone(),
                candidate,
                &options.target,
                &options.limits,
            ),
        }
    }
}
impl RawSnapshot {
    /// Identity of a deterministic, private, process-local construction. This is
    /// NOT a content checksum or a verifier capability. Its complete query key
    /// binds source bytes/positions, rules, every limit and target; ordered
    /// dependency identities bind the exact upstream constructions observed.
    /// Raw snapshots cannot be deserialized or injected through the public API.
    /// The public Typed/Verified fingerprints and independent Verify are intact.
    fn digest(&self, key: &QueryKey, dependencies: &[Arc<QuerySnapshot>]) -> Fingerprint {
        if let Self::Typed(typed) = self {
            return typed.fingerprint();
        }
        let tag = match self {
            Self::Parse(_) => 0,
            Self::Expanded(_) => 1,
            Self::Core(_) => 2,
            Self::Candidate(_) => 3,
            Self::Project(_) => 4,
            Self::ProjectCore(_) => 5,
            Self::ProjectTyped(_) => 6,
            Self::Typed(_) => unreachable!(),
        };
        let mut bytes = b"ZIL-PRIVATE-CONSTRUCTION-150\0".to_vec();
        bytes.push(tag);
        bytes.extend_from_slice(&key.fingerprint().0);
        bytes.extend_from_slice(&(dependencies.len() as u64).to_le_bytes());
        for dependency in dependencies {
            bytes.extend_from_slice(&dependency.key.fingerprint().0);
            bytes.extend_from_slice(&dependency.fingerprints.private_implementation.0);
        }
        Fingerprint::of(&bytes)
    }
}

#[derive(Clone, Debug)]
pub(super) struct ExpandedSource {
    module: zil_expand::ExpandedModule,
    selection: super::ct_selection::Receipt,
}

pub struct CompilationSession {
    pub(super) database: Database,
    pub(super) raw: BTreeMap<QueryKey, RawSnapshot>,
    pub(super) capacity: usize,
    recency: BTreeMap<QueryKey, u64>,
    clock: u64,
}
impl Default for CompilationSession {
    fn default() -> Self {
        Self::with_capacity(32)
    }
}
impl CompilationSession {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            database: Database::default(),
            raw: BTreeMap::new(),
            capacity: capacity.max(8),
            recency: BTreeMap::new(),
            clock: 0,
        }
    }
    pub fn clear(&mut self) {
        self.database.clear();
        self.raw.clear();
        self.recency.clear();
        self.clock = 0;
    }
    pub fn cached_query_count(&self) -> usize {
        self.raw.len()
    }
    pub fn active_query_count(&self) -> usize {
        self.database.active_count()
    }
    /// Successful syntax is independent of preceding modules' consumption.
    /// Insufficient receipts use the original quota-specific query/parser so
    /// the first error, source span and partial work remain unchanged.
    pub(super) fn parse_key(&self, source: &SourceUnit, options: &CompileOptions) -> QueryKey {
        let key = parse_identity(source, options);
        if matches!(self.raw.get(&key), Some(RawSnapshot::Parse(parsed)) if !parsed.fits_limits(&options.limits))
        {
            query_key(source, options, Phase::Parse, false)
        } else {
            key
        }
    }
    pub fn compile_path(&mut self, path: &Path, options: &CompileOptions) -> CompileReport {
        self.compile_project_path(path, options)
    }
    pub fn compile_source(
        &mut self,
        source: &SourceUnit,
        options: &CompileOptions,
    ) -> CompileReport {
        self.compile_source_with_observer(source, options, &mut |_| {})
    }
    /// An observer sees phase scheduling, not candidate values or a verifier
    /// capability. Editors may use it for progress and cooperative cancellation.
    pub fn compile_source_with_observer(
        &mut self,
        source: &SourceUnit,
        options: &CompileOptions,
        observer: &mut dyn FnMut(&QueryEvent),
    ) -> CompileReport {
        let start = Instant::now();
        let mut report = CompileReport::new(source);
        if let Err(errors) = self.pipeline(source, options, &mut report, observer) {
            report.fail(errors);
        }
        self.trim_history();
        report.seconds = start.elapsed().as_secs_f64();
        report
    }

    fn touch(&mut self, key: &QueryKey) {
        self.clock = self.clock.saturating_add(1);
        self.recency.insert(key.clone(), self.clock);
    }

    /// Bound retained raw snapshots without deleting the current query graph.
    /// Dependency metadata is retained only while reachable from a raw root.
    /// Eviction occurs between requests, never while borrowed inputs execute.
    pub(super) fn trim_history(&mut self) {
        if self.raw.len() <= self.capacity {
            return;
        }
        let mut oldest = self
            .recency
            .iter()
            .map(|(key, &age)| (age, key.clone()))
            .collect::<Vec<_>>();
        oldest.sort_unstable();
        for (_, key) in oldest.into_iter().take(self.raw.len() - self.capacity) {
            self.raw.remove(&key);
            self.recency.remove(&key);
        }
        // The driver closes/aborts every query before a request returns. If a
        // future path violates that invariant, preserve state rather than GC an
        // in-progress dependency; active-query controls detect that defect.
        self.database.retain_reachable(self.raw.keys());
    }

    pub(super) fn query(
        &mut self,
        key: QueryKey,
        dependencies: &[Arc<QuerySnapshot>],
        options: &CompileOptions,
        report: &mut CompileReport,
        observer: &mut dyn FnMut(&QueryEvent),
        execute: impl FnOnce() -> CompileResult<RawSnapshot>,
    ) -> CompileResult<(RawSnapshot, Arc<QuerySnapshot>)> {
        options
            .cancellation
            .check(key.phase)
            .map_err(|error| vec![error])?;
        if let Some(snapshot) = self.database.read(&key).map_err(|error| vec![error])?
            && let Some(raw) = self.raw.get(&key).cloned()
        {
            event(key.phase, QueryDisposition::Reused, report, observer);
            options
                .cancellation
                .check(key.phase)
                .map_err(|error| vec![error])?;
            report.cache_hit = true;
            self.touch(&key);
            return Ok((raw, snapshot));
        }
        self.database
            .begin(key.clone())
            .map_err(|error| vec![error])?;
        let result = execute();
        event(key.phase, QueryDisposition::Executed, report, observer);
        let raw = match result {
            Ok(raw) => raw,
            Err(errors) => {
                self.database.abort(&key);
                return Err(errors);
            }
        };
        if let Err(error) = options.cancellation.check(key.phase) {
            self.database.abort(&key);
            return Err(vec![error]);
        }
        let digest = raw.digest(&key, dependencies);
        // The current whole-module raw queries conservatively observe complete
        // implementation identity. Kind-selective graph support is separate;
        // this does not claim cross-module private-body incremental checking.
        let fingerprints = ResultFingerprints {
            semantic: digest,
            abi: key.target,
            private_implementation: digest,
            source_positions: key.input,
            build_inputs: options.build_inputs,
        };
        let dependencies = dependencies
            .iter()
            .map(|input| Dependency {
                key: input.key.clone(),
                kind: FingerprintKind::PrivateImplementation,
                observed: input.fingerprints.private_implementation,
            })
            .collect();
        let snapshot = self
            .database
            .publish(
                key.clone(),
                ResultCategory::Success,
                fingerprints,
                dependencies,
                digest.0.to_vec(),
            )
            .map_err(|error| vec![error])?;
        self.touch(&key);
        self.raw.insert(key, raw.clone());
        Ok((raw, snapshot))
    }

    fn pipeline(
        &mut self,
        source: &SourceUnit,
        options: &CompileOptions,
        report: &mut CompileReport,
        observer: &mut dyn FnMut(&QueryEvent),
    ) -> CompileResult<()> {
        options
            .cancellation
            .check(Phase::Driver)
            .map_err(|error| vec![error])?;
        if options.profile == ExecutionProfile::Managed {
            return Err(vec![Diagnostic::unsupported(
                Phase::Driver,
                "managed execution profile and host bridge are not implemented",
                None,
            )]);
        }
        if source.text.len() > options.limits.max_source_bytes {
            return Err(vec![Diagnostic::limit(
                Phase::Parse,
                "source byte budget exceeded",
                Some(source.span()),
            )]);
        }
        let artifacts = ArtifactPlan::new(source, options)?;
        let identity = query_key(source, options, Phase::Parse, false);
        disk_source(source, options, report, &identity)?;
        // These immutable identities describe this request, not a proof. Reuse
        // them vertically instead of rehashing source bytes for every phase.
        let target = options.target.fingerprint();
        let key = |phase, depends_on_target| phase_key(&identity, phase, depends_on_target, target);
        let parse_key = self.parse_key(source, options);
        let (parsed, parse_snapshot) =
            self.query(parse_key.clone(), &[], options, report, observer, || {
                let parsed = zil_syntax::parse_with_limits(source, &options.limits);
                if !parsed.diagnostics.is_empty() {
                    return Err(parsed.diagnostics);
                }
                Ok(RawSnapshot::Parse(Arc::new(parsed)))
            })?;
        let RawSnapshot::Parse(parsed) = parsed else {
            return Err(infra(
                "E-QUERY-KIND",
                "parse query returned another snapshot kind",
            ));
        };
        report.phase = Phase::Parse;
        artifacts.dump_lazy(report, options, "syntax", |_| format!("{parsed:#?}"))?;
        if options.stop_after == Stage::Parse {
            return Ok(());
        }

        let selection_needed = super::ct_selection::needed(source, &parsed);
        let (core, core_snapshot) = self.query(
            key(Phase::Expand, selection_needed),
            std::slice::from_ref(&parse_snapshot),
            options,
            report,
            observer,
            || {
                let selected = if selection_needed {
                    Some(super::ct_selection::select(
                        std::slice::from_ref(source),
                        &[&parsed.module],
                        &[Vec::new()],
                        source.module_id,
                        source.fingerprint(),
                        &options.target,
                        &options.limits,
                    )?)
                } else {
                    None
                };
                let syntax = selected
                    .as_ref()
                    .map_or(&parsed.module, |selected| &selected.modules[0]);
                let expanded = zil_expand::expand_with_map_limits(source, syntax, &options.limits)?;
                expanded
                    .source_map
                    .validate(std::slice::from_ref(source), &options.limits)?;
                Ok(RawSnapshot::Expanded(Arc::new(ExpandedSource {
                    module: expanded,
                    selection: selected
                        .map_or_else(super::ct_selection::Receipt::default, |selected| {
                            selected.receipt
                        }),
                })))
            },
        )?;
        let RawSnapshot::Expanded(expanded) = core else {
            return Err(infra(
                "E-QUERY-KIND",
                "Core query returned another snapshot kind",
            ));
        };
        let core = &expanded.module.core;
        report.phase = Phase::Expand;
        artifacts.dump_lazy(report, options, "core", |_| format!("{core:#?}"))?;
        artifacts.dump_lazy(report, options, "source-map", |_| {
            format!("{:#?}", expanded.module.source_map)
        })?;
        if selection_needed {
            artifacts.dump_lazy(report, options, "selection", |_| {
                expanded.selection.to_json()
            })?;
        }
        if options.stop_after == Stage::Core {
            return Ok(());
        }

        // Keep the original Core artifact for source provenance. Collect bounded
        // closure constraints before integer defaults or explicit # execution.
        // These hints remain raw Core data, with no verifier authority; CT still
        // enters its independently verified VM before ordinary type checking.
        let (ct_core, ct_snapshot) = self.query(
            key(Phase::Ct, true),
            std::slice::from_ref(&core_snapshot),
            options,
            report,
            observer,
            || {
                let inferred = zil_types::infer_source_closures(core, &options.limits)?;
                let remaining = expanded.selection.remaining(&options.limits)?;
                super::ct_program::single(&inferred, &options.target, &remaining)
                    .map(|core| RawSnapshot::Core(Arc::new(core)))
            },
        )?;
        let RawSnapshot::Core(ct_core) = ct_core else {
            return Err(infra(
                "E-QUERY-KIND",
                "CT query returned another snapshot kind",
            ));
        };
        report.phase = Phase::Ct;
        artifacts.dump_lazy(report, options, "ct-core", |_| format!("{ct_core:#?}"))?;
        check_entry_contract(&ct_core)?;
        let (typed, typed_snapshot) = self.query(
            key(Phase::Types, true),
            std::slice::from_ref(&ct_snapshot),
            options,
            report,
            observer,
            || {
                zil_types::check(&ct_core, &options.target, &options.limits)
                    .map(|typed| RawSnapshot::Typed(Arc::new(typed)))
            },
        )?;
        let RawSnapshot::Typed(typed) = typed else {
            return Err(infra(
                "E-QUERY-KIND",
                "types query returned another snapshot kind",
            ));
        };
        report.phase = Phase::Types;
        artifacts.dump_lazy(report, options, "typed", |_| format!("{typed:#?}"))?;
        if options.stop_after == Stage::Types {
            return Ok(());
        }
        self.finish_typed(
            options,
            report,
            observer,
            TypedInput::Module(typed),
            typed_snapshot,
            artifacts,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish_typed(
        &mut self,
        options: &CompileOptions,
        report: &mut CompileReport,
        observer: &mut dyn FnMut(&QueryEvent),
        typed: TypedInput,
        typed_snapshot: Arc<QuerySnapshot>,
        artifacts: ArtifactPlan,
    ) -> CompileResult<()> {
        // Types and Analyze depend on the same complete request identity. The
        // cached raw result is still independently checked below on every call.
        let mut analysis_key = typed_snapshot.key.clone();
        analysis_key.phase = Phase::Analyze;
        let (candidate, _) = self.query(
            analysis_key,
            std::slice::from_ref(&typed_snapshot),
            options,
            report,
            observer,
            || {
                zil_analyze::analyze(typed.module(), &options.limits)
                    .map(|candidate| RawSnapshot::Candidate(Arc::new(candidate)))
            },
        )?;
        let RawSnapshot::Candidate(candidate) = candidate else {
            return Err(infra(
                "E-QUERY-KIND",
                "analysis query returned another snapshot kind",
            ));
        };
        report.phase = Phase::Analyze;
        artifacts.dump_lazy(report, options, "candidate", |_| format!("{candidate:#?}"))?;
        if options.stop_after == Stage::Analyze {
            return Ok(());
        }

        options
            .cancellation
            .check(Phase::Verify)
            .map_err(|error| vec![error])?;
        report.verification_runs += 1;
        let verified = typed.verify(candidate, options);
        event(Phase::Verify, QueryDisposition::Executed, report, observer);
        options
            .cancellation
            .check(Phase::Verify)
            .map_err(|error| vec![error])?;
        let verified = verified?;
        let project = typed.program();
        report.phase = Phase::Verify;
        super::contracts::publish(&verified, &mut report.scalar_interfaces)?;
        if project.is_some() {
            artifacts.dump_lazy(report, options, "verified-interfaces", |report| {
                format!("{:#?}", report.scalar_interfaces)
            })?;
        }
        artifacts.dump_lazy(report, options, "verified", |_| {
            format!(
                "fingerprint={}\n{:#?}",
                verified.fingerprint(),
                verified.module()
            )
        })?;
        if options.stop_after == Stage::Verify {
            return Ok(());
        }
        let lir = zil_lower::lower_view(&verified)?;
        event(Phase::Lower, QueryDisposition::Executed, report, observer);
        options
            .cancellation
            .check(Phase::Lower)
            .map_err(|error| vec![error])?;
        report.phase = Phase::Lower;
        artifacts.dump_lazy(report, options, "lir", |_| format!("{lir:#?}"))?;
        if options.stop_after == Stage::Lower {
            return Ok(());
        }
        options
            .cancellation
            .check(Phase::Codegen)
            .map_err(|error| vec![error])?;
        let requested_entry = if let Some(name) = &options.entry {
            let id = if let Some(program) = project {
                program
                    .root_functions
                    .iter()
                    .find(|function| &function.name == name)
                    .map(|function| function.function)
            } else {
                verified
                    .module()
                    .functions
                    .iter()
                    .find(|function| &function.name == name)
                    .map(|function| function.id)
            }
            .ok_or_else(|| {
                vec![Diagnostic::error(
                    Phase::Driver,
                    "E-ENTRY-NOT-FOUND",
                    None,
                    format!("entry {name} does not exist in the root module"),
                )]
            })?;
            if let Some(function) = verified
                .module()
                .functions
                .iter()
                .find(|function| function.id == id && function.environment.is_some())
            {
                return Err(vec![Diagnostic::error(
                    Phase::Driver,
                    "E-ENTRY-ENVIRONMENT",
                    Some(function.span),
                    format!("entry {name} is a closure function that requires its environment"),
                )]);
            }
            Some(EmitOptions {
                entry: EntryPoint::ReturnValue(id),
            })
        } else {
            None
        };
        let publication = artifacts.native()?;
        let mut build = BuildOptions::new(publication.output());
        build.backend = options.backend;
        build.optimization = options.optimization;
        build.codegen = options.codegen;
        build.object_cache = options.object_cache.clone();
        build.windows = options.windows.clone();
        build.clang = options
            .clang
            .clone()
            .or_else(|| std::env::var_os("ZIL_CLANG").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("clang"));
        if let Some(entry) = requested_entry {
            build.emit = entry;
        }
        // Cancellation is cooperative between bounded compiler phases. clang
        // itself is a single external build step; this API does not claim to
        // interrupt a linker already running. Outputs stay in a private directory
        // until the last cancellation check and all fallible dumps have finished.
        let artifact =
            zil_backend_llvm::build_view_with_cancellation(&verified, &lir, &build, &|| {
                options.cancellation.check(Phase::Link).is_err()
            })?;
        event(Phase::Link, QueryDisposition::Executed, report, observer);
        if let Err(error) = options.cancellation.check(Phase::Link) {
            return Err(vec![error]);
        }
        let llvm = fs::read_to_string(&artifact.llvm_ir)
            .map_err(|error| infra("E-ARTIFACT-IO", error.to_string()))?;
        report.phase = Phase::Link;
        artifacts.dump(report, options, "llvm", &llvm)?;
        artifacts.validate()?;
        let mut artifact = publication.commit(artifact, &options.cancellation)?;
        // Only complete successful requests populate the machine cache. The
        // permission/capability for this request is never serialized or reused.
        artifact.commit_object_cache();
        report.artifacts.insert("llvm".into(), artifact.llvm_ir);
        report
            .artifacts
            .insert("executable".into(), artifact.executable);
        report.native_codegen = Some(artifact.codegen);
        report.commands.extend(artifact.commands);
        Ok(())
    }
}

pub(super) fn event(
    phase: Phase,
    disposition: QueryDisposition,
    report: &mut CompileReport,
    observer: &mut dyn FnMut(&QueryEvent),
) {
    let event = QueryEvent { phase, disposition };
    observer(&event);
    report.query_events.push(event);
}
pub(super) fn query_key(
    source: &SourceUnit,
    options: &CompileOptions,
    phase: Phase,
    target_dependent: bool,
) -> QueryKey {
    query_key_with_limits(source, options, phase, target_dependent, &options.limits)
}
fn query_key_with_limits(
    source: &SourceUnit,
    options: &CompileOptions,
    phase: Phase,
    target_dependent: bool,
    limits: &Limits,
) -> QueryKey {
    let mut input = b"ZIL-DRIVER-SOURCE-2\0".to_vec();
    input.extend_from_slice(&source.file_id.0.to_le_bytes());
    input.extend_from_slice(&source.module_id.0.to_le_bytes());
    input.extend_from_slice(&(source.path.len() as u64).to_le_bytes());
    input.extend_from_slice(source.path.as_bytes());
    input.extend_from_slice(&source.fingerprint().0);
    let mut rules = format!(
        "{RULE_VERSION}:{SCHEMA_VERSION}:{}:raw-session-v2",
        env!("CARGO_PKG_VERSION")
    )
    .into_bytes();
    for value in [
        limits.max_source_bytes as u64,
        limits.max_tokens as u64,
        limits.max_nodes as u64,
        limits.max_functions as u64,
        limits.max_blocks as u64,
        limits.max_instructions as u64,
        limits.max_locals as u64,
        limits.max_values as u64,
        limits.max_analysis_steps as u64,
        limits.max_ct_steps,
        limits.max_call_depth as u64,
    ] {
        rules.extend_from_slice(&value.to_le_bytes());
    }
    rules.extend_from_slice(&options.build_inputs.0);
    QueryKey {
        module: source.module_id,
        phase,
        input: Fingerprint::of(&input),
        rules: Fingerprint::of(&rules),
        target: if target_dependent {
            options.target.fingerprint()
        } else {
            Fingerprint::ZERO
        },
    }
}
fn parse_identity(source: &SourceUnit, options: &CompileOptions) -> QueryKey {
    let mut neutral = options.limits.clone();
    neutral.max_tokens = 0;
    neutral.max_nodes = 0;
    let mut key = query_key_with_limits(source, options, Phase::Parse, false, &neutral);
    let mut rules = b"ZIL-PARSE-RECEIPT-CP33-v1\0".to_vec();
    rules.extend_from_slice(&key.rules.0);
    key.rules = Fingerprint::of(&rules);
    key
}
pub(super) fn phase_key(
    identity: &QueryKey,
    phase: Phase,
    target_dependent: bool,
    target: Fingerprint,
) -> QueryKey {
    QueryKey {
        phase,
        target: if target_dependent {
            target
        } else {
            Fingerprint::ZERO
        },
        ..identity.clone()
    }
}
pub(super) fn disk_source(
    source: &SourceUnit,
    options: &CompileOptions,
    report: &mut CompileReport,
    parse_key: &QueryKey,
) -> CompileResult<()> {
    let Some(root) = &options.cache_dir else {
        return Ok(());
    };
    let cache = ArtifactCache::with_limit(root, options.limits.max_source_bytes);
    let key = parse_key.fingerprint();
    match cache
        .inspect_candidate(key)
        .map_err(|error| infra("E-CACHE-IO", error.to_string()))?
    {
        CacheRead::Candidate(bytes) if bytes == source.text.as_bytes() => {
            report.disk_cache = DiskCacheStatus::Hit;
            report.cache_hit = true;
            return Ok(());
        }
        CacheRead::Missing => report.disk_cache = DiskCacheStatus::Missing,
        _ => report.disk_cache = DiskCacheStatus::Rebuilt,
    }
    cache
        .write_candidate_cancellable(key, source.text.as_bytes(), &options.cancellation)
        .map_err(|error| match error {
            CacheWriteError::Io(error) => infra("E-CACHE-IO", error.to_string()),
            CacheWriteError::Cancelled => vec![Diagnostic::new(
                Phase::Driver,
                ResultCategory::Cancelled,
                "E-CANCELLED",
                "cache publication cancelled",
                None,
            )],
        })
}
pub(super) fn check_entry_contract(core: &CoreModule) -> CompileResult<()> {
    let mains = core
        .functions
        .iter()
        .filter(|function| function.name == "main")
        .collect::<Vec<_>>();
    if mains.len() > 1 {
        return Err(vec![Diagnostic::error(
            Phase::Types,
            "E-NAME-DUPLICATE",
            Some(mains[1].span),
            "main is declared more than once",
        )]);
    }
    if let Some(main) = mains.first() {
        // Unresolved nominal/error-union capabilities continue into the normal
        // type checker and remain Unsupported until their implementation exists.
        if !main.parameters.is_empty()
            || matches!(
                main.return_type,
                Type::Bool
                    | Type::I64
                    | Type::U8
                    | Type::U16
                    | Type::ForeignHandle
                    | Type::CWideString
                    | Type::ForeignCallback { .. }
                    | Type::Tuple(_)
                    | Type::FunctionPointer { .. }
                    | Type::Address { .. }
                    | Type::Managed(_)
                    | Type::Desc(_)
            )
        {
            return Err(vec![Diagnostic::error(
                Phase::Types,
                "E-ENTRY-SIGNATURE",
                Some(main.span),
                "native main must take no parameters and return unit or i32, or a supported error union",
            )]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod construction_identity150_tests {
    use super::*;
    use zil_base::{FileId, ModuleId};

    fn input() -> SourceUnit {
        SourceUnit::new(FileId(1), ModuleId(2), "identity.zil", "fn main()->i32=>0")
    }
    fn raw(source: &SourceUnit) -> RawSnapshot {
        RawSnapshot::Parse(Arc::new(zil_syntax::parse_with_limits(
            source,
            &Limits::default(),
        )))
    }
    fn snapshot(key: QueryKey, observation: &[u8]) -> Arc<QuerySnapshot> {
        Arc::new(QuerySnapshot {
            key,
            status: ResultCategory::Success,
            fingerprints: ResultFingerprints {
                private_implementation: Fingerprint::of(observation),
                ..ResultFingerprints::default()
            },
            dependencies: vec![],
            bytes: Arc::from([]),
            revision: 1,
        })
    }
    #[test]
    fn construction_identity_binds_order_count_upstream_key_and_observation() {
        let source = input();
        let raw = raw(&source);
        let key = query_key(&source, &CompileOptions::default(), Phase::Parse, false);
        let a = snapshot(key.clone(), b"a");
        let mut second = key.clone();
        second.module = ModuleId(3);
        let b = snapshot(second.clone(), b"b");
        let base = raw.digest(&key, &[a.clone(), b.clone()]);
        assert_eq!(base, raw.digest(&key, &[a.clone(), b.clone()]));
        assert_ne!(base, raw.digest(&key, &[b.clone(), a.clone()]));
        assert_ne!(base, raw.digest(&key, std::slice::from_ref(&a)));
        assert_ne!(
            base,
            raw.digest(&key, &[a.clone(), snapshot(second, b"changed")])
        );
        assert_ne!(
            base,
            raw.digest(&key, &[a.clone(), snapshot(key.clone(), b"b")])
        );
        assert_ne!(raw.digest(&key, &[]), raw.digest(&key, &[a]));
    }
    #[test]
    fn construction_identity_binds_every_query_axis_and_all_limits() {
        let source = input();
        let raw = raw(&source);
        let options = CompileOptions::default();
        let key = query_key(&source, &options, Phase::Parse, false);
        let base = raw.digest(&key, &[]);
        for k in [
            QueryKey {
                module: ModuleId(8),
                ..key.clone()
            },
            QueryKey {
                phase: Phase::Expand,
                ..key.clone()
            },
            QueryKey {
                input: Fingerprint::of(b"changed"),
                ..key.clone()
            },
            QueryKey {
                rules: Fingerprint::of(b"changed"),
                ..key.clone()
            },
            QueryKey {
                target: Fingerprint::of(b"changed"),
                ..key.clone()
            },
        ] {
            assert_ne!(base, raw.digest(&k, &[]));
        }
        macro_rules! limit_changes {
            ($($field:ident),* $(,)?) => { $(
                let mut o = options.clone();
                o.limits.$field += 1;
                assert_ne!(base, raw.digest(&query_key(&source, &o, Phase::Parse, false), &[]), stringify!($field));
            )* };
        }
        limit_changes!(
            max_source_bytes,
            max_tokens,
            max_nodes,
            max_functions,
            max_blocks,
            max_instructions,
            max_locals,
            max_values,
            max_analysis_steps,
            max_ct_steps,
            max_call_depth
        );
        for changed in [
            SourceUnit::new(FileId(9), source.module_id, &source.path, &source.text),
            SourceUnit::new(source.file_id, ModuleId(9), &source.path, &source.text),
            SourceUnit::new(
                source.file_id,
                source.module_id,
                "elsewhere.zil",
                &source.text,
            ),
            SourceUnit::new(
                source.file_id,
                source.module_id,
                &source.path,
                "fn main()->i32=>1",
            ),
        ] {
            assert_ne!(
                base,
                raw.digest(&query_key(&changed, &options, Phase::Parse, false), &[])
            );
        }
        let mut changed = options.clone();
        changed.build_inputs = Fingerprint::of(b"other build inputs");
        assert_ne!(
            base,
            raw.digest(&query_key(&source, &changed, Phase::Parse, false), &[])
        );
    }
}

#[cfg(test)]
mod bounded_history_tests {
    use super::*;
    use zil_base::{FileId, ModuleId};

    fn project(n: u32) -> ProjectSources {
        ProjectSources {
            root: ModuleId(0),
            paths: BTreeMap::new(),
            modules: (0..n)
                .map(|i| {
                    SourceUnit::new(
                        FileId(i),
                        ModuleId(i),
                        format!("history-{i}.zil"),
                        format!("fn f{i}()->i32 {{ return {i}; }}"),
                    )
                })
                .collect(),
        }
    }
    fn success(report: &CompileReport) {
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    fn equivalent(a: &CompileReport, b: &CompileReport) {
        assert_eq!(a.status, b.status);
        assert_eq!(a.phase, b.phase);
        assert_eq!(a.diagnostics, b.diagnostics);
        assert_eq!(a.scalar_interfaces, b.scalar_interfaces);
        assert_eq!(a.input_fingerprint, b.input_fingerprint);
        assert_eq!(a.verification_runs, b.verification_runs);
    }
    #[test]
    fn large_current_projects_reuse_semantics_across_the_old_capacity_cliff() {
        for n in [28, 29, 40, 128] {
            for capacity in [8, 32] {
                let p = project(n);
                let options = CompileOptions::default();
                let mut session = CompilationSession::with_capacity(capacity);
                success(&session.compile_project(&p, &options));
                for _ in 0..3 {
                    let warm = session.compile_project(&p, &options);
                    success(&warm);
                    for phase in [Phase::Expand, Phase::Ct, Phase::Types, Phase::Analyze] {
                        assert!(
                            warm.query_events.iter().any(|event| event.phase == phase
                                && event.disposition == QueryDisposition::Reused),
                            "n={n} capacity={capacity} phase={phase:?}"
                        );
                    }
                    assert!(session.raw.len() <= capacity);
                    assert_eq!(session.raw.len(), session.recency.len());
                    assert_eq!(session.database.len(), n as usize + 4);
                    assert_eq!(session.active_query_count(), 0);
                }
            }
        }
    }
    #[test]
    fn edits_errors_cancellation_and_eviction_match_fresh_without_growing_history() {
        let mut p = project(40);
        let options = CompileOptions::default();
        let mut session = CompilationSession::with_capacity(8);
        for edit in 0..48 {
            p.modules[0].text = format!("fn f0()->i32 {{ return {edit} + 0; }}");
            let current = session.compile_project(&p, &options);
            success(&current);
            equivalent(
                &current,
                &CompilationSession::default().compile_project(&p, &options),
            );
            assert!(session.raw.len() <= 8);
            // Roots include only this request's late Parse and semantic stages;
            // previous source epochs are not immortal dependency receipts.
            assert_eq!(session.database.len(), 44);
            let mut invalid = p.clone();
            invalid.modules[1].text = "fn f1()->i32 { return true; }".into();
            let rejected = session.compile_project(&invalid, &options);
            assert_ne!(rejected.status, ResultCategory::Success);
            equivalent(
                &rejected,
                &CompilationSession::default().compile_project(&invalid, &options),
            );
            let cancelled = CompileOptions::default();
            cancelled.cancellation.cancel();
            let stopped = session.compile_project(&p, &cancelled);
            assert_eq!(stopped.status, ResultCategory::Cancelled);
            assert_eq!(session.active_query_count(), 0);
            assert!(session.raw.len() <= 8);
        }
        session.clear();
        assert!(session.database.is_empty());
        assert!(session.raw.is_empty());
        assert!(session.recency.is_empty());
        success(&session.compile_project(&p, &CompileOptions::default()));
    }
}
