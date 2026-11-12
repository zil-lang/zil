//! Explicit source graphs. Module identity never depends on a concatenated name
//! or source buffer. The filesystem adapter is the only import-reading entry.
use super::artifacts::ArtifactPlan;
use super::session::{
    RawSnapshot, TypedInput, check_entry_contract, disk_source, phase_key, query_key,
};
use super::*;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use zil_base::{CompileResult, ModuleId, SourceMap, Span};
use zil_db::{QueryKey, QuerySnapshot};
use zil_ir::{CandidatePlan, CoreImport, CoreProgram, ProgramModule, TypedProgram};
use zil_syntax::{ParseResult, ast};

#[path = "project_discovery_cache.rs"]
mod discovery_cache;
#[path = "stdlib.rs"]
mod stdlib;

/// Actual work performed by the filesystem ingress adapter, before queries.
/// Query reuse labels are deliberately not used to infer parser or disk work.
#[derive(Clone, Debug, Default)]
pub struct SourceIngressStats {
    /// Successfully accepted SourceUnit data, not physical kernel I/O volume.
    pub source_files_read: usize,
    pub source_bytes_read: usize,
    pub source_clone_bytes: usize,
    pub parser_executions: usize,
    pub parser_reuses: usize,
    pub semantic_load_attempts: usize,
    pub semantic_restored_functions: usize,
    pub resident_routes: usize,
    pub resident_frontend_hits: usize,
    pub discovery_receipt_probes: usize,
    pub discovery_receipt_complete: usize,
    pub source_read_ns: u128,
    pub discovery_receipt_ns: u128,
    pub semantic_probe_ns: u128,
    pub parser_ns: u128,
}
impl SourceIngressStats {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"source_files_read\":{},\"source_bytes_read\":{},\"source_clone_bytes\":{},\"parser_executions\":{},\"parser_reuses\":{},\"semantic_load_attempts\":{},\"semantic_restored_functions\":{},\"resident_routes\":{},\"resident_frontend_hits\":{},\"discovery_receipt_probes\":{},\"discovery_receipt_complete\":{},\"source_read_ns\":{},\"discovery_receipt_ns\":{},\"semantic_probe_ns\":{},\"parser_ns\":{}}}",
            self.source_files_read,
            self.source_bytes_read,
            self.source_clone_bytes,
            self.parser_executions,
            self.parser_reuses,
            self.semantic_load_attempts,
            self.semantic_restored_functions,
            self.resident_routes,
            self.resident_frontend_hits,
            self.discovery_receipt_probes,
            self.discovery_receipt_complete,
            self.source_read_ns,
            self.discovery_receipt_ns,
            self.semantic_probe_ns,
            self.parser_ns
        )
    }
    fn read(&mut self, source: &SourceUnit) {
        self.source_files_read += 1;
        self.source_bytes_read += source.text.len();
    }
}

/// All bytes are supplied by the caller; compiling this value never loads an
/// import from disk. Paths are root-relative logical import segments. Multiple
/// paths may refer to one module (for example, canonicalized filesystem aliases).
#[derive(Clone, Debug)]
pub struct ProjectSources {
    pub root: ModuleId,
    pub modules: Vec<SourceUnit>,
    pub paths: BTreeMap<Vec<String>, ModuleId>,
}

#[derive(Clone, Debug)]
pub(super) struct ExpandedProject {
    core: Arc<CoreProgram>,
    maps: Vec<Arc<SourceMap>>,
    selection: super::ct_selection::Receipt,
    ct_usage: Option<zil_ir::ct_artifact::CtUsageReceipt>,
    ct_previews: Vec<super::ct_artifact::Preview>,
    pub(super) syntax: Arc<ProjectSyntax>,
    producer: Option<zil_expand::ProducedProgram>,
    producer_epoch: Option<Arc<ProducerEpoch>>,
}

/// Raw acceleration is owned by the existing Expand snapshot/LRU entry. It
/// carries sealed parser origins and budget receipts, never Verify authority.
/// Keeping this metadata in the raw owner adds no independent epoch history.
pub(super) struct ProducerEpoch {
    core: Arc<CoreProgram>,
    target: Fingerprint,
    limits: Limits,
    inputs: Vec<zil_syntax::ParsedSource>,
    producer: Box<zil_expand::ExpandSession>,
    materializer: super::ct_artifact::ScalarMaterializationSession,
    raw_inference: std::sync::OnceLock<zil_types::RawInferenceSession>,
}
impl std::fmt::Debug for ProducerEpoch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProducerEpoch(..)")
    }
}
#[inline(never)]
fn capture_producer_epoch(
    core: &zil_expand::ProducedProgram,
    producer: &zil_expand::ExpandSession,
    materializer: &super::ct_artifact::ScalarMaterializationSession,
    inputs: &[zil_syntax::ParsedSource],
    options: &CompileOptions,
) -> Arc<ProducerEpoch> {
    Arc::new(ProducerEpoch {
        core: core.program().clone(),
        target: options.target.fingerprint(),
        limits: options.limits.clone(),
        inputs: inputs.to_vec(),
        producer: super::session::producer_checkpoint(producer),
        materializer: materializer.checkpoint(),
        raw_inference: std::sync::OnceLock::new(),
    })
}

/// Syntax belongs to an existing whole-project raw owner, not an independent
/// unbounded history. A new request may borrow this index without keeping the
/// old Core/Typed owners alive. Unchanged modules share their original Arcs.
#[derive(Debug)]
pub(super) struct ProjectSyntax {
    pub(super) root: (FileId, ModuleId, String),
    pub(super) entries: BTreeMap<QueryKey, Arc<ParseResult>>,
}

/// Discovery has already done the real parsing work, outside project_pipeline.
/// Preserve its origin so lending newly parsed syntax is not reported as reuse.
#[derive(Debug)]
struct LoadedSyntax {
    parsed: Arc<ParseResult>,
    producer: Option<zil_syntax::ParsedSource>,
    reused: bool,
    snapshot: Option<Arc<QuerySnapshot>>,
    // Present only when filesystem discovery supplied a lossless canonical
    // identity for the explicit closed-executable trial.
    stable_module_key: Option<Arc<str>>,
}

/// An exact import-free source may restore its already decoded raw frontend.
/// Ordinary discovery still owns syntax; neither variant carries Verify authority.
enum LoadedProject {
    Parsed(Vec<LoadedSyntax>),
    Prepared(Vec<LoadedSyntax>, Box<CompileReport>),
    Semantic(Box<(TypedProgram, CandidatePlan)>),
    /// Current source/import replay completed; only resident raw candidates may
    /// be tried before the ordinary disk/source fallbacks. No Verify authority.
    Resident,
}

pub fn compile_project(project: &ProjectSources, options: &CompileOptions) -> CompileReport {
    CompilationSession::default().compile_project(project, options)
}

/// Discover test declarations from the original parsed root source. Every
/// selected test is still compiled and verified through the ordinary pipeline.
pub fn discover_tests(path: &Path, options: &CompileOptions) -> CompileResult<Vec<String>> {
    let mut project = ProjectSources {
        root: ModuleId(0),
        modules: vec![SourceUnit::new(
            FileId(0),
            ModuleId(0),
            path.to_string_lossy(),
            "",
        )],
        paths: BTreeMap::new(),
    };
    let mut discovery_options = options.clone();
    load_sources(
        &mut project,
        path,
        &mut discovery_options,
        &CompilationSession::default(),
    )?;
    let parsed = zil_syntax::parse_with_limits(&project.modules[0], &options.limits);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }
    let mut names = Vec::new();
    let mut seen = BTreeSet::new();
    for item in &parsed.module.items {
        if !item
            .attributes
            .iter()
            .any(|a| a.path.as_slice() == ["test"])
        {
            continue;
        }
        let ast::ItemKind::Function(function) = &item.kind else {
            return Err(reject(
                "E-TEST-SIGNATURE",
                Some(item.span),
                "test must be an ordinary function",
            ));
        };
        if !function.params.is_empty()
            || !function.generics.is_empty()
            || matches!(function.body, ast::FunctionBody::Declaration)
        {
            return Err(reject(
                "E-TEST-SIGNATURE",
                Some(item.span),
                "test must be a defined zero-argument non-generic function",
            ));
        }
        if !seen.insert(function.name.clone()) {
            return Err(reject(
                "E-TEST-DUPLICATE",
                Some(item.span),
                format!("duplicate test {}", function.name),
            ));
        }
        names.push(function.name.clone());
    }
    Ok(names)
}

pub(super) fn artifact_candidate(source: &SourceUnit, parsed: &ParseResult) -> bool {
    let standard = parsed.module.items.iter().any(|item| {
        matches!(&item.kind,
        ast::ItemKind::Import(import) if matches!(import.path.as_slice(),
            [a, b] if a == "std" && (b == "meta" || b == "syntax")))
    });
    let runtime_generic_ct = parsed.module.items.iter().any(|item| {
        matches!(&item.kind, ast::ItemKind::Function(function)
            if function.generics.iter().any(|parameter| !matches!(parameter.kind, ast::GenericParamKind::Lifetime))
            && parsed.tokens.iter().any(|token| token.span.start >= function.span.start
                && token.span.end <= function.span.end && token.text(&source.text) == "#"))
    });
    // Candidate routing only. A real static-host call, its complete source
    // actuals, effects and successful execution are checked by the CT consumer.
    // An unrelated #(7) cannot qualify any of these ordinary methods.
    let static_namespace_ct = parsed.module.items.iter().any(|item| {
        matches!(&item.kind, ast::ItemKind::Aggregate(host)
            if !host.generics.is_empty() && !host.methods.is_empty())
    }) && parsed
        .tokens
        .iter()
        .any(|token| token.text(&source.text) == "#");
    standard
        || runtime_generic_ct
        || static_namespace_ct
        || parsed.type_meta_candidate
        || parsed.tokens.iter().any(|token| {
            token.kind == zil_syntax::token::TokenKind::Identifier
                && matches!(
                    token.text(&source.text),
                    "Type" | "Syntax" | "Ident" | "quote" | "splice" | "emit"
                )
                || token.text(&source.text) == "const"
        })
}

impl CompilationSession {
    /// A closed DB read has already validated this exact Expand owner. Check
    /// its private output association and all current source bytes before using
    /// its acceleration metadata; hashes alone cannot retarget parser seals.
    pub(super) fn retained_producer_epoch(
        &self,
        key: &QueryKey,
        sources: &[SourceUnit],
        options: &CompileOptions,
    ) -> Option<Arc<ProducerEpoch>> {
        if !options.reuse_producers() {
            return None;
        }
        let RawSnapshot::Project(expanded) = self.raw.get(key)? else {
            return None;
        };
        let epoch = expanded.producer_epoch.as_ref()?;
        if !Arc::ptr_eq(&expanded.core, &epoch.core)
            || !Arc::ptr_eq(expanded.producer.as_ref()?.program(), &epoch.core)
            || epoch.target != options.target.fingerprint()
            || epoch.limits != options.limits
            || epoch.inputs.len() != sources.len()
            || !epoch
                .inputs
                .iter()
                .zip(sources)
                .all(|(parsed, source)| parsed.source() == source)
        {
            return None;
        }
        Some(epoch.clone())
    }
    pub(super) fn activate_producer_epoch(
        &mut self,
        epoch: &ProducerEpoch,
        options: &CompileOptions,
    ) {
        self.producer = super::session::producer_checkpoint(&epoch.producer);
        self.producer
            .set_cost_profiling(options.profile_compilation);
        self.materializer = epoch.materializer.checkpoint();
        if options.push_reuse {
            self.raw_inference = epoch
                .raw_inference
                .get()
                .and_then(|session| session.checkpoint_for(&epoch.core))
                .unwrap_or_default();
        }
        let mut cache = self.producer_syntax.borrow_mut();
        for parsed in &epoch.inputs {
            let source = parsed.source();
            cache.insert(
                (source.module_id, source.file_id, source.path.clone()),
                parsed.clone(),
            );
        }
        while cache.len() > self.capacity {
            cache.pop_first();
        }
    }

    /// This cache can only contain real parser constructions. A current read
    /// still compares all source bytes and the complete parsing envelope.
    /// Public ParseResult/Core values cannot be paired with these revisions.
    fn producer_parse(
        &self,
        source: &SourceUnit,
        limits: &Limits,
    ) -> (zil_syntax::ParsedSource, bool) {
        let context = zil_syntax::attributes::AttributeContext::default();
        let owner = (source.module_id, source.file_id, source.path.clone());
        let mut cache = self.producer_syntax.borrow_mut();
        if let Some(parsed) = cache.get(&owner)
            && parsed.matches_current(source, limits, &context)
        {
            return (parsed.clone(), true);
        }
        let parsed = zil_syntax::ParsedSource::parse(Arc::new(source.clone()), limits, &context);
        if parsed.parsed().diagnostics.is_empty() {
            cache.insert(owner, parsed.clone());
            // A failed discovery cannot accumulate unbounded source owners.
            while cache.len() > self.capacity {
                cache.pop_first();
            }
        }
        (parsed, false)
    }

    pub(super) fn compile_source_artifact_project(
        &mut self,
        source: &SourceUnit,
        parsed: Arc<ParseResult>,
        snapshot: Arc<QuerySnapshot>,
        options: &CompileOptions,
        observer: &mut dyn FnMut(&QueryEvent),
        report: CompileReport,
    ) -> CompileReport {
        let project = ProjectSources {
            root: source.module_id,
            modules: vec![source.clone()],
            paths: BTreeMap::new(),
        };
        self.compile_project_request(
            &project,
            options,
            observer,
            Some(LoadedProject::Prepared(
                vec![LoadedSyntax {
                    parsed,
                    producer: None,
                    reused: true,
                    snapshot: Some(snapshot),
                    stable_module_key: None,
                }],
                Box::new(report),
            )),
        )
    }
    pub fn compile_project(
        &mut self,
        project: &ProjectSources,
        options: &CompileOptions,
    ) -> CompileReport {
        self.compile_project_with_observer(project, options, &mut |_| {})
    }

    pub fn compile_project_with_observer(
        &mut self,
        project: &ProjectSources,
        options: &CompileOptions,
        observer: &mut dyn FnMut(&QueryEvent),
    ) -> CompileReport {
        self.compile_project_request(project, options, observer, None)
    }

