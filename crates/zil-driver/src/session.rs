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
/// VerifiedModule/VerifiedCTIR variant. The explicit trusted semantic cache can
/// restore a raw ProjectTyped value, followed by current Analyze and Verify.
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
pub(super) struct ProjectFrontend {
    pub typed: Arc<TypedProgram>,
    pub snapshot: Arc<QuerySnapshot>,
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
    /// Most values are private constructions. ProjectTyped can also come from
    /// the explicit trusted semantic cache, bound to the complete key and
    /// installed compiler identity; deserialization never grants authority.
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
    analysis: zil_analyze::AnalysisSession,
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
            analysis: zil_analyze::AnalysisSession::default(),
        }
    }
    pub fn clear(&mut self) {
        self.database.clear();
        self.raw.clear();
        self.recency.clear();
        self.clock = 0;
        self.analysis.clear();
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

    /// A project larger than the raw capacity cannot retain both complete
    /// epochs. Release obsolete whole-project owners before constructing the
    /// next epoch, while retaining unchanged Parse inputs and exact-current
    /// semantic results. This avoids doubling the live IR on every private edit.
    pub(super) fn prepare_project(
        &mut self,
        identity: &QueryKey,
        target: Fingerprint,
        modules: usize,
    ) {
        if modules.saturating_add(4) <= self.capacity {
            return;
        }
        let before = self.raw.len();
        self.raw.retain(|key, raw| {
            let project_stage = matches!(
                raw,
                RawSnapshot::Project(_)
                    | RawSnapshot::ProjectCore(_)
                    | RawSnapshot::ProjectTyped(_)
                    | RawSnapshot::Candidate(_)
            );
            !project_stage
                || key.module != identity.module
                || (key.input == identity.input
                    && key.rules == identity.rules
                    && (key.target == Fingerprint::ZERO || key.target == target))
        });
        if before != self.raw.len() {
            self.recency.retain(|key, _| self.raw.contains_key(key));
            self.database.retain_reachable(self.raw.keys());
        }
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
        self.query_with_disposition(
            key,
            dependencies,
            options,
            report,
            observer,
            execute,
            QueryDisposition::Executed,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn query_with_disposition(
        &mut self,
        key: QueryKey,
        dependencies: &[Arc<QuerySnapshot>],
        options: &CompileOptions,
        report: &mut CompileReport,
        observer: &mut dyn FnMut(&QueryEvent),
        execute: impl FnOnce() -> CompileResult<RawSnapshot>,
        disposition: QueryDisposition,
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
        event(key.phase, disposition, report, observer);
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

    /// Reuse an exact, successfully constructed project frontend without
    /// materializing evicted Parse values. This is raw computation reuse, never
    /// verification authority. The caller has checked all current source and
    /// artifact guards and excluded requests requiring intermediate artifacts.
    pub(super) fn reuse_project_frontend(
        &mut self,
        key: &QueryKey,
        sources: &[SourceUnit],
        options: &CompileOptions,
        report: &mut CompileReport,
        observer: &mut dyn FnMut(&QueryEvent),
    ) -> CompileResult<Option<ProjectFrontend>> {
        let Some(RawSnapshot::ProjectTyped(typed)) = self.raw.get(key).cloned() else {
            return Ok(None);
        };
        // Any incomplete/incompatible metadata falls through to the original
        // ordered pipeline, which retains its original first-error behavior.
        let trace = (|| {
            let types = self.database.read(key).ok()??;
            let [ct_dependency] = types.dependencies.as_slice() else {
                return None;
            };
            if ct_dependency.key != phase_key(key, Phase::Ct, true, key.target) {
                return None;
            }
            let ct = self.database.read(&ct_dependency.key).ok()??;
            let [expand_dependency] = ct.dependencies.as_slice() else {
                return None;
            };
            let expand_key = &expand_dependency.key;
            if expand_key != &phase_key(key, Phase::Expand, false, key.target)
                && expand_key != &phase_key(key, Phase::Expand, true, key.target)
            {
                return None;
            }
            let expand = self.database.read(expand_key).ok()??;
            if [types.status, ct.status, expand.status]
                .iter()
                .any(|status| *status != ResultCategory::Success)
                || expand.dependencies.len() != sources.len()
                || expand
                    .dependencies
                    .iter()
                    .zip(sources)
                    .any(|(dep, source)| {
                        dep.key.phase != Phase::Parse || dep.key.module != source.module_id
                    })
            {
                return None;
            }
            let mut replay = expand
                .dependencies
                .iter()
                .map(|dep| dep.key.clone())
                .collect::<Vec<_>>();
            replay.extend([expand.key.clone(), ct.key.clone(), types.key.clone()]);
            Some((types, replay))
        })();
        let Some((snapshot, replay)) = trace else {
            return Ok(None);
        };
        for completed in replay {
            options
                .cancellation
                .check(completed.phase)
                .map_err(|e| vec![e])?;
            event(completed.phase, QueryDisposition::Reused, report, observer);
            options
                .cancellation
                .check(completed.phase)
                .map_err(|e| vec![e])?;
            report.cache_hit = true;
            // A dependency receipt does not own a raw value. Do not invent
            // recency entries for evicted values or change the capacity bound.
            if self.raw.contains_key(&completed) {
                self.touch(&completed);
            }
        }
        Ok(Some(ProjectFrontend { typed, snapshot }))
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
        // A completed subquery is still raw data. Commit its replacement epoch
        // only if the enclosing driver query survives publication/cancellation.
        let mut analysis = std::mem::take(&mut self.analysis);
        let checkpoint = analysis.clone();
        let queried = self.query(
            analysis_key,
            std::slice::from_ref(&typed_snapshot),
            options,
            report,
            observer,
            || {
                analysis
                    .analyze(typed.module(), &options.limits)
                    .map(|candidate| RawSnapshot::Candidate(Arc::new(candidate)))
            },
        );
        self.analysis = if queried.is_ok() {
            analysis
        } else {
            checkpoint
        };
        let (candidate, _) = queried?;
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
        build.closed_executable = options.closed_executable;
        build.incremental_o2 = options.incremental_o2;
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
    fn large_edits_release_obsolete_owners_before_constructing_the_new_epoch() {
        let mut project = project(128);
        let options = CompileOptions::default();
        let mut session = CompilationSession::default();
        success(&session.compile_project(&project, &options));
        let old = session
            .raw
            .values()
            .find_map(|raw| match raw {
                RawSnapshot::ProjectTyped(program) => Some(Arc::downgrade(program)),
                _ => None,
            })
            .unwrap();
        success(&session.compile_project(&project, &options));
        assert!(
            old.upgrade().is_some(),
            "unchanged input retains the current epoch"
        );
        project.modules[0].text = "fn f0()->i32 { return 123 + 0; }".into();
        let mut observed = false;
        let edited = session.compile_project_with_observer(&project, &options, &mut |event| {
            if event.phase == Phase::Parse {
                assert!(
                    old.upgrade().is_none(),
                    "obsolete semantic owners were retained"
                );
                observed = true;
            }
        });
        assert!(observed);
        success(&edited);
        equivalent(
            &edited,
            &CompilationSession::default().compile_project(&project, &options),
        );
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

    #[test]
    fn frontend_reuse_requires_reachable_receipts_and_never_supplies_authority() {
        let p = project(40);
        let options = CompileOptions::default();
        let mut session = CompilationSession::with_capacity(8);
        success(&session.compile_project(&p, &options));
        session.database.clear();
        let repaired = session.compile_project(&p, &options);
        success(&repaired);
        assert!(
            repaired.query_events.iter().any(|e| {
                e.phase == Phase::Parse && e.disposition == QueryDisposition::Executed
            })
        );
        equivalent(
            &repaired,
            &CompilationSession::default().compile_project(&p, &options),
        );

        // Fault-inject only private raw input. An exact-source checkpoint must
        // still obtain current independent Verify, which rejects this change.
        for raw in session.raw.values_mut() {
            if let RawSnapshot::ProjectTyped(typed) = raw {
                Arc::make_mut(typed).module.schema_version = 0;
            }
        }
        let rejected = session.compile_project(&p, &options);
        assert_eq!(rejected.verification_runs, 1);
        assert_eq!(rejected.phase, Phase::Verify);
        assert_eq!(rejected.status, ResultCategory::SafeReject);
        assert_eq!(rejected.diagnostics[0].code, "E-VERIFY-SCHEMA");
        assert!(
            rejected
                .query_events
                .iter()
                .filter(|e| e.phase == Phase::Parse)
                .all(|e| e.disposition == QueryDisposition::Reused)
        );
        assert_eq!(session.raw.len(), session.recency.len());
        assert!(session.raw.len() <= 8);
    }

    #[test]
    fn reactive_initialization_queries_survive_real_private_and_public_source_edits() {
        let mut p = project(40);
        let options = CompileOptions::default();
        let mut session = CompilationSession::with_capacity(8);
        success(&session.compile_project(&p, &options));
        p.modules[0].text = "fn f0()->i32 { return 123; }".into();
        let edited = session.compile_project(&p, &options);
        success(&edited);
        equivalent(
            &edited,
            &CompilationSession::default().compile_project(&p, &options),
        );
        assert_eq!(session.analysis.last_stats().reused_functions, 40);
        assert_eq!(session.analysis.last_stats().executed_functions, 0);

        // An inserted declaration shifts later whole-program IDs. Initialization
        // identity follows the actual local semantic input instead of that ID.
        p.modules[0].text = "fn inserted()->i32 {return 9;} fn f0()->i32 {return 123;}".into();
        let inserted = session.compile_project(&p, &options);
        success(&inserted);
        equivalent(
            &inserted,
            &CompilationSession::default().compile_project(&p, &options),
        );
        assert_eq!(session.analysis.last_stats().reused_functions, 41);
        assert_eq!(session.analysis.last_stats().executed_functions, 0);

        p.modules[0].text = "pub fn f0(x:i32)->i32 {return x;}".into();
        let public = session.compile_project(&p, &options);
        success(&public);
        equivalent(
            &public,
            &CompilationSession::default().compile_project(&p, &options),
        );
        assert!(session.analysis.last_stats().reused_functions >= 39);
        assert!(session.analysis.last_stats().executed_functions > 0);
        p.modules[0].text = "fn f0()->i32 {return true;}".into();
        let rejected = session.compile_project(&p, &options);
        assert_ne!(rejected.status, ResultCategory::Success);
        equivalent(
            &rejected,
            &CompilationSession::default().compile_project(&p, &options),
        );
        session.clear();
        assert_eq!(session.analysis.last_stats().retained_queries, 0);
    }

    #[test]
    fn cancellation_after_analysis_rolls_back_the_subquery_epoch() {
        let mut p = project(4);
        let options = CompileOptions::default();
        let mut session = CompilationSession::default();
        success(&session.compile_project(&p, &options));
        let prior = session.analysis.last_stats();
        p.modules[0].text = "pub fn f0(x:i32)->i32 {return x;}".into();
        let cancelled = CompileOptions::default();
        let stopped = session.compile_project_with_observer(&p, &cancelled, &mut |event| {
            if event.phase == Phase::Analyze {
                cancelled.cancellation.cancel();
            }
        });
        assert_eq!(stopped.status, ResultCategory::Cancelled);
        assert_eq!(session.analysis.last_stats(), prior);
        assert_eq!(session.active_query_count(), 0);
        let resumed = session.compile_project(&p, &options);
        success(&resumed);
        equivalent(
            &resumed,
            &CompilationSession::default().compile_project(&p, &options),
        );
        assert!(session.analysis.last_stats().executed_functions > 0);
    }

    #[test]
    fn real_source_subquery_hits_replay_the_exact_current_budget_boundary() {
        let p = project(4);
        let options = CompileOptions::default();
        let mut session = CompilationSession::default();
        success(&session.compile_project(&p, &options));
        let mut low = 0;
        let mut high = options.limits.max_analysis_steps;
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            let mut limited = CompileOptions::default();
            limited.limits.max_analysis_steps = middle;
            let result = CompilationSession::default().compile_project(&p, &limited);
            if result.status == ResultCategory::Success {
                high = middle;
            } else {
                assert_eq!(result.status, ResultCategory::ResourceLimit);
                low = middle;
            }
        }
        for limit in [high, high - 1, high] {
            let mut limited = CompileOptions::default();
            limited.limits.max_analysis_steps = limit;
            let current = session.compile_project(&p, &limited);
            equivalent(
                &current,
                &CompilationSession::default().compile_project(&p, &limited),
            );
            assert_eq!(current.status == ResultCategory::Success, limit == high);
            assert_eq!(session.active_query_count(), 0);
        }
    }
}