    /// The filesystem adapter may lend the exact syntax it already parsed to
    /// discover imports. This private channel never carries verified authority
    /// and publishes nothing before the project/artifact guards have passed.
    fn compile_project_request(
        &mut self,
        project: &ProjectSources,
        options: &CompileOptions,
        observer: &mut dyn FnMut(&QueryEvent),
        loaded: Option<LoadedProject>,
    ) -> CompileReport {
        let start = Instant::now();
        let mut effective_options = options.clone();
        if let Some(lock) = &options.syntax_lock {
            let mut input = b"zil/syntax-lock-build-input/v1\0".to_vec();
            input.extend_from_slice(&options.build_inputs.0);
            input.extend_from_slice(&lock.fingerprint.0);
            effective_options.build_inputs = Fingerprint::of(&input);
        }
        let options = &effective_options;
        let fallback = SourceUnit::new(FileId(0), project.root, "<project>", "");
        let root = project
            .modules
            .iter()
            .find(|m| m.module_id == project.root)
            .unwrap_or(&fallback);
        let (loaded_syntax, preloaded_semantic, prepared_report) = match loaded {
            Some(LoadedProject::Parsed(syntax)) => (Some(syntax), None, None),
            Some(LoadedProject::Prepared(syntax, report)) => (Some(syntax), None, Some(report)),
            Some(LoadedProject::Semantic(pair)) => (None, Some(pair), None),
            Some(LoadedProject::Resident) => (None, None, None),
            None => (None, None, None),
        };
        let mut report = prepared_report.unwrap_or_else(|| Box::new(CompileReport::new(root)));
        report.cost_profiled = options.profile_compilation;
        let checkpoint_start = cost_start(options.profile_compilation);
        let function_checkpoint = super::session::function_checkpoint(&self.function_types);
        report.record_driver_cost("function_checkpoint", checkpoint_start);
        if let Err(errors) = self.project_pipeline(
            project,
            options,
            &mut report,
            observer,
            loaded_syntax.as_deref(),
            preloaded_semantic,
        ) {
            // Raw function work products receive the same rejected-candidate
            // fallback as whole-project semantic candidates. Reuse the immutable
            // current source graph; never reread files or retry cancellation/IO.
            let retry = !options.cancellation.is_cancelled()
                && report
                    .function_type_work
                    .as_deref()
                    .is_some_and(|work| work.reused_functions > 0)
                && matches!(report.phase, Phase::Types | Phase::Analyze)
                && errors.iter().all(|e| {
                    matches!(
                        e.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )
                });
            if retry {
                self.clear();
                report.restart_attempt(
                    CompileReport::new(root),
                    "rejected_function_work_product",
                    errors,
                );
                report.disk_cache = DiskCacheStatus::Rebuilt;
                let mut fresh_options = options.clone();
                fresh_options.semantic_cache = None;
                if let Err(errors) = self.project_pipeline(
                    project,
                    &fresh_options,
                    &mut report,
                    observer,
                    loaded_syntax.as_deref(),
                    None,
                ) {
                    self.function_types = function_checkpoint;
                    report.fail(errors);
                }
            } else {
                self.function_types = function_checkpoint;
                report.fail(errors);
            }
        }
        self.trim_history();
        report.finish_attempts();
        report.seconds += start.elapsed().as_secs_f64();
        *report
    }

    pub(super) fn compile_project_path(
        &mut self,
        path: &Path,
        options: &CompileOptions,
    ) -> CompileReport {
        let start = Instant::now();
        let mut effective_options = options.clone();
        if let Some(manifest) = &options.project {
            let mut input = b"zil/manifest-build-input/v1\0".to_vec();
            input.extend_from_slice(&options.build_inputs.0);
            input.extend_from_slice(&manifest.fingerprint.0);
            effective_options.build_inputs = Fingerprint::of(&input);
        }
        let options = &mut effective_options;
        // Discovery may borrow immutable syntax from the previous request.
        // Request-end eviction bounds retained raw owners, including the syntax
        // indexes owned by complete projects. Borrowing grants no authority.
        let mut project = ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(
                FileId(0),
                ModuleId(0),
                path.to_string_lossy(),
                "",
            )],
            paths: BTreeMap::new(),
        };
        // Parse is a syntax-only stop for the requested file. Import discovery
        // belongs to expansion and must not read or reject dependencies yet.
        let discovery_start = cost_start(options.profile_compilation);
        let mut ingress = SourceIngressStats::default();
        let loaded = if options.stop_after == Stage::Parse {
            let accepted = if let Some(manifest) = &options.project {
                manifest
                    .check_lock()
                    .and_then(|_| manifest.check_entry(path))
            } else {
                Ok(path.to_path_buf())
            };
            accepted
                .and_then(|accepted| read_source(&accepted, options))
                .map(|source| {
                    if options.profile_compilation {
                        ingress.read(&source);
                    }
                    project.modules[0] = source;
                    LoadedProject::Parsed(Vec::new())
                })
        } else {
            load_sources_profiled(
                &mut project,
                path,
                options,
                self,
                options.profile_compilation.then_some(&mut ingress),
            )
        };
        let discovery_pending = match &loaded {
            Ok(LoadedProject::Parsed(syntax)) if options.stop_after != Stage::Parse => options
                .project
                .is_none()
                .then(|| discovery_cache::Pending::new(&project, syntax, options))
                .flatten(),
            _ => None,
        };
        let discovery_ns = discovery_start.map(|start| start.elapsed().as_nanos());
        let mut report = match loaded {
            Ok(_) if options.stop_after == Stage::Parse => {
                self.compile_source(&project.modules[0], options)
            }
            Ok(parsed) => {
                self.compile_project_request(&project, options, &mut |_| {}, Some(parsed))
            }
            Err(errors) => {
                let mut report = CompileReport::new(&project.modules[0]);
                report.source_files = project
                    .modules
                    .iter()
                    .map(|m| (m.file_id, m.path.clone()))
                    .collect();
                report.fail(errors);
                report
            }
        };
        report.cost_profiled = options.profile_compilation;
        if options.profile_compilation {
            ingress.resident_frontend_hits =
                usize::from(report.disk_cache == DiskCacheStatus::Resident);
            report.source_ingress = Some(Box::new(ingress));
        }
        if let Some(nanos) = discovery_ns {
            *report
                .driver_timings_ns
                .entry("source_ingress")
                .or_default() += nanos;
        }
        let publication_start = cost_start(options.profile_compilation);
        if report.status == ResultCategory::Success
            && report.verification_runs != 0
            && let Some(pending) = discovery_pending
        {
            pending.publish(&project, options);
        }
        report.record_driver_cost("discovery_receipt_publication", publication_start);
        report.seconds = start.elapsed().as_secs_f64();
        report
    }

    fn project_pipeline(
        &mut self,
        project: &ProjectSources,
        options: &CompileOptions,
        report: &mut CompileReport,
        observer: &mut dyn FnMut(&QueryEvent),
        loaded_syntax: Option<&[LoadedSyntax]>,
        preloaded_semantic: Option<Box<(TypedProgram, CandidatePlan)>>,
    ) -> CompileResult<()> {
        let setup_start = cost_start(options.profile_compilation);
        options
            .cancellation
            .check(Phase::Driver)
            .map_err(|e| vec![e])?;
        if options.profile == ExecutionProfile::Managed {
            return Err(vec![Diagnostic::unsupported(
                Phase::Driver,
                "managed execution profile and host bridge are not implemented",
                None,
            )]);
        }
        let root = validate_sources(project, &options.limits)?;
        // Protect every dependency before publishing even a raw cache entry.
        let mut artifacts = ArtifactPlan::project(root, &project.modules, options)?;
        let fingerprint = project_fingerprint(project);
        report.input_fingerprint = fingerprint;
        // Syntax from the current filesystem request is also available on a
        // semantic-cache hit. Never recover this hint from cached Typed names.
        let stable_source = if super::stable_source_identity::eligible(options) {
            loaded_syntax.and_then(|syntax| {
                super::stable_source_identity::capture(
                    project.root,
                    fingerprint,
                    options.limits.max_functions,
                    &project.modules,
                    syntax
                        .iter()
                        .map(|loaded| (&loaded.parsed.module, loaded.stable_module_key.as_deref())),
                )
            })
        } else {
            None
        };
        report.source_files = project
            .modules
            .iter()
            .map(|m| (m.file_id, m.path.clone()))
            .collect();
        let identity = project_key(root, options, fingerprint, Phase::Parse, false);
        let target = options.target.fingerprint();
        // Only borrow syntax: prepare_project must still release obsolete large
        // semantic owners before the first current Parse query executes.
        let retained_syntax = loaded_syntax
            .is_none()
            .then(|| self.retained_project_syntax(root))
            .flatten();
        self.prepare_project(&identity, target, project.modules.len());
        let key = |phase, depends_on_target| phase_key(&identity, phase, depends_on_target, target);
        report.record_driver_cost("project_setup", setup_start);
        let mut semantic = super::semantic_cache::Store::new(options, &key(Phase::Types, true));
        if let Some(store) = &mut semantic {
            store.bind_path(&artifacts.semantic_cache(&store.path())?);
            report.disk_cache = if store.path().exists() {
                DiskCacheStatus::Rebuilt
            } else {
                DiskCacheStatus::Missing
            };
        }
        if (semantic.is_none() || options.reuse_producers())
            // Discovery may have parsed changed files while returning to a
            // retained parent. In research mode its complete current sources
            // still permit the same closed DB/owner validation; supplied syntax
            // is not a reason to decode disk IR or leave accelerators stale.
            && (loaded_syntax.is_none() || options.reuse_producers())
            && options.stop_after >= Stage::Types
            && options.emit_dir.is_none()
            && options.cache_dir.is_none()
            && let Some(frontend) = self.reuse_project_frontend(
                &key(Phase::Types, true),
                &project.modules,
                options,
                report,
                observer,
            )?
        {
            if options.reuse_producers() {
                report.disk_cache = DiskCacheStatus::Resident;
            }
            report.phase = Phase::Types;
            report.scalar_interfaces = frontend.typed.interfaces.clone();
            if options.stop_after == Stage::Types {
                return Ok(());
            }
            return self.finish_typed(
                options,
                report,
                observer,
                TypedInput::Program(frontend.typed),
                frontend.snapshot,
                artifacts,
                stable_source.as_ref(),
            );
        }
        if let Some(store) = &semantic {
            let cache_start = cost_start(options.profile_compilation);
            let restored = preloaded_semantic.or_else(|| store.load(options).map(Box::new));
            report.record_driver_cost("semantic_cache_read", cache_start);
            if let Some(pair) = restored
                && pair.0.module.source_fingerprint == fingerprint
                && pair.0.module.module_id == project.root
                && pair.0.module.target == options.target
            {
                store.remember_packed_epoch(options, &mut self.semantic_publication);
                let (typed, candidate) = *pair;
                let before = Box::new(report.clone());
                let imported = self.query_with_disposition(
                    key(Phase::Types, true),
                    &[],
                    options,
                    report,
                    observer,
                    || {
                        Ok(RawSnapshot::ProjectTyped(
                            super::session::ProjectTypedOwner::restored(typed),
                        ))
                    },
                    QueryDisposition::Reused,
                );
                let attempt = imported.and_then(|(raw, snapshot)| {
                    let RawSnapshot::ProjectTyped(typed) = raw else {
                        return Err(infra(
                            "E-QUERY-KIND",
                            "restored frontend has another snapshot kind",
                        ));
                    };
                    report.phase = Phase::Types;
                    report.cache_hit = true;
                    report.disk_cache = DiskCacheStatus::Hit;
                    report.scalar_interfaces = typed.interfaces.clone();
                    self.finish_typed(
                        options,
                        report,
                        observer,
                        TypedInput::CachedProgram(typed.program().clone(), Arc::new(candidate)),
                        snapshot,
                        artifacts.clone(),
                        stable_source.as_ref(),
                    )
                });
                match attempt {
                    Ok(()) => return Ok(()),
                    Err(errors) => {
                        // A decodable but rejected candidate cannot change the
                        // source result. Remove imported raw queries and rerun
                        // the current sources; cancellation is never retried.
                        self.clear();
                        if options.cancellation.is_cancelled()
                            || !matches!(report.phase, Phase::Types | Phase::Analyze)
                        {
                            return Err(errors);
                        }
                        report.restart_attempt(*before, "rejected_semantic_candidate", errors);
                        report.disk_cache = DiskCacheStatus::Rebuilt;
                    }
                }
            }
        }
        let mut parsed = Vec::new();
        let mut producer_syntax = options.reuse_producers().then(Vec::new);
        let mut dependencies: Vec<Arc<QuerySnapshot>> = Vec::new();
        let mut remaining = options.limits.clone();
        for (index, source) in project.modules.iter().enumerate() {
            if options.cache_dir.is_some() {
                disk_source(
                    source,
                    options,
                    report,
                    &query_key(source, options, Phase::Parse, false),
                )?;
            }
            let mut parse_options = options.clone();
            parse_options.limits = remaining.clone();
            // Tokens also bound front-end work across all modules. This is a
            // conservative project quota, including trivia and EOF tokens.
            parse_options.limits.max_tokens = remaining.max_tokens.min(remaining.max_nodes);
            let loaded = loaded_syntax.map(|syntax| &syntax[index]);
            if let Some(loaded) = loaded
                && let Some(snapshot) = &loaded.snapshot
            {
                // The single-source bridge has already executed this exact
                // Parse query and recorded its observer event. Reuse the real
                // published dependency without a second query or event.
                charge_frontend(&mut remaining, &loaded.parsed)?;
                parsed.push(loaded.parsed.clone());
                if let Some(inputs) = &mut producer_syntax {
                    inputs.push(loaded.producer.clone());
                }
                dependencies.push(snapshot.clone());
                continue;
            }
            let (parse_key, borrowed) = if loaded.is_none() {
                self.project_parse_input(source, &parse_options, retained_syntax.as_deref())?
            } else {
                (self.parse_key(source, &parse_options), None)
            };
            let producer_input = if options.reuse_producers() {
                match loaded {
                    Some(loaded) => loaded.producer.clone().map(|p| (p, loaded.reused)),
                    None => Some(self.producer_parse(source, &parse_options.limits)),
                }
            } else {
                None
            };
            let reused = producer_input.as_ref().map_or_else(
                || loaded.map_or(borrowed.is_some(), |syntax| syntax.reused),
                |(_, reused)| *reused,
            );
            let (raw, snapshot) = self.query_with_disposition(
                parse_key,
                &[],
                options,
                report,
                observer,
                || {
                    if let Some((producer, _)) = &producer_input {
                        let parsed = producer.parsed_shared();
                        if !parsed.diagnostics.is_empty() {
                            return Err(parsed.diagnostics.clone());
                        }
                        return Ok(RawSnapshot::Parse(parsed));
                    }
                    if let Some(syntax) = loaded {
                        // load_sources parsed these immutable bytes with the
                        // same per-file remaining token/node/depth budgets.
                        // Its byte budget was charged when reading each file;
                        // validate_sources has independently checked the total.
                        return Ok(RawSnapshot::Parse(syntax.parsed.clone()));
                    }
                    if let Some(parsed) = borrowed {
                        return Ok(RawSnapshot::Parse(parsed));
                    }
                    let result = zil_syntax::parse_with_limits(source, &parse_options.limits);
                    if !result.diagnostics.is_empty() {
                        return Err(result.diagnostics);
                    }
                    Ok(RawSnapshot::Parse(Arc::new(result)))
                },
                if reused {
                    QueryDisposition::Reused
                } else {
                    QueryDisposition::Executed
                },
            )?;
            report.cache_hit |= reused;
            let RawSnapshot::Parse(result) = raw else {
                return Err(infra(
                    "E-QUERY-KIND",
                    "project parse query returned another snapshot kind",
                ));
            };
            charge_frontend(&mut remaining, &result)?;
            parsed.push(result);
            if let Some(inputs) = &mut producer_syntax {
                inputs.push(producer_input.map(|(source, _)| source));
            }
            dependencies.push(snapshot);
        }
        report.phase = Phase::Parse;
        artifacts.dump_lazy(report, options, "syntax", |_| format!("{parsed:#?}"))?;
        if options.stop_after == Stage::Parse {
            return Ok(());
        }
        let syntax_modules = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
        let locked = if let Some(lock) = &options.syntax_lock {
            let mut syntax_paths = project.paths.clone();
            if let Some(scoped) = &options.scoped_imports {
                for entry in &lock.entries {
                    for ((_, path), module) in scoped.iter() {
                        if path != &entry.path {
                            continue;
                        }
                        if syntax_paths
                            .insert(entry.path.clone(), *module)
                            .is_some_and(|previous| previous != *module)
                        {
                            return Err(reject(
                                "E-SYNTAX-LOCK-AMBIGUOUS",
                                None,
                                format!(
                                    "syntax provider {} resolves to different project modules",
                                    entry.path.join(".")
                                ),
                            ));
                        }
                    }
                }
            }
            Some(lock.resolve(&project.modules, &syntax_modules, &syntax_paths)?)
        } else {
            None
        };
        let imports = resolve_imports(
            project,
            &parsed,
            &options.limits,
            locked.as_ref(),
            options.scoped_imports.as_deref(),
        )?;
        // Finite #if/#for declaration selection is part of Expand even for a
        // Core stop. Ordinary value #(...) remains a Core CT expression and
        // is only executed in the later Ct stage.
        let selection_needed = project
            .modules
            .iter()
            .zip(&parsed)
            .any(|(source, parsed)| super::ct_selection::needed(source, parsed));
        // This is only a cheap candidate filter, never a CT authority or a
        // classification result. A plain `#` belongs to the existing CT
        // producer. The artifact materializer uses the source compiler's
        // bounded domain classifier at each mixed-project expression.
        let artifact_needed = options.syntax_lock.is_some()
            || project
                .modules
                .iter()
                .zip(&parsed)
                .any(|(source, parsed)| artifact_candidate(source, parsed));
        let producer_sources =
            if options.reuse_producers() && !selection_needed && options.syntax_lock.is_none() {
                producer_syntax.and_then(|inputs| inputs.into_iter().collect::<Option<Vec<_>>>())
            } else {
                None
            };
        let mut producer = std::mem::take(&mut self.producer);
        let mut materializer = std::mem::take(&mut self.materializer);
        producer.set_cost_profiling(options.profile_compilation);
        let checkpoint_start = cost_start(options.profile_compilation);
        let producer_checkpoint = options
            .reuse_producers()
            .then(|| super::session::producer_checkpoint(&producer));
        let materializer_checkpoint = options.reuse_producers().then(|| materializer.checkpoint());
        report.record_driver_cost("producer_checkpoint", checkpoint_start);
        let mut producer_work = None;
        let mut materialization_work = None;
        let mut materialization_ns = 0;
        let expand_query = self.query(
            key(Phase::Expand, selection_needed || artifact_needed),
            &dependencies,
            options,
            report,
            observer,
            || {
                // The scalar materializer admits the current global environment
                // and replays only successful unchanged-module walks. It returns
                // no public AST proof and does not replace current CT or Verify.
                if artifact_needed && let Some(sources) = &producer_sources {
                    let start = cost_start(options.profile_compilation);
                    let admitted = materializer.materialize(
                        &producer, sources, &project.modules, &project.paths,
                        options.scoped_imports.as_deref(), &imports, project.root,
                        fingerprint, &options.target, &options.limits,
                    );
                    if let Some(start) = start {
                        materialization_ns += start.elapsed().as_nanos();
                    }
                    materialization_work = Some(materializer.last_stats());
                    if let Some(admitted) = admitted? {
                        let result = producer.expand_parsed(
                            sources, &imports, admitted.remaining(), &options.target,
                            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                                root: project.root,
                                aliases: &project.paths,
                                declarations: zil_expand::source_context::DeclarationPlacement::Unavailable,
                            },
                        );
                        producer_work = Some(super::session::producer_work(&producer));
                        if let Some(expanded) = result? {
                            let (produced, maps) = expanded.into_program(project.root, fingerprint);
                            return Ok(RawSnapshot::Project(Arc::new(ExpandedProject {
                                core: produced.program().clone(), maps,
                                selection: super::ct_selection::Receipt::default(),
                                ct_usage: Some(admitted.usage().clone()), ct_previews: Vec::new(),
                                syntax: Arc::new(ProjectSyntax {
                                    root: (root.file_id, root.module_id, root.path.clone()),
                                    entries: dependencies.iter().zip(&parsed)
                                        .map(|(snapshot, parsed)| (snapshot.key.clone(), parsed.clone())).collect(),
                                }),
                                producer_epoch: Some(capture_producer_epoch(&produced,&producer,&materializer,sources,options)),
                            producer: Some(produced),
                            })));
                        }
                    }
                }
                if !artifact_needed && let Some(sources) = &producer_sources {
                    let result = producer.expand_parsed(
                        sources,
                        &imports,
                        &options.limits,
                        &options.target,
                        zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                            root: project.root,
                            aliases: &project.paths,
                            declarations: zil_expand::source_context::DeclarationPlacement::Original,
                        },
                    );
                    producer_work = Some(super::session::producer_work(&producer));
                    if let Some(expanded) = result? {
                        let (produced, maps) = expanded.into_program(project.root, fingerprint);
                        return Ok(RawSnapshot::Project(Arc::new(ExpandedProject {
                            core: produced.program().clone(),
                            maps,
                            selection: super::ct_selection::Receipt::default(),
                            ct_usage: None,
                            ct_previews: Vec::new(),
                            syntax: Arc::new(ProjectSyntax {
                                root: (root.file_id, root.module_id, root.path.clone()),
                                entries: dependencies.iter().zip(&parsed)
                                    .map(|(snapshot, parsed)| (snapshot.key.clone(), parsed.clone())).collect(),
                            }),
                            producer_epoch: Some(capture_producer_epoch(&produced,&producer,&materializer,sources,options)),
                            producer: Some(produced),
                        })));
                    }
                }
                let mut modules = Vec::new();
                let mut maps = Vec::new();
                let syntax = parsed.iter().map(|p| &p.module).collect::<Vec<_>>();
                let selected = if selection_needed {
                    Some(super::ct_selection::select(
                        &project.modules,
                        &syntax,
                        &imports,
                        project.root,
                        fingerprint,
                        &options.target,
                        &options.limits,
                    )?)
                } else {
                    None
                };
                let materialized = selected
                    .as_ref()
                    .map(|selected| selected.modules.iter().collect::<Vec<_>>());
                let selected_syntax = materialized.as_deref().unwrap_or(&syntax);
                let mut ct_result = if artifact_needed {
                    let mut remaining = selected.as_ref().map_or_else(|| Ok(options.limits.clone()),
                        |selected| selected.receipt.remaining(&options.limits))?;
                    remaining.max_ct_steps = remaining.max_ct_steps.min(options.limits.max_ct_steps);
                    let start = cost_start(options.profile_compilation);
                    let result = super::ct_artifact::materialize_scoped(
                        &project.modules, &parsed, selected_syntax, &project.paths, options.scoped_imports.as_deref(), &imports,
                        project.root, fingerprint, &options.target, &remaining, locked.as_ref(),
                        selected.is_none(),
                    );
                    if let Some(start) = start {
                        materialization_ns += start.elapsed().as_nanos();
                    }
                    Some(result?)
                } else { None };
                let mut runtime_imports = imports.clone();
                if let Some(result) = &mut ct_result {
                    for (module, import) in &result.required_imports {
                        let index = project.modules.iter().position(|source| source.module_id == *module)
                            .ok_or_else(|| infra("E-CT-IMPORT", "generated caller is outside the frozen project"))?;
                        if !runtime_imports[index].iter().any(|existing| existing.alias == import.alias && existing.module == import.module) {
                            runtime_imports[index].push(import.clone());
                        }
                    }
                    let active = result.modules.iter().map(|module| (module.id,
                        module.items.iter().any(|item| !matches!(item.kind, ast::ItemKind::Import(_)))))
                        .collect::<BTreeMap<_, _>>();
                    let mut reachable = BTreeSet::new();
                    let mut queue = VecDeque::from([project.root]);
                    while let Some(module) = queue.pop_front() {
                        if !reachable.insert(module) { continue; }
                        if let Some(index) = project.modules.iter().position(|source| source.module_id == module) {
                            for import in &runtime_imports[index] {
                                if active.get(&import.module).copied().unwrap_or(false) { queue.push_back(import.module); }
                            }
                        }
                    }
                    for (index, module_imports) in runtime_imports.iter_mut().enumerate().take(result.modules.len()) {
                        if !reachable.contains(&result.modules[index].id) {
                            result.retain_items(index, |_| false)?;
                            module_imports.clear();
                            continue;
                        }
                        module_imports.retain(|import| reachable.contains(&import.module));
                        let importer = result.modules[index].id;
                        result.retain_items(index, |item| match &item.kind {
                            ast::ItemKind::Import(import) => {
                                if import.syntax || matches!(import.path.as_slice(), [a,b] if a == "std" && (b == "meta" || b == "syntax")) { return false; }
                                options.scoped_imports.as_ref().and_then(|paths| paths.get(&(importer, import.path.clone())))
                                    .or_else(|| project.paths.get(&import.path)).is_none_or(|target| reachable.contains(target))
                            }
                            _ => true,
                        })?;
                    }
                }
                let runtime_phase = ct_result.as_mut().and_then(|result|
                    result.take_runtime_phase(&project.modules, &parsed, &project.paths));
                let generated_syntax = ct_result.as_ref().map(|result| result.modules.iter().collect::<Vec<_>>());
                let syntax = generated_syntax.as_deref().unwrap_or(selected_syntax);
                let context=zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                    root:project.root,
                    aliases:&project.paths,
                    declarations:if let Some(origins) = ct_result.as_ref().and_then(|result| result.declaration_origins.as_deref()) {
                        zil_expand::source_context::DeclarationPlacement::Aligned(origins)
                    } else if selected.is_some() || ct_result.is_some() {
                        zil_expand::source_context::DeclarationPlacement::Unavailable
                    } else {
                        zil_expand::source_context::DeclarationPlacement::Original
                    },
                };
                let expand_limits = ct_result.as_ref().and_then(|result| result.lineage_remaining.as_ref()).unwrap_or(&options.limits);
                // A word such as an ordinary `io.emit` is only a broad CT
                // candidate. Keep its complete materializer and quota work:
                // even a semantic no-op can fail an earlier budget boundary.
                // Only after that succeeds may the sealed scalar classifier
                // prove no body/type rewrite was possible. The remaining
                // checks bind actual declaration retention and import pruning;
                // they do not recursively compare a newly produced Core.
                let retained_original = producer_sources.is_some()
                    && runtime_phase.is_none()
                    && runtime_imports == imports
                    && ct_result.as_ref().is_some_and(|result| {
                        result.required_imports.is_empty()
                            && result.previews.is_empty()
                            && result.declaration_origins.is_none()
                            && result.modules.len() == parsed.len()
                            && result.modules.iter().zip(&parsed).all(|(output, original)| {
                                output.id == original.module.id
                                    && output.span == original.module.span
                                    && output.items.len() == original.module.items.len()
                                    && output.items.iter().zip(&original.module.items).all(|(a, b)| {
                                        a.span == b.span && a.public == b.public
                                            && a.attributes == b.attributes
                                            && std::mem::discriminant(&a.kind) == std::mem::discriminant(&b.kind)
                                    })
                            })
                    });
                let mut produced = None;
                if retained_original && let Some(sources) = &producer_sources {
                    let result = producer.expand_parsed(
                        sources, &runtime_imports, expand_limits, &options.target, context,
                    );
                    producer_work = Some(super::session::producer_work(&producer));
                    if let Some(expanded) = result? {
                        let (program, source_maps) = expanded.into_program(project.root, fingerprint);
                        produced = Some(program);
                        maps = source_maps;
                    }
                }
                if produced.is_none() {
                    let expanded = if let Some(phase) = &runtime_phase {
                        zil_expand::expand_program_with_runtime_ct(
                            &project.modules, syntax, &runtime_imports, expand_limits, &options.target, context, phase,
                        )?
                    } else {
                        zil_expand::expand_program_with_limits_target_context(
                            &project.modules, syntax, &runtime_imports, expand_limits, &options.target, context,
                        )?
                    };
                    for (module, map) in expanded {
                        modules.push(module);
                        maps.push(Arc::new(map));
                    }
                }
                if let Some(phase) = runtime_phase {
                    ct_result.as_mut().expect("runtime phase came from materialization").finish_runtime_phase(phase)?;
                }
                Ok(RawSnapshot::Project(Arc::new(ExpandedProject {
                    core: produced.as_ref().map(|program| program.program().clone()).unwrap_or_else(|| Arc::new(CoreProgram {
                        root: project.root,
                        source_fingerprint: fingerprint,
                        modules,
                    })),
                    maps,
                    selection: selected
                        .map_or_else(super::ct_selection::Receipt::default, |selected| {
                            selected.receipt
                        }),
                    ct_usage: ct_result.as_ref().map(|result| result.usage.clone()),
                    ct_previews: ct_result.map(|result| result.previews).unwrap_or_default(),
                    syntax: Arc::new(ProjectSyntax {
                        root: (root.file_id, root.module_id, root.path.clone()),
                        entries: dependencies
                            .iter()
                            .zip(&parsed)
                            .map(|(snapshot, parsed)| (snapshot.key.clone(), parsed.clone()))
                            .collect(),
                    }),
                    producer_epoch: produced.as_ref().zip(producer_sources.as_deref()).map(|(core,inputs)|
                        capture_producer_epoch(core,&producer,&materializer,inputs,options)),
                    producer: produced,
                })))
            },
        );
        report.producer_work = producer_work;
        report.materialization_work = materialization_work;
        report.materialization_ns += materialization_ns;
        self.producer = if expand_query.is_ok() {
            producer
        } else {
            producer_checkpoint.unwrap_or_default()
        };
        self.materializer = if expand_query.is_ok() {
            materializer
        } else {
            materializer_checkpoint.unwrap_or_default()
        };
        let (raw, expand_snapshot) = expand_query?;
        let RawSnapshot::Project(expanded) = raw else {
            return Err(infra(
                "E-QUERY-KIND",
                "project expand query returned another snapshot kind",
            ));
        };
        report.phase = Phase::Expand;
        artifacts.dump_lazy(report, options, "core", |_| format!("{:#?}", expanded.core))?;
        artifacts.dump_lazy(report, options, "source-map", |_| {
            format!("{:#?}", expanded.maps)
        })?;
        if selection_needed {
            artifacts.dump_lazy(report, options, "selection", |_| {
                expanded.selection.to_json()
            })?;
        }
        if artifact_needed {
            artifacts.dump_lazy(report, options, "ct-syntax", |_| {
                format!(
                    "[{}]",
                    expanded
                        .ct_previews
                        .iter()
                        .map(|preview| preview.json())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })?;
        }
        if options.stop_after == Stage::Core {
            return Ok(());
        }
        let mut raw_inference = std::mem::take(&mut self.raw_inference);
        let inference_checkpoint = raw_inference.clone();
        let mut inference_work = None;
        let ct_result = self.query(
            key(Phase::Ct, true),
            std::slice::from_ref(&expand_snapshot),
            options,
            report,
            observer,
            || {
                let inference = if options.reuse_producers() {
                    if let Some(produced) = &expanded.producer {
                        if options.push_reuse {
                            zil_types::infer_produced_program_for_ct_with_session(
                                produced,
                                &options.limits,
                                &mut raw_inference,
                            )?
                        } else {
                            zil_types::infer_produced_program_for_ct(produced, &options.limits)?
                        }
                    } else {
                        zil_types::infer_shared_program_for_ct(&expanded.core, &options.limits)?
                    }
                } else {
                    zil_types::infer_shared_program_for_ct(&expanded.core, &options.limits)?
                };
                inference_work = inference.raw_inference_stats().cloned().map(Box::new);
                let inferred = inference.program();
                let mut remaining = expanded.selection.remaining(&options.limits)?;
                if let Some(usage) = &expanded.ct_usage {
                    remaining.max_ct_steps = remaining
                        .max_ct_steps
                        .checked_sub(usage.steps)
                        .ok_or_else(|| {
                            vec![Diagnostic::limit(
                                Phase::Ct,
                                "Type/Syntax CT step budget exceeded",
                                None,
                            )]
                        })?;
                    remaining.max_nodes = remaining
                        .max_nodes
                        .checked_sub(usage.generated_nodes as usize)
                        .ok_or_else(|| {
                            vec![Diagnostic::limit(
                                Phase::Ct,
                                "Type/Syntax CT generated node budget exceeded",
                                None,
                            )]
                        })?;
                }
                let context = zil_ct::SourceProgramContext::new(inferred, &remaining)?;
                let initial_remaining = remaining.clone();
                let mut unchanged = true;
                for module in &inferred.modules {
                    let Some(usage) = zil_ct::validate_unchanged_source(
                        &module.core,
                        &options.target,
                        &remaining,
                        Some(&context),
                    ) else {
                        unchanged = false;
                        break;
                    };
                    charge_ct_usage(&mut remaining, usage?, module.core.functions.len())?;
                }
                if unchanged {
                    drop(context);
                    return Ok(RawSnapshot::ProjectCore(Arc::new(
                        inference.into_prepared(),
                    )));
                }
                // A plain prefix performed no CT queries or mutation. The full
                // evaluator starts with its original logical allowance.
                remaining = initial_remaining;
                let mut modules = Vec::new();
                for module in &inferred.modules {
                    // Restrict execution to the remaining graph allowance BEFORE
                    // entering the CT verifier/VM, then deduct actual usage.
                    let compiler = super::ct_program::Compiler {
                        program: inferred,
                        module: module.core.module_id,
                        target: &options.target,
                    };
                    let (core, usage) = zil_ct::evaluate_source_constants_with_compiler(
                        &module.core,
                        &options.target,
                        &remaining,
                        Some(&context),
                        &compiler,
                    )?;
                    charge_ct_usage(&mut remaining, usage, core.functions.len())?;
                    modules.push(ProgramModule {
                        core: Arc::new(core),
                        imports: module.imports.clone(),
                        exports: module.exports.clone(),
                    });
                }
                drop(context);
                Ok(RawSnapshot::ProjectCore(Arc::new(
                    inference.after_evaluation(CoreProgram {
                        root: project.root,
                        source_fingerprint: fingerprint,
                        modules,
                    }),
                )))
            },
        );
        self.raw_inference = if ct_result.is_ok() {
            raw_inference
        } else {
            inference_checkpoint
        };
        report.raw_inference_work = inference_work;
        let (raw, ct_snapshot) = ct_result?;
        if options.push_reuse
            && let Some(epoch) = &expanded.producer_epoch
            && Arc::ptr_eq(&epoch.core, &expanded.core)
            && expanded
                .producer
                .as_ref()
                .is_some_and(|produced| Arc::ptr_eq(produced.program(), &epoch.core))
            && epoch.limits == options.limits
            && epoch.target == options.target.fingerprint()
        {
            if report.raw_inference_work.is_some() {
                if let Some(checkpoint) = self.raw_inference.checkpoint_for(&epoch.core) {
                    let _ = epoch.raw_inference.set(checkpoint);
                }
            } else if let Some(checkpoint) = epoch
                .raw_inference
                .get()
                .and_then(|session| session.checkpoint_for(&epoch.core))
            {
                self.raw_inference = checkpoint;
            }
        }
        let RawSnapshot::ProjectCore(prepared) = raw else {
            return Err(infra(
                "E-QUERY-KIND",
                "project CT query returned another snapshot kind",
            ));
        };
        let core = prepared.program();
        report.phase = Phase::Ct;
        artifacts.dump_lazy(report, options, "ct-core", |_| format!("{core:#?}"))?;
        check_entry_contract(
            &core
                .modules
                .iter()
                .find(|m| m.core.module_id == project.root)
                .expect("validated root")
                .core,
        )?;
        let mut function_store =
            super::semantic_cache::FunctionStore::new(options, &key(Phase::Types, true), root);
        if let Some(store) = &mut function_store {
            store.bind_path(&artifacts.semantic_cache(&store.path())?);
        }
        let mut function_types = std::mem::take(&mut self.function_types);
        function_types.set_cost_profiling(options.profile_compilation);
        let function_restore_start = cost_start(options.profile_compilation);
        if let Some(store) = &function_store {
            store.restore(
                &mut function_types,
                options,
                self.semantic_publication.function_packs(),
            );
        }
        report.record_driver_cost("function_cache_read", function_restore_start);
        let checkpoint_start = cost_start(options.profile_compilation);
        let checkpoint = super::session::function_checkpoint(&function_types);
        report.record_driver_cost("function_checkpoint", checkpoint_start);
        // Paths identify owners; current immutable Core, binding IDs, complete
        // limits/target and observed declarations still prove validity inside
        // FunctionSession. A path or source fingerprint grants no reuse alone.
        let owner_start = cost_start(options.profile_compilation);
        let reactive_owners = (options.reactive_queries || options.reuse_producers()).then(|| {
            project
                .modules
                .iter()
                .map(|source| (source.module_id, (root.path.clone(), source.path.clone())))
                .collect::<BTreeMap<_, _>>()
        });
        report.record_driver_cost("owner_binding", owner_start);
        let mut function_work = None;
        let mut types_push_work = None;
        let mut push_types = std::mem::take(&mut self.push_types);
        let push_checkpoint = push_types.clone();
        let typed_result = self.query(
            key(Phase::Types, true),
            std::slice::from_ref(&ct_snapshot),
            options,
            report,
            observer,
            || {
                if options.push_reuse {
                    return push_types
                        .apply(&prepared, &options.target, &options.limits)
                        .map(|update| {
                            types_push_work = Some(Box::new(update.stats));
                            RawSnapshot::ProjectTyped(super::session::ProjectTypedOwner::product(
                                update.program,
                                key(Phase::Types, true),
                                &push_types,
                            ))
                        });
                }
                let checked = if options.reactive_queries
                    && let Some(owners) = &reactive_owners
                {
                    function_types.check_prepared_program_with_owners(
                        &prepared,
                        &options.target,
                        &options.limits,
                        owners,
                    )
                } else if options.reuse_producers()
                    && let Some(owners) = &reactive_owners
                {
                    function_types.check_prepared_program_with_producer_owners(
                        &prepared,
                        &options.target,
                        &options.limits,
                        owners,
                    )
                } else {
                    function_types.check_prepared_program(
                        &prepared,
                        &options.target,
                        &options.limits,
                    )
                };
                function_work = Some(super::session::function_work(&function_types));
                checked.map(|typed| {
                    RawSnapshot::ProjectTyped(super::session::ProjectTypedOwner::produced(
                        typed,
                        key(Phase::Types, true),
                        &function_types,
                        options.reuse_producers(),
                    ))
                })
            },
        );
        report.function_type_work = function_work;
        report.types_push_work = types_push_work;
        self.push_types = if typed_result.is_ok() {
            push_types
        } else {
            push_checkpoint
        };
        self.function_types = if typed_result.is_ok() {
            function_types
        } else {
            checkpoint
        };
        let (raw, typed_snapshot) = typed_result?;
        let RawSnapshot::ProjectTyped(typed) = raw else {
            return Err(infra(
                "E-QUERY-KIND",
                "project types query returned another snapshot kind",
            ));
        };
        if report.function_type_work.is_none() && report.types_push_work.is_none() {
            self.activate_types_epoch(&typed, &typed_snapshot.key, options);
        }
        report.phase = Phase::Types;
        report.scalar_interfaces = typed.interfaces.clone();
        artifacts.dump_lazy(report, options, "typed", |_| format!("{:#?}", typed.module))?;
        artifacts.dump_lazy(report, options, "interfaces", |_| {
            format!("{:#?}", typed.interfaces)
        })?;
        artifacts.dump_lazy(report, options, "task-interfaces", |_| {
            format!("{:#?}", typed.task_interfaces)
        })?;
        if options.stop_after == Stage::Types {
            return Ok(());
        }
        let result = self.finish_typed(
            options,
            report,
            observer,
            TypedInput::Program(typed.program().clone()),
            typed_snapshot.clone(),
            artifacts.clone(),
            stable_source.as_ref(),
        );
        let persistence_start = cost_start(options.profile_compilation);
        if result.is_ok()
            && let Some(store) = &semantic
            && let Some(candidate) = self.analysis_candidate(&typed_snapshot)
        {
            if !options.push_reuse {
                self.semantic_publication
                    .bind_products(self.function_types.last_products());
            }
            let publication = store.save(
                typed.program(),
                &candidate,
                options,
                &artifacts,
                &mut self.semantic_publication,
            );
            if options.profile_compilation {
                report.semantic_cache_publication = publication;
            }
        }
        report.record_driver_cost("semantic_candidate_publication", persistence_start);
        let persistence_start = cost_start(options.profile_compilation);
        if result.is_ok()
            && let Some(store) = &function_store
        {
            let publication = store.save(
                &self.function_types,
                options,
                &artifacts,
                self.semantic_publication.function_packs(),
            );
            if options.profile_compilation
                && let Some((encoding, stats)) = publication
            {
                report.function_cache_encoding = Some(Box::new(encoding));
                report.function_cache_publication = Some(stats);
            }
        }
        report.record_driver_cost("function_cache_publication", persistence_start);
        if result.is_ok() {
            // Both stores may have filled shared fragment/pack placement hints.
            // Attach only to the exact successful producer owner, never a cache
            // restored value or the currently active but unrelated epoch.
            typed.checkpoint_publication(&typed_snapshot.key, &self.semantic_publication);
        }
        result
    }
}

fn limit(phase: Phase, message: &str) -> Vec<Diagnostic> {
    vec![Diagnostic::limit(phase, message, None)]
}
fn take(remaining: &mut usize, count: usize, phase: Phase, message: &str) -> CompileResult<()> {
    *remaining = remaining
        .checked_sub(count)
        .ok_or_else(|| limit(phase, message))?;
    Ok(())
}
fn reject(code: &str, span: Option<Span>, message: impl Into<String>) -> Vec<Diagnostic> {
    vec![Diagnostic::error(Phase::Expand, code, span, message)]
}
pub(super) fn valid_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || zil_syntax::unicode::is_xid_start(c))
        && chars.all(|c| c == '_' || zil_syntax::unicode::is_xid_continue(c))
}

fn validate_sources<'a>(
    project: &'a ProjectSources,
    limits: &Limits,
) -> CompileResult<&'a SourceUnit> {
    if project.modules.len() > limits.max_nodes || project.paths.len() > limits.max_nodes {
        return Err(limit(Phase::Parse, "project module budget exceeded"));
    }
    let mut bytes = limits.max_source_bytes;
    let mut work = limits.max_analysis_steps;
    let mut modules = BTreeSet::new();
    let mut files = BTreeSet::new();
    for source in &project.modules {
        take(
            &mut bytes,
            source.text.len(),
            Phase::Parse,
            "project source byte budget exceeded",
        )?;
        take(
            &mut work,
            source.path.len(),
            Phase::Parse,
            "project source path budget exceeded",
        )?;
        if !modules.insert(source.module_id) || !files.insert(source.file_id) {
            return Err(reject(
                "E-MODULE-ID",
                Some(source.span()),
                "module and file identities must be unique",
            ));
        }
    }
    for (path, id) in &project.paths {
        if path.first().is_some_and(|segment| segment == "std") {
            let Some(expected) = stdlib::source(path) else {
                return Err(reject(
                    "E-STD-RESERVED",
                    None,
                    "compiler CT modules and unknown standard modules cannot be supplied",
                ));
            };
            take(
                &mut work,
                project.modules.len(),
                Phase::Expand,
                "standard library source identity lookup budget exceeded",
            )?;
            let source = project
                .modules
                .iter()
                .find(|source| source.module_id == *id)
                .ok_or_else(|| reject("E-MODULE-ID", None, "standard source module is missing"))?;
            take(
                &mut work,
                expected.len(),
                Phase::Expand,
                "standard library source identity byte budget exceeded",
            )?;
            if source.text.as_str() != expected {
                return Err(reject(
                    "E-STD-RESERVED",
                    None,
                    "standard library bytes must match the distributed ordinary source",
                ));
            }
        }
        take(
            &mut work,
            path.len(),
            Phase::Expand,
            "project import path budget exceeded",
        )?;
        if path.is_empty() {
            return Err(reject(
                "E-MODULE-PATH",
                None,
                "import paths must contain identifier segments",
            ));
        }
        if path.len() > limits.max_call_depth {
            return Err(limit(
                Phase::Expand,
                "project import path depth budget exceeded",
            ));
        }
        for segment in path {
            take(
                &mut work,
                segment.len(),
                Phase::Expand,
                "project import path budget exceeded",
            )?;
            if !valid_segment(segment) {
                return Err(reject(
                    "E-MODULE-PATH",
                    None,
                    "import paths must contain identifiers without extensions or parent traversal",
                ));
            }
        }
        if !modules.contains(id) {
            return Err(reject(
                "E-MODULE-NOT-FOUND",
                None,
                "logical import path refers to an absent module",
            ));
        }
    }
    project
        .modules
        .iter()
        .find(|m| m.module_id == project.root)
        .ok_or_else(|| reject("E-MODULE-ROOT", None, "project root module does not exist"))
}

fn charge_frontend(remaining: &mut Limits, parsed: &ParseResult) -> CompileResult<()> {
    take(
        &mut remaining.max_tokens,
        parsed.tokens.len(),
        Phase::Parse,
        "project token budget exceeded",
    )?;
    take(
        &mut remaining.max_nodes,
        parsed.tokens.len(),
        Phase::Parse,
        "project front-end work budget exceeded",
    )
}

fn resolve_imports(
    project: &ProjectSources,
    parsed: &[Arc<ParseResult>],
    limits: &Limits,
    locked: Option<&zil_ir::ct_artifact::CtLockedProviderRegistry>,
    scoped: Option<&BTreeMap<(ModuleId, Vec<String>), ModuleId>>,
) -> CompileResult<Vec<Vec<CoreImport>>> {
    let mut remaining = limits.max_nodes;
    let known = project
        .modules
        .iter()
        .map(|source| source.module_id)
        .collect::<BTreeSet<_>>();
    let mut graph = Vec::new();
    for module in parsed {
        let mut imports = Vec::new();
        let mut syntax_aliases = BTreeSet::new();
        for item in &module.module.items {
            let ast::ItemKind::Import(import) = &item.kind else {
                continue;
            };
            take(
                &mut remaining,
                1,
                Phase::Expand,
                "project import edge budget exceeded",
            )?;
            unsupported_import(item, import)?;
            if import.path.first().is_some_and(|part| part == "std") {
                let compiler_module = matches!(import.path.as_slice(),
                    [a, b] if a == "std" && (b == "meta" || b == "syntax"));
                if import.syntax || (!compiler_module && stdlib::source(&import.path).is_none()) {
                    return Err(reject(
                        "E-STD-IMPORT",
                        Some(item.span),
                        "unknown or nonordinary standard library import",
                    ));
                }
                if compiler_module {
                    continue;
                }
            }
            if import.syntax && locked.is_none() {
                return Err(reject(
                    "E-SYNTAX-LOCK-MISSING",
                    Some(item.span),
                    "syntax provider use needs an explicit lock",
                ));
            }
            let id = scoped
                .filter(|_| import.path.first().is_none_or(|part| part != "std"))
                .and_then(|paths| paths.get(&(module.module.id, import.path.clone())))
                .or_else(|| project.paths.get(&import.path))
                .copied()
                .ok_or_else(|| {
                    reject(
                        "E-MODULE-NOT-FOUND",
                        Some(item.span),
                        format!("module {} was not supplied", import.path.join(".")),
                    )
                })?;
            if !known.contains(&id) {
                return Err(reject(
                    "E-MODULE-NOT-FOUND",
                    Some(item.span),
                    "scoped import target is absent from the source graph",
                ));
            }
            let alias = import
                .alias
                .clone()
                .or_else(|| import.path.last().cloned())
                .ok_or_else(|| reject("E-MODULE-PATH", Some(item.span), "empty import path"))?;
            if import.syntax {
                if !syntax_aliases.insert(alias) {
                    return Err(reject(
                        "E-SYNTAX-ALIAS",
                        Some(item.span),
                        "duplicate syntax provider alias",
                    ));
                }
                let path = import.path.join(".");
                if !locked.is_some_and(|registry| {
                    registry
                        .entries
                        .iter()
                        .any(|entry| entry.path == path && entry.export == "default")
                }) {
                    return Err(reject(
                        "E-SYNTAX-LOCK-MISSING",
                        Some(item.span),
                        format!("syntax provider {path} needs a locked default entry"),
                    ));
                }
                continue;
            }
            imports.push(CoreImport {
                alias,
                module: id,
                span: item.span,
            });
        }
        graph.push(imports);
    }
    check_graph(project, &graph, limits)?;
    Ok(graph)
}

fn unsupported_import(item: &ast::Item, _import: &ast::Import) -> CompileResult<()> {
    if item.public {
        return Err(vec![Diagnostic::unsupported(
            Phase::Expand,
            "public reexports require a later module interface",
            Some(item.span),
        )]);
    }
    Ok(())
}

fn check_graph(
    project: &ProjectSources,
    imports: &[Vec<CoreImport>],
    limits: &Limits,
) -> CompileResult<()> {
    let indices: BTreeMap<_, _> = project
        .modules
        .iter()
        .enumerate()
        .map(|(i, m)| (m.module_id, i))
        .collect();
    let mut incoming = vec![0usize; project.modules.len()];
    for edges in imports {
        for edge in edges {
            let count = &mut incoming[indices[&edge.module]];
            *count = count
                .checked_add(1)
                .ok_or_else(|| limit(Phase::Expand, "project import edge count overflow"))?;
        }
    }
    let mut ready: VecDeque<_> = incoming
        .iter()
        .enumerate()
        .filter_map(|(i, n)| (*n == 0).then_some(i))
        .collect();
    let mut depth = vec![0usize; project.modules.len()];
    let mut visited = 0;
    while let Some(index) = ready.pop_front() {
        visited += 1;
        for edge in &imports[index] {
            let next = indices[&edge.module];
            let next_depth = depth[index]
                .checked_add(1)
                .filter(|d| *d <= limits.max_call_depth)
                .ok_or_else(|| limit(Phase::Expand, "project import depth budget exceeded"))?;
            depth[next] = depth[next].max(next_depth);
            incoming[next] -= 1;
            if incoming[next] == 0 {
                ready.push_back(next);
            }
        }
    }
    if visited != project.modules.len() {
        return Err(vec![Diagnostic::unsupported(
            Phase::Expand,
            "module import cycles are not implemented; this is not a package-cycle language rule",
            None,
        )]);
    }
    Ok(())
}

fn project_key(
    root: &SourceUnit,
    options: &CompileOptions,
    fingerprint: Fingerprint,
    phase: Phase,
    target: bool,
) -> QueryKey {
    let mut key = query_key(root, options, phase, target);
    key.input = fingerprint;
    key
}
fn project_fingerprint(project: &ProjectSources) -> Fingerprint {
    let mut bytes = b"ZIL-SOURCE-PROJECT-1\0".to_vec();
    bytes.extend(project.root.0.to_le_bytes());
    bytes.extend((project.modules.len() as u64).to_le_bytes());
    for source in &project.modules {
        bytes.extend(source.module_id.0.to_le_bytes());
        bytes.extend(source.file_id.0.to_le_bytes());
        bytes.extend((source.path.len() as u64).to_le_bytes());
        bytes.extend(source.path.as_bytes());
        bytes.extend(source.fingerprint().0);
    }
    bytes.extend((project.paths.len() as u64).to_le_bytes());
    for (path, module) in &project.paths {
        bytes.extend((path.len() as u64).to_le_bytes());
        for segment in path {
            bytes.extend((segment.len() as u64).to_le_bytes());
            bytes.extend(segment.as_bytes());
        }
        bytes.extend(module.0.to_le_bytes());
    }
    Fingerprint::of(&bytes)
}

pub(super) fn resolve_file_module(
    root: &Path,
    logical: &[String],
    span: Span,
) -> CompileResult<PathBuf> {
    let mut base = root.to_path_buf();
    for segment in logical {
        base.push(segment);
    }
    let file = base.with_extension("zil");
    let directory = base.join("mod.zil");
    let present = |path: &Path| -> CompileResult<bool> {
        match std::fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(reject(
                "E-MODULE-NOT-FOUND",
                Some(span),
                format!("cannot inspect module entry {}: {error}", path.display()),
            )),
        }
    };
    let requested = match (present(&file)?, present(&directory)?) {
        (true, false) => file,
        (false, true) => directory,
        (true, true) => {
            return Err(reject(
                "E-MODULE-AMBIGUOUS",
                Some(span),
                format!(
                    "module {} has both {} and {}; keep exactly one entry",
                    logical.join("."),
                    file.display(),
                    directory.display(),
                ),
            ));
        }
        (false, false) => {
            return Err(reject(
                "E-MODULE-NOT-FOUND",
                Some(span),
                format!(
                    "module {} needs {} or {}; directories are not scanned",
                    logical.join("."),
                    file.display(),
                    directory.display(),
                ),
            ));
        }
    };
    let canonical = std::fs::canonicalize(&requested).map_err(|error| {
        reject(
            "E-MODULE-NOT-FOUND",
            Some(span),
            format!("cannot load {}: {error}", requested.display()),
        )
    })?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err(reject(
            "E-MODULE-PATH",
            Some(span),
            "module entry must resolve to a file inside the project root",
        ));
    }
    Ok(canonical)
}

/// A raw substring check deliberately over-rejects comments and identifiers.
/// Parser::at_word compares original bytes; both spellings that introduce an
/// import are excluded, including self-imports whose filesystem aliases change.
fn single_file_semantic_profiled(
    project: &ProjectSources,
    options: &CompileOptions,
    session: &CompilationSession,
    stats: Option<&mut SourceIngressStats>,
) -> Option<(TypedProgram, CandidatePlan)> {
    if options.profile != ExecutionProfile::Native
        || options.closed_executable
        || options.semantic_cache.is_none()
        || session.active_query_count() != 0
        || project.modules.len() != 1
        || project.modules[0].text.contains("import")
        || project.modules[0].text.contains("syntax")
    {
        return None;
    }
    exact_discovered_semantic_profiled(project, options, stats)
}

fn exact_discovered_semantic_profiled(
    project: &ProjectSources,
    options: &CompileOptions,
    mut stats: Option<&mut SourceIngressStats>,
) -> Option<(TypedProgram, CandidatePlan)> {
    // A failed optional probe cannot move a source/import error behind an
    // artifact error. The original discovery and pipeline report that error.
    let source = validate_sources(project, &options.limits).ok()?;
    let fingerprint = project_fingerprint(project);
    let key = project_key(source, options, fingerprint, Phase::Types, true);
    let mut store = super::semantic_cache::Store::new(options, &key)?;
    let mut artifacts = ArtifactPlan::project(source, &project.modules, options).ok()?;
    store.bind_path(&artifacts.semantic_cache(&store.path()).ok()?);
    let probe_started = stats.as_ref().map(|_| Instant::now());
    if let Some(stats) = stats.as_deref_mut() {
        stats.semantic_load_attempts += 1;
    }
    let pair = store.load(options);
    if let Some(stats) = stats {
        stats.semantic_probe_ns += probe_started
            .expect("profiled semantic probe")
            .elapsed()
            .as_nanos();
        if let Some((typed, _)) = &pair {
            stats.semantic_restored_functions += typed.module.functions.len();
        }
    }
    let (typed, candidate) = pair?;
    (typed.module.source_fingerprint == fingerprint
        && typed.module.module_id == project.root
        && typed.module.target == options.target)
        .then_some((typed, candidate))
}

fn resident_discovered_frontend(
    project: &ProjectSources,
    options: &CompileOptions,
    session: &CompilationSession,
) -> bool {
    if !options.reuse_producers()
        || options.profile != ExecutionProfile::Native
        || options.closed_executable
        || options.semantic_cache.is_none()
        || options.stop_after < Stage::Verify
        || options.emit_dir.is_some()
        || options.cache_dir.is_some()
        || session.active_query_count() != 0
    {
        return false;
    }
    let Ok(root) = validate_sources(project, &options.limits) else {
        return false;
    };
    let key = project_key(
        root,
        options,
        project_fingerprint(project),
        Phase::Types,
        true,
    );
    matches!(session.raw.get(&key), Some(RawSnapshot::ProjectTyped(_)))
        && session.database.read(&key).ok().flatten().is_some()
}

fn load_sources(
    project: &mut ProjectSources,
    path: &Path,
    options: &mut CompileOptions,
    session: &CompilationSession,
) -> CompileResult<LoadedProject> {
    load_sources_profiled(project, path, options, session, None)
}

fn load_sources_profiled(
    project: &mut ProjectSources,
    path: &Path,
    options: &mut CompileOptions,
    session: &CompilationSession,
    mut stats: Option<&mut SourceIngressStats>,
) -> CompileResult<LoadedProject> {
    options
        .cancellation
        .check(Phase::Driver)
        .map_err(|e| vec![e])?;
    let root_path = std::fs::canonicalize(path)
        .map_err(|e| infra("E-SOURCE-READ", format!("{}: {e}", path.display())))?;
    if let Some(manifest) = &options.project {
        manifest.check_lock()?;
        manifest.check_entry(&root_path)?;
    }
    let directory = root_path
        .parent()
        .ok_or_else(|| infra("E-SOURCE-READ", "entry must have a parent directory"))?;
    let mut remaining = options.limits.clone();
    if remaining.max_nodes == 0 {
        return Err(limit(Phase::Parse, "project module budget exceeded"));
    }
    let read_started = stats.as_ref().map(|_| Instant::now());
    let read = read_source(&root_path, options);
    if let Some(stats) = stats.as_deref_mut() {
        stats.source_read_ns += read_started.expect("profiled read").elapsed().as_nanos();
        if let Ok(source) = &read {
            stats.read(source);
        }
    }
    project.modules[0] = read?;
    take(
        &mut remaining.max_source_bytes,
        project.modules[0].text.len(),
        Phase::Parse,
        "project source byte budget exceeded",
    )?;
    if let Some(stem) = root_path
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| valid_segment(s))
    {
        project.paths.insert(vec![stem.into()], project.root);
    }
    // Preserve the original discovery cancellation boundary before replacing
    // parsing with an exact successful raw frontend lookup.
    options
        .cancellation
        .check(Phase::Parse)
        .map_err(|e| vec![e])?;
    if options.project.is_none()
        && project.modules.len() == 1
        && !project.modules[0].text.contains("import")
        && !project.modules[0].text.contains("syntax")
        && resident_discovered_frontend(project, options, session)
    {
        if let Some(stats) = stats.as_deref_mut() {
            stats.resident_routes += 1;
        }
        return Ok(LoadedProject::Resident);
    }
    if options.project.is_none()
        && let Some(pair) =
            single_file_semantic_profiled(project, options, session, stats.as_deref_mut())
    {
        return Ok(LoadedProject::Semantic(Box::new(pair)));
    }
    let mut prefetched = BTreeMap::new();
    let receipt_started = stats.as_ref().map(|_| Instant::now());
    let discovery = if options.project.is_none() {
        discovery_cache::load(project, &root_path, options, session, stats.as_deref_mut())?
    } else {
        None
    };
    if let Some(stats) = stats.as_deref_mut() {
        stats.discovery_receipt_ns += receipt_started
            .expect("profiled receipt")
            .elapsed()
            .as_nanos();
    }
    if let Some(attempt) = discovery {
        if attempt.complete && resident_discovered_frontend(&attempt.project, options, session) {
            *project = attempt.project;
            if let Some(stats) = stats.as_deref_mut() {
                stats.resident_routes += 1;
            }
            return Ok(LoadedProject::Resident);
        }
        if attempt.complete
            && let Some(pair) =
                exact_discovered_semantic_profiled(&attempt.project, options, stats.as_deref_mut())
        {
            *project = attempt.project;
            return Ok(LoadedProject::Semantic(Box::new(pair)));
        }
        // A receipt mismatch or semantic miss restores the original ordered
        // parser. Reuse immutable reads when it discovers the same canonical
        // file, including changed files read during the failed receipt probe.
        prefetched.extend(
            attempt
                .project
                .modules
                .into_iter()
                .skip(1)
                .map(|source| (PathBuf::from(&source.path), source)),
        );
    }
    let retained_syntax = session.retained_project_syntax(&project.modules[0]);
    let mut known = BTreeMap::from([(root_path.clone(), project.root)]);
    let mut stable_modules = super::stable_source_identity::eligible(options)
        .then(|| super::stable_source_identity::CanonicalModules::new(&root_path))
        .flatten();
    let mut index = 0;
    let mut syntax = Vec::new();
    while index < project.modules.len() {
        options
            .cancellation
            .check(Phase::Parse)
            .map_err(|e| vec![e])?;
        let mut parse_limits = remaining.clone();
        // Loading already charged each unique file against the project byte
        // budget. The lexer sees that file's checked size, not the budget for
        // files which have yet to be read.
        parse_limits.max_source_bytes = project.modules[index].text.len();
        parse_limits.max_tokens = remaining.max_tokens.min(remaining.max_nodes);
        // Match the exact query key project_pipeline will use. The loader
        // has separately charged source bytes; per-file token/node quotas and
        // every other query axis must still match, including source identity.
        let mut query_options = options.clone();
        // The scoped map is discovery output, not a parse-query input. Avoid
        // cloning its growing Arc on the next import insertion.
        query_options.scoped_imports = None;
        query_options.limits.max_tokens = parse_limits.max_tokens;
        query_options.limits.max_nodes = remaining.max_nodes;
        let parser_started = stats.as_ref().map(|_| Instant::now());
        let (parsed, reused, producer) = if options.reuse_producers() {
            let (source, reused) = session.producer_parse(&project.modules[index], &parse_limits);
            (source.parsed_shared(), reused, Some(source))
        } else {
            let (_, cached) = session.project_parse_input(
                &project.modules[index],
                &query_options,
                retained_syntax.as_deref(),
            )?;
            let reused = cached.is_some();
            let parsed = cached.unwrap_or_else(|| {
                Arc::new(zil_syntax::parse_with_limits(
                    &project.modules[index],
                    &parse_limits,
                ))
            });
            (parsed, reused, None)
        };
        if let Some(stats) = stats.as_deref_mut() {
            stats.parser_ns += parser_started.expect("profiled parse").elapsed().as_nanos();
            if reused {
                stats.parser_reuses += 1;
            } else {
                stats.parser_executions += 1;
                if options.reuse_producers() {
                    stats.source_clone_bytes += project.modules[index].text.len();
                }
            }
        }
        if !parsed.diagnostics.is_empty() {
            return Err(parsed.diagnostics.clone());
        }
        charge_frontend(&mut remaining, &parsed)?;
        for item in &parsed.module.items {
            let ast::ItemKind::Import(import) = &item.kind else {
                continue;
            };
            unsupported_import(item, import)?;
            if import.syntax && options.syntax_lock.is_none() {
                return Err(reject(
                    "E-SYNTAX-LOCK-MISSING",
                    Some(item.span),
                    "syntax provider use needs an explicit lock",
                ));
            }
            if import.path.is_empty() || import.path.iter().any(|s| !valid_segment(s)) {
                return Err(reject(
                    "E-MODULE-PATH",
                    Some(item.span),
                    "invalid logical import path",
                ));
            }
            if import.path.len() > options.limits.max_call_depth {
                return Err(limit(
                    Phase::Expand,
                    "project import path depth budget exceeded",
                ));
            }
            if import.path.first().is_some_and(|part| part == "std") {
                let compiler_module = matches!(import.path.as_slice(),
                    [a, b] if a == "std" && (b == "meta" || b == "syntax"));
                if import.syntax || (!compiler_module && stdlib::source(&import.path).is_none()) {
                    return Err(reject(
                        "E-STD-IMPORT",
                        Some(item.span),
                        "unknown or nonordinary standard library import",
                    ));
                }
                if compiler_module {
                    continue;
                }
            }
            // Resolve even an already registered path: the root is preseeded
            // in `paths`, but that must not hide a second directory entry.
            let embedded = stdlib::source(&import.path);
            let canonical = if embedded.is_some() {
                PathBuf::from(format!("<{}>", import.path.join(".")))
            } else if let Some(manifest) = &options.project {
                manifest.resolve(
                    Path::new(&project.modules[index].path),
                    &import.path,
                    item.span,
                )?
            } else {
                resolve_file_module(directory, &import.path, item.span)?
            };
            let id = if let Some(id) = known.get(&canonical) {
                *id
            } else {
                if project.modules.len() >= options.limits.max_nodes {
                    return Err(limit(Phase::Parse, "project module budget exceeded"));
                }
                let number = u32::try_from(project.modules.len())
                    .map_err(|_| limit(Phase::Parse, "project module identity budget exceeded"))?;
                let mut read_options = options.clone();
                read_options.limits.max_source_bytes = remaining.max_source_bytes;
                let mut source = (if let Some(text) = embedded {
                    if text.len() > read_options.limits.max_source_bytes {
                        Err(limit(
                            Phase::Parse,
                            "standard library source byte budget exceeded",
                        ))
                    } else {
                        // Embedded modules use the ordinary source graph and all normal kernels.
                        // Filesystem-only discovery receipts deliberately do not cache virtual paths.
                        stable_modules = None;
                        Ok(SourceUnit::new(
                            FileId(number),
                            ModuleId(number),
                            canonical.to_string_lossy().into_owned(),
                            text,
                        ))
                    }
                } else if let Some(source) = prefetched.remove(&canonical) {
                    options
                        .cancellation
                        .check(Phase::Driver)
                        .map_err(|error| vec![error])?;
                    if source.text.len() > read_options.limits.max_source_bytes {
                        Err(limit(Phase::Parse, "source byte budget exceeded"))
                    } else {
                        Ok(source)
                    }
                } else {
                    let read_started = stats.as_ref().map(|_| Instant::now());
                    let read = read_source(&canonical, &read_options);
                    if let Some(stats) = stats.as_deref_mut() {
                        stats.source_read_ns +=
                            read_started.expect("profiled read").elapsed().as_nanos();
                        if let Ok(source) = &read {
                            stats.read(source);
                        }
                    }
                    read
                })
                .map_err(|errors| {
                    errors
                        .into_iter()
                        .map(|e| {
                            e.with_note(format!("imported from {}", project.modules[index].path))
                        })
                        .collect::<Vec<_>>()
                })?;
                if let Some(manifest) = &options.project {
                    manifest.check_source(&source)?;
                }
                take(
                    &mut remaining.max_source_bytes,
                    source.text.len(),
                    Phase::Parse,
                    "project source byte budget exceeded",
                )?;
                source.module_id = ModuleId(number);
                source.file_id = FileId(number);
                if stable_modules
                    .as_mut()
                    .is_some_and(|keys| keys.push(&canonical).is_none())
                {
                    stable_modules = None;
                }
                known.insert(canonical, source.module_id);
                project.modules.push(source);
                ModuleId(number)
            };
            if let Some(manifest) = &options.project
                && import.path.first().is_some_and(|part| part != "std")
            {
                let importer = project.modules[index].module_id;
                let scoped = options
                    .scoped_imports
                    .get_or_insert_with(|| Arc::new(BTreeMap::new()));
                if let Some(previous) =
                    Arc::make_mut(scoped).insert((importer, import.path.clone()), id)
                    && previous != id
                {
                    return Err(reject(
                        "E-MODULE-IDENTITY",
                        Some(item.span),
                        "an import resolved to two distinct modules",
                    ));
                }
                let owner = manifest.owner(Path::new(&project.modules[index].path))?;
                let mut identity_path = vec![format!("__zil_pkg{owner}")];
                identity_path.extend(import.path.iter().cloned());
                if let Some(previous) = project.paths.insert(identity_path, id)
                    && previous != id
                {
                    return Err(reject(
                        "E-MODULE-IDENTITY",
                        Some(item.span),
                        "canonical package module identity conflicts",
                    ));
                }
            } else if let Some(previous) = project.paths.insert(import.path.clone(), id)
                && previous != id
            {
                return Err(reject(
                    "E-MODULE-IDENTITY",
                    Some(item.span),
                    format!(
                        "logical module {} resolves to two distinct files",
                        import.path.join(".")
                    ),
                ));
            }
        }
        let stable_module_key = stable_modules.as_ref().and_then(|keys| keys.get(index));
        syntax.push(LoadedSyntax {
            parsed,
            producer,
            reused,
            snapshot: None,
            stable_module_key,
        });
        index += 1;
    }
    Ok(LoadedProject::Parsed(syntax))
}

fn charge_ct_usage(
    remaining: &mut Limits,
    usage: zil_ct::SourceEvaluationUsage,
    functions: usize,
) -> CompileResult<()> {
    remaining.max_ct_steps = remaining
        .max_ct_steps
        .checked_sub(usage.steps)
        .ok_or_else(|| limit(Phase::Ct, "project CT step budget exceeded"))?;
    take(
        &mut remaining.max_nodes,
        usage.nodes,
        Phase::Ct,
        "project CT node budget exceeded",
    )?;
    take(
        &mut remaining.max_functions,
        functions,
        Phase::Ct,
        "project CT function budget exceeded",
    )
}

#[cfg(test)]
mod filesystem_discovery_tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Dir(std::path::PathBuf);
    impl Dir {
        fn new() -> Self {
            let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(format!(
                    "discovery158-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    fn discover(
        session: &CompilationSession,
        path: &Path,
        options: &CompileOptions,
    ) -> CompileResult<Vec<Arc<ParseResult>>> {
        discover_loaded(session, path, options)
            .map(|syntax| syntax.into_iter().map(|syntax| syntax.parsed).collect())
    }
    fn discover_loaded(
        session: &CompilationSession,
        path: &Path,
        options: &CompileOptions,
    ) -> CompileResult<Vec<LoadedSyntax>> {
        let mut p = ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "placeholder", "")],
            paths: BTreeMap::new(),
        };
        match load_sources(&mut p, path, &mut options.clone(), session)? {
            LoadedProject::Parsed(syntax) => Ok(syntax),
            LoadedProject::Prepared(_, _)
            | LoadedProject::Semantic(_)
            | LoadedProject::Resident => {
                panic!("this control expects ordinary syntax discovery")
            }
        }
    }
    fn discover_candidate(
        path: &Path,
        options: &CompileOptions,
    ) -> CompileResult<(ProjectSources, LoadedProject)> {
        let mut project = ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "placeholder", "")],
            paths: BTreeMap::new(),
        };
        let loaded = load_sources(
            &mut project,
            path,
            &mut options.clone(),
            &CompilationSession::default(),
        )?;
        Ok((project, loaded))
    }

    #[test]
    fn import_free_discovery_reuses_decoded_pair_and_preserves_corrupt_and_limit_fallback() {
        let dir = Dir::new();
        let path = dir.0.join("main.zil");
        fs::write(&path, "pub fn main()->i32=>0").unwrap();
        let options = CompileOptions {
            semantic_cache: Some(dir.0.join("cache")),
            ..Default::default()
        };
        let cold = CompilationSession::default().compile_path(&path, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        let (project, loaded) = discover_candidate(&path, &options).unwrap();
        assert!(matches!(&loaded, LoadedProject::Semantic(_)));
        let key = project_key(
            &project.modules[0],
            &options,
            project_fingerprint(&project),
            Phase::Types,
            true,
        );
        let store = super::super::semantic_cache::Store::new(&options, &key).unwrap();
        // Once decoded, the same immutable pair reaches the normal pipeline;
        // a second disk decode here would miss this deliberately corrupted file.
        fs::write(store.path(), "corrupt after discovery").unwrap();
        let warm = CompilationSession::default().compile_project_request(
            &project,
            &options,
            &mut |_| {},
            Some(loaded),
        );
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.disk_cache, DiskCacheStatus::Hit);
        assert_eq!(warm.verification_runs, 1);
        assert!(
            !warm
                .query_events
                .iter()
                .any(|event| event.phase == Phase::Parse)
        );
        assert!(
            warm.query_events
                .iter()
                .any(|event| event.phase == Phase::Analyze
                    && event.disposition == QueryDisposition::Reused)
        );
        let (_, rebuilt) = discover_candidate(&path, &options).unwrap();
        assert!(matches!(rebuilt, LoadedProject::Parsed(_)));
        let repaired = CompilationSession::default().compile_path(&path, &options);
        assert_eq!(
            repaired.status,
            ResultCategory::Success,
            "{}",
            repaired.to_json()
        );
        assert_eq!(repaired.disk_cache, DiskCacheStatus::Rebuilt);
        assert!(
            repaired
                .query_events
                .iter()
                .any(|event| event.phase == Phase::Parse
                    && event.disposition == QueryDisposition::Executed)
        );
        let mut limited = options.clone();
        limited.limits.max_tokens = 1;
        let with_cache = CompilationSession::default().compile_path(&path, &limited);
        limited.semantic_cache = None;
        let without_cache = CompilationSession::default().compile_path(&path, &limited);
        assert_eq!(with_cache.status, without_cache.status);
        assert_eq!(with_cache.diagnostics, without_cache.diagnostics);
        assert_ne!(with_cache.status, ResultCategory::Success);
        fs::remove_dir_all(dir.0).unwrap();
    }

    #[test]
    fn rejected_discovery_pair_reparses_the_same_current_root_bytes() {
        let dir = Dir::new();
        let path = dir.0.join("main.zil");
        fs::write(&path, "pub fn main()->i32=>0").unwrap();
        let options = CompileOptions {
            semantic_cache: Some(dir.0.join("cache")),
            ..Default::default()
        };
        let cold = CompilationSession::default().compile_path(&path, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        let (project, mut loaded) = discover_candidate(&path, &options).unwrap();
        let LoadedProject::Semantic(pair) = &mut loaded else {
            panic!("exact successful import-free source should skip discovery parsing");
        };
        assert!(pair.0.module.functions[0].locals.is_empty());
        pair.1.functions[0].blocks[0]
            .initialized_out
            .push(zil_ir::LocalId(0));
        // Discovery already captured immutable input bytes. A rejected raw
        // claim must not reread a subsequently changed file during its retry.
        fs::write(&path, "fn malformed(").unwrap();
        let report = CompilationSession::default().compile_project_request(
            &project,
            &options,
            &mut |_| {},
            Some(loaded),
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.disk_cache, DiskCacheStatus::Rebuilt);
        assert_eq!(report.verification_runs, 2);
        assert!(
            report
                .query_events
                .iter()
                .any(|event| event.phase == Phase::Parse
                    && event.disposition == QueryDisposition::Executed)
        );
        assert_eq!(
            report
                .query_events
                .iter()
                .filter(|event| event.phase == Phase::Verify)
                .count(),
            2
        );
        fs::remove_dir_all(dir.0).unwrap();
    }

    #[test]
    fn discovery_probe_does_not_hide_added_imports_or_ambiguous_self_entries() {
        let dir = Dir::new();
        let path = dir.0.join("main.zil");
        let options = CompileOptions {
            semantic_cache: Some(dir.0.join("cache")),
            ..Default::default()
        };
        fs::write(&path, "pub fn main()->i32=>0").unwrap();
        let first = CompilationSession::default().compile_path(&path, &options);
        assert_eq!(first.status, ResultCategory::Success, "{}", first.to_json());
        fs::write(&path, "import missing; fn main()->i32=>0").unwrap();
        let missing = CompilationSession::default().compile_path(&path, &options);
        assert!(
            missing
                .diagnostics
                .iter()
                .any(|error| error.code == "E-MODULE-NOT-FOUND")
        );
        fs::write(&path, "import main; fn main()->i32=>0").unwrap();
        fs::create_dir(dir.0.join("main")).unwrap();
        fs::write(dir.0.join("main/mod.zil"), "fn main()->i32=>0").unwrap();
        let ambiguous = CompilationSession::default().compile_path(&path, &options);
        assert!(
            ambiguous
                .diagnostics
                .iter()
                .any(|error| error.code == "E-MODULE-AMBIGUOUS")
        );
        for text in [
            "// import
pub fn main()->i32=>0",
            "// syntax
pub fn main()->i32=>0",
        ] {
            fs::write(&path, text).unwrap();
            let first = CompilationSession::default().compile_path(&path, &options);
            assert_eq!(first.status, ResultCategory::Success, "{}", first.to_json());
            assert!(matches!(
                discover_candidate(&path, &options).unwrap().1,
                LoadedProject::Parsed(_)
            ));
        }
        fs::remove_dir_all(dir.0).unwrap();
    }

    fn setup() -> (Dir, std::path::PathBuf, CompilationSession) {
        let dir = Dir::new();
        let main = dir.0.join("main.zil");
        fs::write(&main, "import leaf; fn main()->i32=>leaf.answer()-42").unwrap();
        fs::write(dir.0.join("leaf.zil"), "pub fn answer()->i32=>42").unwrap();
        let mut session = CompilationSession::with_capacity(1024);
        let r = session.compile_path(&main, &CompileOptions::default());
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        (dir, main, session)
    }
    #[test]
    fn discovery_reuses_exact_published_syntax_arcs_but_reads_edited_bytes() {
        let (dir, path, session) = setup();
        let options = CompileOptions::default();
        let syntax = discover(&session, &path, &options).unwrap();
        assert_eq!(syntax.len(), 2);
        for p in &syntax {
            assert!(
                session
                    .raw
                    .values()
                    .any(|v| matches!(v,RawSnapshot::Parse(q) if Arc::ptr_eq(p,q)))
            );
        }
        fs::write(dir.0.join("leaf.zil"), "pub fn answer()->i32=>43").unwrap();
        let changed = discover(&session, &path, &options).unwrap();
        assert!(Arc::ptr_eq(&syntax[0], &changed[0]));
        assert!(!Arc::ptr_eq(&syntax[1], &changed[1]));
    }
    #[test]
    fn evicted_syntax_is_really_shared_without_retaining_old_semantic_owners() {
        let dir = Dir::new();
        let path = dir.0.join("main.zil");
        let mut main = String::new();
        for i in 1..40 {
            main += &format!("import m{i}\n");
            fs::write(
                dir.0.join(format!("m{i}.zil")),
                format!("pub fn answer()->i32{{return {i};}}"),
            )
            .unwrap();
        }
        main += "fn main()->i32{return m1.answer();}";
        fs::write(&path, main).unwrap();
        let options = CompileOptions::default();
        let mut session = CompilationSession::with_capacity(8);
        let cold = session.compile_path(&path, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        assert!(session.cached_query_count() <= 8);
        let old_project = session
            .raw
            .values()
            .find_map(|raw| match raw {
                RawSnapshot::Project(project) => Some(Arc::downgrade(project)),
                _ => None,
            })
            .unwrap();
        let before = discover_loaded(&session, &path, &options).unwrap();
        assert_eq!(before.len(), 40);
        assert!(before.iter().all(|syntax| syntax.reused));
        // The first modules have no individual raw owners after capacity trim.
        assert!(!session.raw.values().any(
            |raw| matches!(raw, RawSnapshot::Parse(parsed) if Arc::ptr_eq(parsed, &before[1].parsed))
        ));
        let changed_old = Arc::downgrade(&before[1].parsed);
        fs::write(dir.0.join("m1.zil"), "pub fn answer()->i32{return 123;}").unwrap();
        let after = discover_loaded(&session, &path, &options).unwrap();
        for (index, (old, new)) in before.iter().zip(&after).enumerate() {
            assert_eq!(new.reused, index != 1);
            assert_eq!(Arc::ptr_eq(&old.parsed, &new.parsed), index != 1);
        }
        drop(before);
        drop(after);
        // Discovery itself did not mutate or replace a published project.
        assert!(old_project.upgrade().is_some());
        let edited = session.compile_path(&path, &options);
        assert_eq!(
            edited.status,
            ResultCategory::Success,
            "{}",
            edited.to_json()
        );
        assert_eq!(edited.verification_runs, 1);
        assert_eq!(
            edited
                .query_events
                .iter()
                .filter(|event| event.phase == Phase::Parse
                    && event.disposition == QueryDisposition::Executed)
                .count(),
            1
        );
        assert!(old_project.upgrade().is_none());
        assert!(changed_old.upgrade().is_none());
        assert!(session.cached_query_count() <= 8);
        assert_eq!(session.active_query_count(), 0);
        let retained = discover(&session, &path, &options).unwrap();
        let current = Arc::downgrade(&retained[0]);
        drop(retained);
        session.clear();
        assert!(current.upgrade().is_none());
    }
    #[test]
    fn cached_syntax_cannot_hide_lower_quotas_syntax_errors_or_ambiguous_entries() {
        let (dir, path, session) = setup();
        let mut low = CompileOptions::default();
        low.limits.max_tokens = 3;
        assert!(discover(&session, &path, &low).is_err());
        fs::write(dir.0.join("leaf.zil"), "pub fn answer( ->").unwrap();
        assert!(discover(&session, &path, &CompileOptions::default()).is_err());
        fs::write(dir.0.join("leaf.zil"), "pub fn answer()->i32=>42").unwrap();
        fs::create_dir(dir.0.join("leaf")).unwrap();
        fs::write(dir.0.join("leaf/mod.zil"), "pub fn answer()->i32=>42").unwrap();
        let errors = discover(&session, &path, &CompileOptions::default()).unwrap_err();
        assert!(errors.iter().any(|e| e.code == "E-MODULE-AMBIGUOUS"));
    }
    #[test]
    fn capacity_eviction_keeps_lent_syntax_valid_without_caching_authority() {
        let (_dir, path, mut session) = setup();
        session.capacity = 1;
        assert!(session.raw.len() > session.capacity);
        let before = discover(&session, &path, &CompileOptions::default()).unwrap();
        let r = session.compile_path(&path, &CompileOptions::default());
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        assert!(r.cache_hit); // Raw inputs may be reused; Verify still ran.
        assert_eq!(session.cached_query_count(), 1);
        let after = discover(&session, &path, &CompileOptions::default()).unwrap();
        for (a, b) in before.iter().zip(after.iter()) {
            // Evicted raw syntax remains valid for existing immutable readers;
            // discovery reconstructs equivalent syntax after eviction.
            assert!(!Arc::ptr_eq(a, b));
            assert_eq!(format!("{a:?}"), format!("{b:?}"));
        }
        assert_eq!(session.active_query_count(), 0);
    }
    #[test]
    fn discovery_cancellation_and_active_query_guards_still_apply() {
        let (_dir, path, mut session) = setup();
        let options = CompileOptions::default();
        options.cancellation.cancel();
        assert!(discover(&session, &path, &options).is_err());
        let key = session
            .raw
            .iter()
            .find_map(|(k, v)| matches!(v, RawSnapshot::Parse(_)).then(|| k.clone()))
            .unwrap();
        session.database.begin(key.clone()).unwrap();
        assert!(discover(&session, &path, &CompileOptions::default()).is_err());
        session.database.abort(&key);
        assert!(discover(&session, &path, &CompileOptions::default()).is_ok());
    }
    #[test]
    fn resident_discovery_reads_current_imports_without_decoding_old_semantic_files() {
        let dir = Dir::new();
        let path = dir.0.join("main.zil");
        fs::write(&path, "import dep; pub fn main()->i32=>dep.value(3)").unwrap();
        fs::write(
            dir.0.join("dep.zil"),
            "pub fn value(x:i32)->i32{if x<10{return x+1;}else{return x*2;}}",
        )
        .unwrap();
        let options = CompileOptions {
            producer_reuse: true,
            profile_compilation: true,
            semantic_cache: Some(dir.0.join("cache")),
            ..Default::default()
        };
        let mut session = CompilationSession::default();
        let cold = session.compile_path(&path, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        let root = fs::read_dir(options.semantic_cache.as_ref().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("semantic-v")
            })
            .unwrap();
        // A complete resident candidate must remain usable even when optional
        // disk candidates are corrupt or their content-addressed items vanish.
        for entry in fs::read_dir(&root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                fs::write(path, b"corrupt optional manifest").unwrap();
            }
        }
        fs::remove_dir_all(root.join("packs-v1")).unwrap();
        let mut project = ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "placeholder", "")],
            paths: BTreeMap::new(),
        };
        let loaded = load_sources(&mut project, &path, &mut options.clone(), &session).unwrap();
        assert!(matches!(loaded, LoadedProject::Resident));
        assert_eq!(project.modules.len(), 2);
        let warm = session.compile_path(&path, &options);
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.verification_runs, 1);
        let ingress = warm.source_ingress.as_ref().unwrap();
        assert_eq!(ingress.source_files_read, 2);
        assert_eq!(ingress.parser_executions, 0);
        assert_eq!(ingress.semantic_load_attempts, 0);
        assert_eq!(ingress.semantic_restored_functions, 0);
        assert_eq!(ingress.resident_routes, 1);
        assert_eq!(ingress.resident_frontend_hits, 1);
        assert!(ingress.source_clone_bytes < ingress.source_bytes_read);
        // This real graph has two Parse leaves, one Expand, one Ct and one
        // Types owner. The Types traversal closes all five once; the explicit
        // Ct and Expand reads then reuse that same immutable proof set.
        let reads = warm.frontend_dependency_reads.as_ref().unwrap();
        assert_eq!(reads.validated_snapshots, 5);
        assert_eq!(reads.dependency_checks, 4);
        assert_eq!(reads.reused_validations, 2);
        assert_eq!(reads.retained_validations, 5);

        assert!(
            warm.query_events
                .iter()
                .filter(|e| e.phase == Phase::Parse)
                .all(|e| e.disposition == QueryDisposition::Reused)
        );
        assert!(
            !warm
                .query_events
                .iter()
                .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed)
        );

        // Added or ambiguous imports are still resolved from the current disk
        // graph before any resident frontend can be selected.
        fs::create_dir_all(dir.0.join("dep")).unwrap();
        fs::write(dir.0.join("dep/mod.zil"), "pub fn value(x:i32)->i32=>x").unwrap();
        let ambiguous = session.compile_path(&path, &options);
        assert_eq!(ambiguous.diagnostics[0].code, "E-MODULE-AMBIGUOUS");
        fs::remove_file(dir.0.join("dep/mod.zil")).unwrap();

        session.database.clear();
        project = ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "placeholder", "")],
            paths: BTreeMap::new(),
        };
        let loaded = load_sources(&mut project, &path, &mut options.clone(), &session).unwrap();
        assert!(matches!(loaded, LoadedProject::Parsed(_)));
        let repaired = session.compile_path(&path, &options);
        assert_eq!(
            repaired.status,
            ResultCategory::Success,
            "{}",
            repaired.to_json()
        );
        assert_eq!(repaired.verification_runs, 1);
        assert!(
            repaired
                .query_events
                .iter()
                .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Executed)
        );
        session.clear();
        let cleared = session.compile_path(&path, &options);
        assert_eq!(
            cleared.status,
            ResultCategory::Success,
            "{}",
            cleared.to_json()
        );
        assert_eq!(cleared.verification_runs, 1);
        fs::remove_dir_all(dir.0).unwrap();
    }

    #[test]
    fn resident_discovery_does_not_cross_limits_or_target_bindings() {
        let dir = Dir::new();
        let path = dir.0.join("main.zil");
        fs::write(&path, "pub fn main()->i32=>7").unwrap();
        let options = CompileOptions {
            producer_reuse: true,
            semantic_cache: Some(dir.0.join("cache")),
            ..Default::default()
        };
        let mut session = CompilationSession::default();
        assert_eq!(
            session.compile_path(&path, &options).status,
            ResultCategory::Success
        );
        for limited in [true, false] {
            let mut current = options.clone();
            if limited {
                current.limits.max_tokens = 1;
            } else {
                current.target.rule_version.push_str("/different");
            }
            let mut project = ProjectSources {
                root: ModuleId(0),
                modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "placeholder", "")],
                paths: BTreeMap::new(),
            };
            let loaded = load_sources(&mut project, &path, &mut current.clone(), &session);
            assert!(!matches!(loaded, Ok(LoadedProject::Resident)));
            let actual = session.compile_path(&path, &current);
            let expected = CompilationSession::default().compile_path(&path, &current);
            assert_eq!(actual.status, expected.status);
            assert_eq!(actual.diagnostics, expected.diagnostics);
        }
        fs::remove_dir_all(dir.0).unwrap();
    }
    // Keep the actual profile workload size and generated arithmetic. Small
    // import-free tests cannot exercise parsed-discovery parent returns or the
    // failed Types rollback after a broad/position epoch.
    fn full_epoch_fixture(label: &str) -> BTreeMap<String, String> {
        let mut files = BTreeMap::from([(
            "io.zil".into(),
            include_str!("../../../examples/mainline/resource_closures/io.zil").into(),
        )]);
        for i in 0..20 {
            let mut code = String::new();
            for j in 0..20 {
                let delta = match label {
                    "private" => usize::from(i == 0 && j == 0),
                    "multi" => usize::from([0, 10, 19].contains(&i) && j == 0),
                    "broad" => 1,
                    "recovery" => 2 * usize::from(i == 0 && j == 0),
                    _ => 0,
                };
                code.push_str(&format!("fn h{j}(x: i32) -> i32 {{\n    var y: i32 = x\n"));
                for k in 0..16 {
                    let a = (i + j + k) % 13 + 1 + delta;
                    let b = (j * 3 + k) % 11 + 2;
                    code.push_str(&format!("    if y % 3 == 0 {{ y = (y * 2 + {a}) % 251 }} else {{ y = (y + {b}) % 251 }}\n"));
                }
                code.push_str(if label == "error" && i == 0 && j == 0 {
                    "    return absent_name\n}\n"
                } else {
                    "    return y\n}\n"
                });
            }
            code.push_str("pub fn value(x: i32) -> i32 {\n    var total: i32 = x\n");
            for j in 0..20 {
                code.push_str(&format!("    total = (total + h{j}(x)) % 65521\n"));
            }
            code.push_str("    return total\n}\n");
            files.insert(format!("m{i}.zil"), code);
        }
        let mut main = String::from("import io\n");
        for i in 0..20 {
            main.push_str(&format!("import m{i}\n"));
        }
        main.push_str("fn main() -> i32 {\n    var x: i32 = io.read()\n    if x < 0 { x = 0 }\n    x = x % 251\n    var total: i32 = 0\n");
        for i in 0..20 {
            main.push_str(&format!("    total = (total + m{i}.value(x)) % 65521\n"));
        }
        for shift in [0, 8, 16, 24] {
            main.push_str(&format!("    io.emit((total >> {shift}) & 255)\n"));
        }
        main.push_str("    return 0\n}\n");
        files.insert("main.zil".into(), main);
        if label == "positions" {
            for text in files.values_mut() {
                *text = format!("// Source position edit; semantic body is unchanged.\n\n{text}");
            }
        }
        files
    }

    #[test]
    fn full_profile_history_reactivates_parsed_parents_and_rolls_back_sealed_memos() {
        let dir = Dir::new();
        let path = dir.0.join("main.zil");
        let cold = full_epoch_fixture("cold");
        assert_eq!(cold.len(), 22);
        assert_eq!(cold.values().map(String::len).sum::<usize>(), 498_620);
        let options = CompileOptions {
            producer_reuse: true,
            profile_compilation: true,
            semantic_cache: Some(dir.0.join("cache")),
            ..Default::default()
        };
        // This is the measured resident protocol's actual capacity, not a
        // larger test-only history or a reduced source/CFG fixture.
        let mut session = CompilationSession::with_capacity(256);
        let mut errors = Vec::new();
        for (index, label) in [
            "cold",
            "private",
            "cold",
            "multi",
            "cold",
            "broad",
            "cold",
            "positions",
            "cold",
            "error",
            "error",
            "recovery",
        ]
        .into_iter()
        .enumerate()
        {
            let files = full_epoch_fixture(label);
            for (name, text) in &files {
                fs::write(dir.0.join(name), text).unwrap();
            }
            let report = session.compile_path(&path, &options);
            if label == "error" {
                assert_eq!(
                    report.status,
                    ResultCategory::SafeReject,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.phase, Phase::Types);
                assert_eq!(report.verification_runs, 0);
                let work = report.function_type_work.as_ref().unwrap();
                assert_eq!((work.body_key_executions, work.body_key_reuses), (1, 420));
                let producer = report.producer_work.as_ref();
                if index == 9 {
                    assert_eq!(
                        (
                            producer.unwrap().lower_modules,
                            producer.unwrap().reused_lower_modules
                        ),
                        (1, 21)
                    );
                }
                errors.push(report.diagnostics.clone());
                continue;
            }
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{label}: {}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            if label == "cold" && index != 0 {
                assert_eq!(
                    report.disk_cache,
                    DiskCacheStatus::Resident,
                    "parent at {index}: {}",
                    report.to_json()
                );
                assert!(report.function_type_work.is_none());
                assert!(
                    report
                        .driver_timings_ns
                        .contains_key("producer_epoch_activation")
                );
                assert!(!report.driver_timings_ns.contains_key("semantic_cache_read"));
                if index == 6 {
                    // All twenty broad modules really had to be rediscovered,
                    // yet their exact retained parent still owns raw Types.
                    assert_eq!(
                        report.source_ingress.as_ref().unwrap().parser_executions,
                        20
                    );
                }
            }
            if label == "private" {
                let work = report.function_type_work.as_ref().unwrap();
                assert_eq!((work.body_key_executions, work.body_key_reuses), (1, 420));
                assert_eq!((work.executed_functions, work.reused_functions), (5, 420));
                assert_eq!(work.cfg_relocated_functions, 20);
                let producer = report.producer_work.as_ref().unwrap();
                assert_eq!(
                    (
                        producer.function_input_hashes,
                        producer.function_input_reuses
                    ),
                    (1, 424)
                );
                assert_eq!(producer.function_input_text_comparisons, 21);
                assert_eq!(producer.function_input_binding_hashes, 21);
                let proof = report.verification_work.as_ref().unwrap();
                assert_eq!(
                    (proof.function_proofs_checked, proof.function_proofs_reused),
                    (1, 422)
                );
                let legacy_options = CompileOptions {
                    producer_reuse: false,
                    semantic_cache: None,
                    ..options.clone()
                };
                let mut fresh = CompilationSession::default();
                let expected = fresh.compile_path(&path, &legacy_options);
                assert_eq!(
                    expected.status,
                    ResultCategory::Success,
                    "{}",
                    expected.to_json()
                );
                assert_eq!(report.diagnostics, expected.diagnostics);
                assert_eq!(report.source_files, expected.source_files);
                let program = |session: &CompilationSession| {
                    session
                        .raw
                        .values()
                        .find_map(|raw| {
                            if let RawSnapshot::ProjectTyped(owner) = raw
                                && owner.module.source_fingerprint == report.input_fingerprint
                            {
                                Some(owner.program().clone())
                            } else {
                                None
                            }
                        })
                        .unwrap()
                };
                // All raw declarations, instruction/diagnostic spans and CFGs
                // equal the actual fresh full pipeline, not just graph counters.
                assert_eq!(program(&session), program(&fresh));
            }
            if label == "positions" {
                let producer = report.producer_work.as_ref().unwrap();
                assert_eq!(producer.environment_imports_changed, 1);
                assert_eq!(
                    (
                        producer.function_input_hashes,
                        producer.function_input_reuses
                    ),
                    (0, 425)
                );
                assert_eq!(producer.function_input_text_comparisons, 425);
                assert_eq!(producer.function_input_binding_hashes, 425);
                assert_eq!(
                    (producer.lower_modules, producer.reused_lower_modules),
                    (22, 0)
                );
                assert_eq!(producer.retained_normalized_items, 0);
                assert_eq!(producer.retained_restored_modules, 0);
                let materializer = report.materialization_work.as_ref().unwrap();
                assert_eq!(
                    (materializer.executed_modules, materializer.reused_modules),
                    (22, 0)
                );
                let legacy_options = CompileOptions {
                    producer_reuse: false,
                    semantic_cache: None,
                    ..options.clone()
                };
                let mut fresh = CompilationSession::default();
                let expected = fresh.compile_path(&path, &legacy_options);
                assert_eq!(
                    expected.status,
                    ResultCategory::Success,
                    "{}",
                    expected.to_json()
                );
                assert_eq!(report.diagnostics, expected.diagnostics);
                assert_eq!(report.source_files, expected.source_files);
                let program = |session: &CompilationSession| {
                    session
                        .raw
                        .values()
                        .find_map(|raw| {
                            if let RawSnapshot::ProjectTyped(owner) = raw
                                && owner.module.source_fingerprint == report.input_fingerprint
                            {
                                Some(owner.program().clone())
                            } else {
                                None
                            }
                        })
                        .unwrap()
                };
                // Current locations, declarations and the full CFG equal a
                // real fresh pipeline despite preserved relative input tokens.
                assert_eq!(program(&session), program(&fresh));
                let work = report.function_type_work.as_ref().unwrap();
                assert_eq!((work.body_key_executions, work.body_key_reuses), (0, 421));
                assert_eq!((work.executed_functions, work.reused_functions), (4, 421));
                let publication = report.semantic_cache_publication.as_ref().unwrap();
                assert_eq!(
                    (
                        publication.serialized_functions,
                        publication.reused_functions
                    ),
                    (4, 421)
                );
            }
            if label == "recovery" {
                let work = report.function_type_work.as_ref().unwrap();
                assert_eq!((work.body_key_executions, work.body_key_reuses), (1, 420));
                assert_eq!((work.executed_functions, work.reused_functions), (5, 420));
                let publication = report.semantic_cache_publication.as_ref().unwrap();
                assert_eq!(
                    (
                        publication.serialized_functions,
                        publication.reused_functions
                    ),
                    (1, 424)
                );
            }
        }
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0], errors[1]);
        for (name, text) in full_epoch_fixture("error") {
            fs::write(dir.0.join(name), text).unwrap();
        }
        let fresh = CompilationSession::default().compile_path(&path, &options);
        assert_eq!(errors[0], fresh.diagnostics);
        assert_eq!(fresh.phase, Phase::Types);
        assert_eq!(fresh.verification_runs, 0);
        fs::remove_dir_all(dir.0).unwrap();
    }

    #[test]
    fn resident_parent_restores_types_and_publication_before_the_next_private_edit() {
        let dir = Dir::new();
        let path = dir.0.join("main.zil");
        let cold_text = "fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}} fn g(x:i32)->i32{if x<10{return x+3;}else{return x*4;}} pub fn main()->i32{let a=f(2);let b=g(a);let c=f(b);return c;}";
        let options = CompileOptions {
            producer_reuse: true,
            profile_compilation: true,
            semantic_cache: Some(dir.0.join("cache")),
            ..Default::default()
        };
        let mut session = CompilationSession::with_capacity(64);
        fs::write(&path, cold_text).unwrap();
        let cold = session.compile_path(&path, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        let (cold_key, epoch) = session
            .raw
            .iter()
            .find_map(|(key, raw)| {
                if let RawSnapshot::ProjectTyped(owner) = raw
                    && owner.module.source_fingerprint == cold.input_fingerprint
                {
                    Some((key.clone(), owner.epoch_weak()))
                } else {
                    None
                }
            })
            .unwrap();
        let epoch = epoch.expect("successful Types producer retains a private raw epoch");
        let broad_text = cold_text.replace("x+1", "x+11").replace("x+3", "x+13");
        fs::write(&path, broad_text).unwrap();
        let broad = session.compile_path(&path, &options);
        assert_eq!(broad.status, ResultCategory::Success, "{}", broad.to_json());
        fs::write(&path, cold_text).unwrap();
        let parent = session.compile_path(&path, &options);
        assert_eq!(
            parent.status,
            ResultCategory::Success,
            "{}",
            parent.to_json()
        );
        assert_eq!(parent.disk_cache, DiskCacheStatus::Resident);
        assert!(parent.function_type_work.is_none());
        assert_eq!(parent.verification_runs, 1);
        // A new position-only revision must reuse cold canonical fragments,
        // rather than serialize bodies from the unrelated broad epoch.
        let shifted = format!("// current positions\n{cold_text}");
        fs::write(&path, &shifted).unwrap();
        let positions = session.compile_path(&path, &options);
        assert_eq!(
            positions.status,
            ResultCategory::Success,
            "{}",
            positions.to_json()
        );
        let work = positions.function_type_work.as_ref().unwrap();
        assert_eq!((work.executed_functions, work.reused_functions), (0, 3));
        let publication = positions.semantic_cache_publication.as_ref().unwrap();
        assert_eq!(
            (
                publication.serialized_functions,
                publication.reused_functions
            ),
            (0, 3)
        );
        assert_eq!(positions.verification_runs, 1);
        // Return to cold again, then change only one private function. The
        // unchanged bodies retain their original producer revisions and CFGs.
        fs::write(&path, cold_text).unwrap();
        let parent = session.compile_path(&path, &options);
        assert_eq!(
            parent.status,
            ResultCategory::Success,
            "{}",
            parent.to_json()
        );
        let private = cold_text.replace("x+1", "x+7");
        fs::write(&path, &private).unwrap();
        let edited = session.compile_path(&path, &options);
        assert_eq!(
            edited.status,
            ResultCategory::Success,
            "{}",
            edited.to_json()
        );
        let work = edited.function_type_work.as_ref().unwrap();
        assert_eq!((work.body_key_executions, work.body_key_reuses), (1, 2));
        assert_eq!((work.executed_functions, work.reused_functions), (1, 2));
        let publication = edited.semantic_cache_publication.as_ref().unwrap();
        assert_eq!(
            (
                publication.serialized_functions,
                publication.reused_functions
            ),
            (1, 2)
        );
        assert_eq!(edited.verification_runs, 1);
        let fresh = CompilationSession::default().compile_path(&path, &options);
        assert_eq!(edited.status, fresh.status);
        assert_eq!(edited.diagnostics, fresh.diagnostics);
        session.clear();
        assert!(!session.raw.contains_key(&cold_key));
        assert!(
            epoch.upgrade().is_none(),
            "epoch lives only as long as its raw owner"
        );
        fs::remove_dir_all(dir.0).unwrap();
    }

    #[test]
    fn returning_to_resident_parent_reactivates_its_sealed_source_producer_epoch() {
        let dir = Dir::new();
        let path = dir.0.join("main.zil");
        let texts = [
            (
                "main.zil",
                "import dep; import channel; pub fn main()->i32{let a=dep.f(3);let b=channel.emit(a);let c=dep.f(b);return c;}",
            ),
            (
                "dep.zil",
                "pub fn f(x:i32)->i32{if x<10{return x+1;}else{return x*2;}}",
            ),
            (
                "channel.zil",
                "pub fn emit(x:i32)->i32{if x<0{return 0;}else{return x;}}",
            ),
        ];
        for (file, text) in texts {
            fs::write(dir.0.join(file), text).unwrap();
        }
        let options = CompileOptions {
            producer_reuse: true,
            profile_compilation: true,
            semantic_cache: Some(dir.0.join("cache")),
            ..Default::default()
        };
        let mut session = CompilationSession::with_capacity(64);
        let cold = session.compile_path(&path, &options);
        assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
        let (cold_expand, epoch) = session
            .raw
            .iter()
            .find_map(|(key, raw)| match raw {
                RawSnapshot::Project(expanded)
                    if expanded.core.source_fingerprint == cold.input_fingerprint =>
                {
                    expanded
                        .producer_epoch
                        .as_ref()
                        .map(|epoch| (key.clone(), Arc::downgrade(epoch)))
                }
                _ => None,
            })
            .expect("the real scalar Expand owner retains its own sealed epoch");
        for (file, text) in texts {
            fs::write(
                dir.0.join(file),
                format!("// shifted source positions\n\n{text}"),
            )
            .unwrap();
        }
        let positions = session.compile_path(&path, &options);
        assert_eq!(
            positions.status,
            ResultCategory::Success,
            "{}",
            positions.to_json()
        );
        for (file, text) in texts {
            fs::write(dir.0.join(file), text).unwrap();
        }
        let parent = session.compile_path(&path, &options);
        assert_eq!(
            parent.status,
            ResultCategory::Success,
            "{}",
            parent.to_json()
        );
        assert_eq!(parent.disk_cache, DiskCacheStatus::Resident);
        assert_eq!(parent.verification_runs, 1);
        assert_eq!(parent.source_ingress.as_ref().unwrap().parser_executions, 0);
        assert!(parent.producer_work.is_none());
        let broken = texts[1].1.replace("return x+1", "return absent_name+1");
        fs::write(dir.0.join("dep.zil"), &broken).unwrap();
        let error = session.compile_path(&path, &options);
        let fresh = CompilationSession::default().compile_path(&path, &options);
        assert_eq!(error.status, fresh.status);
        assert_eq!(error.phase, fresh.phase);
        assert_eq!(error.diagnostics, fresh.diagnostics);
        assert_eq!(error.diagnostics[0].code, "E-UNKNOWN-NAME");
        let at = broken.find("absent_name").unwrap();
        assert_eq!(
            error.diagnostics[0].span,
            Some(Span::new(FileId(1), at, at + "absent_name".len()))
        );
        assert_eq!(error.verification_runs, 0);
        let ingress = error.source_ingress.as_ref().unwrap();
        assert_eq!((ingress.parser_executions, ingress.parser_reuses), (1, 2));
        let producer = error.producer_work.as_ref().unwrap();
        assert_eq!(producer.classified_modules, 1);
        assert_eq!(
            (producer.lower_modules, producer.reused_lower_modules),
            (1, 2)
        );
        assert_eq!(producer.environment_imports_changed, 0);
        let materializer = error
            .materialization_work
            .as_ref()
            .expect("emit routing exercises the actual scalar materializer");
        assert_eq!(
            (materializer.executed_modules, materializer.reused_modules),
            (1, 2)
        );

        // The enclosing epoch object has no independent history owner. Existing
        // raw-owner removal releases it, including the public clear boundary.
        // No private recency bookkeeping or independent epoch history is needed.
        session.clear();
        assert!(!session.raw.contains_key(&cold_expand));
        assert!(epoch.upgrade().is_none());
        fs::remove_dir_all(dir.0).unwrap();
    }
}

#[cfg(test)]
mod attempt_accounting_tests {
    use super::*;

    #[test]
    fn rejected_semantic_candidate_keeps_verify_cost_and_diagnostic_provenance() {
        let directory =
            std::env::temp_dir().join(format!("zil-semantic-attempt-cost-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("main.zil");
        let text = "pub fn main()->i32=>0";
        fs::write(&path, text).unwrap();
        let project = ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(
                FileId(0),
                ModuleId(0),
                path.to_string_lossy(),
                text,
            )],
            paths: BTreeMap::new(),
        };
        let options = CompileOptions {
            semantic_cache: Some(directory.join("cache")),
            ..Default::default()
        };
        let mut producer = CompilationSession::default();
        let original = producer.compile_project(&project, &options);
        assert_eq!(
            original.status,
            ResultCategory::Success,
            "{}",
            original.to_json()
        );
        let mut typed = producer
            .raw
            .values()
            .find_map(|raw| match raw {
                RawSnapshot::ProjectTyped(typed) => Some((**typed).clone()),
                _ => None,
            })
            .unwrap();
        let mut candidate = producer
            .raw
            .values()
            .find_map(|raw| match raw {
                RawSnapshot::Candidate(candidate) => Some((**candidate).clone()),
                _ => None,
            })
            .unwrap();
        typed.module.functions[0].values[0] = zil_ir::Type::Bool;
        candidate.module_fingerprint = typed.module.fingerprint();
        let mut session = CompilationSession::default();
        let report = session.compile_project_request(
            &project,
            &options,
            &mut |_| {},
            Some(LoadedProject::Semantic(Box::new((typed, candidate)))),
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert!(report.diagnostics.is_empty());
        assert_eq!(report.verification_runs, 2);
        assert_eq!(report.attempts.len(), 2);
        let rejected = &report.attempts[0];
        let accepted = &report.attempts[1];
        assert_eq!(
            rejected.retry_reason.as_deref(),
            Some("rejected_semantic_candidate")
        );
        assert_eq!(rejected.phase, Phase::Verify);
        assert!(!rejected.diagnostics.is_empty());
        assert!(rejected.function_type_work.is_none());
        assert!(rejected.initialization_work.is_none());
        assert_eq!(rejected.verification_runs, 1);
        assert_eq!(accepted.verification_runs, 1);
        assert_eq!(report.function_type_work, accepted.function_type_work);
        assert_eq!(report.initialization_work, accepted.initialization_work);
        assert!(rejected.phase_timings_ns[&Phase::Types] > 0);
        assert!(rejected.phase_timings_ns[&Phase::Analyze] > 0);
        assert!(rejected.phase_timings_ns[&Phase::Verify] > 0);
        assert_eq!(
            report.phase_timings_ns[&Phase::Verify],
            rejected.phase_timings_ns[&Phase::Verify] + accepted.phase_timings_ns[&Phase::Verify]
        );
        assert_eq!(
            report.phase_timings_ns[&Phase::Types],
            rejected.phase_timings_ns[&Phase::Types] + accepted.phase_timings_ns[&Phase::Types]
        );
        let full =
            CompilationSession::default().compile_project(&project, &CompileOptions::default());
        assert_eq!(report.status, full.status);
        assert_eq!(report.phase, full.phase);
        assert_eq!(report.diagnostics, full.diagnostics);
        assert_eq!(report.scalar_interfaces, full.scalar_interfaces);
        fs::remove_dir_all(directory).unwrap();
    }
}
