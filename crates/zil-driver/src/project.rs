//! Explicit source graphs. Module identity never depends on a concatenated name
//! or source buffer. The filesystem adapter is the only import-reading entry.
use super::artifacts::ArtifactPlan;
use super::query_cache::{CtUsage, Prepared};
use super::session::{
    RawSnapshot, TypedInput, check_entry_contract, disk_source, event, phase_key, query_key,
};
use super::*;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;
use zil_base::{CompileResult, ModuleId, SourceMap, Span};
use zil_db::{QueryKey, QuerySnapshot};
use zil_ir::{CoreImport, CoreProgram, ProgramModule};
use zil_syntax::{ParseResult, ast};

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
    core: CoreProgram,
    maps: Vec<SourceMap>,
    selection: super::ct_selection::Receipt,
}

#[derive(Clone, Debug)]
pub(super) struct EvaluatedProject {
    core: CoreProgram,
    usage: Option<Vec<CtUsage>>,
}

pub fn compile_project(project: &ProjectSources, options: &CompileOptions) -> CompileReport {
    CompilationSession::default().compile_project(project, options)
}

impl CompilationSession {
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
        self.compile_project_request(
            project,
            options,
            observer,
            None,
            &TimingTrace::new(options.timings),
        )
    }

    /// The filesystem adapter may lend the exact syntax it already parsed to
    /// discover imports. This private channel never carries verified authority
    /// and publishes nothing before the project/artifact guards have passed.
    fn compile_project_request(
        &mut self,
        project: &ProjectSources,
        options: &CompileOptions,
        observer: &mut dyn FnMut(&QueryEvent),
        loaded_syntax: Option<&[Arc<ParseResult>]>,
        trace: &TimingTrace,
    ) -> CompileReport {
        let mut request = trace.scope("compile_project", "request");
        if self.raw.len() > self.capacity {
            request.trace().value("session_eviction", || self.clear());
        }
        let start = Instant::now();
        let fallback = SourceUnit::new(FileId(0), project.root, "<project>", "");
        let root = project
            .modules
            .iter()
            .find(|m| m.module_id == project.root)
            .unwrap_or(&fallback);
        let mut report = CompileReport::new(root);
        report.query_cache = QueryCacheReport::requested(options, "checkpoint not reached");
        report.trace = request.trace().clone();
        if let Err(errors) =
            self.project_pipeline(project, options, &mut report, observer, loaded_syntax)
        {
            report.fail(errors);
        }
        report.seconds = start.elapsed().as_secs_f64();
        request.finish(report.status.as_str());
        report.timings = trace.report();
        report.trace = TimingTrace::default();
        report
    }

    pub(super) fn compile_project_path(
        &mut self,
        path: &Path,
        options: &CompileOptions,
    ) -> CompileReport {
        let start = Instant::now();
        let trace = TimingTrace::new(options.timings);
        let mut request = trace.scope("compile_path", "request");
        // Discovery may borrow immutable syntax from the previous request.
        // The existing compile_project_request capacity guard still clears
        // cache ownership before publishing this request; the lent Arcs do
        // not preserve query authority or add another retained cache.
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
        let loaded = request
            .trace()
            .measure("source_discovery", "stage", |trace| {
                check_timing_support(options)?;
                if options.stop_after == Stage::Parse {
                    trace
                        .measure("source_read", "stage", |_| read_source(path, options))
                        .map(|source| {
                            project.modules[0] = source;
                            Vec::new()
                        })
                } else {
                    load_sources(&mut project, path, options, self, trace)
                }
            });
        let mut report = match loaded {
            Ok(_) if options.stop_after == Stage::Parse => self.compile_source_traced(
                &project.modules[0],
                options,
                &mut |_| {},
                request.trace(),
            ),
            Ok(parsed) => self.compile_project_request(
                &project,
                options,
                &mut |_| {},
                Some(&parsed),
                request.trace(),
            ),
            Err(errors) => {
                let mut report = CompileReport::new(&project.modules[0]);
                report.query_cache = QueryCacheReport::requested(
                    options,
                    "source discovery failed before checkpoint lookup",
                );
                report.source_files = project
                    .modules
                    .iter()
                    .map(|m| (m.file_id, m.path.clone()))
                    .collect();
                report.fail(errors);
                report
            }
        };
        report.seconds = start.elapsed().as_secs_f64();
        request.finish(report.status.as_str());
        report.timings = trace.report();
        report
    }

    fn project_pipeline(
        &mut self,
        project: &ProjectSources,
        options: &CompileOptions,
        report: &mut CompileReport,
        observer: &mut dyn FnMut(&QueryEvent),
        loaded_syntax: Option<&[Arc<ParseResult>]>,
    ) -> CompileResult<()> {
        check_timing_support(options)?;
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
        let artifacts = ArtifactPlan::project(root, &project.modules, options)?;
        let fingerprint = project_fingerprint(project);
        report.input_fingerprint = fingerprint;
        report.source_files = project
            .modules
            .iter()
            .map(|m| (m.file_id, m.path.clone()))
            .collect();
        let identity = project_key(root, options, fingerprint, Phase::Parse, false);
        let target = options.target.fingerprint();
        let key = |phase, depends_on_target| phase_key(&identity, phase, depends_on_target, target);
        let mut parsed = Vec::new();
        let mut dependencies: Vec<Arc<QuerySnapshot>> = Vec::new();
        let mut remaining = options.limits.clone();
        for (index, source) in project.modules.iter().enumerate() {
            if options.cache_dir.is_some() {
                report.trace.clone().measure("source_cache", "stage", |_| {
                    disk_source(
                        source,
                        options,
                        report,
                        &query_key(source, options, Phase::Parse, false),
                    )
                })?;
            }
            let mut parse_options = options.clone();
            parse_options.limits = remaining.clone();
            // Tokens also bound front-end work across all modules. This is a
            // conservative project quota, including trivia and EOF tokens.
            parse_options.limits.max_tokens = remaining.max_tokens.min(remaining.max_nodes);
            let (raw, snapshot) = self.query(
                query_key(source, &parse_options, Phase::Parse, false),
                &[],
                options,
                report,
                observer,
                || {
                    if let Some(parsed) = loaded_syntax {
                        // load_sources parsed these immutable bytes with the
                        // same per-file remaining token/node/depth budgets.
                        // Its byte budget was charged when reading each file;
                        // validate_sources has independently checked the total.
                        return Ok(RawSnapshot::Parse(parsed[index].clone()));
                    }
                    let result = zil_syntax::parse_with_limits(source, &parse_options.limits);
                    if !result.diagnostics.is_empty() {
                        return Err(result.diagnostics);
                    }
                    Ok(RawSnapshot::Parse(Arc::new(result)))
                },
            )?;
            let RawSnapshot::Parse(result) = raw else {
                return Err(infra(
                    "E-QUERY-KIND",
                    "project parse query returned another snapshot kind",
                ));
            };
            charge_frontend(&mut remaining, &result)?;
            parsed.push(result);
            dependencies.push(snapshot);
        }
        report.phase = Phase::Parse;
        artifacts.dump_lazy(report, options, "syntax", |_| format!("{parsed:#?}"))?;
        if options.stop_after == Stage::Parse {
            return Ok(());
        }
        let imports = report.trace.measure("resolve_imports", "stage", |_| {
            resolve_imports(project, &parsed, &options.limits)
        })?;
        let selection_needed = project
            .modules
            .iter()
            .zip(&parsed)
            .any(|(source, parsed)| super::ct_selection::needed(source, parsed));
        let checkpoint = if selection_needed {
            // CP12 predates source selection and has no receipt for its CT
            // charges. Preserve the current source-selection budget and pipeline.
            report.query_cache = QueryCacheReport::requested(
                options,
                "source #if selection requires the original pipeline",
            );
            None
        } else {
            Prepared::new(
                options,
                &key(Phase::Types, true),
                &imports,
                loaded_syntax.is_some(),
                &mut report.query_cache,
            )
        };
        if let Some(cache) = &checkpoint {
            options
                .cancellation
                .check(Phase::Driver)
                .map_err(|e| vec![e])?;
            let loaded = report.trace.value("query_cache_lookup", || {
                cache.load(project, fingerprint, options, &mut report.query_cache)
            });
            if let Some(loaded) = loaded {
                // No private process-construction digest or old Verified value
                // is imported. The checkpoint supplies only raw current inputs.
                for phase in [Phase::Expand, Phase::Ct, Phase::Types, Phase::Analyze] {
                    options.cancellation.check(phase).map_err(|e| vec![e])?;
                    event(phase, QueryDisposition::Reused, report, observer);
                    options.cancellation.check(phase).map_err(|e| vec![e])?;
                }
                report.cache_hit = true;
                report.scalar_interfaces = loaded.program.interfaces.clone();
                return self.finish_candidate(
                    options,
                    report,
                    observer,
                    TypedInput::Program(Arc::new(loaded.program)),
                    Arc::new(loaded.candidate),
                    artifacts,
                );
            }
        }

        let (raw, expand_snapshot) = self.query(
            key(Phase::Expand, selection_needed),
            &dependencies,
            options,
            report,
            observer,
            || {
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
                let syntax = materialized.as_deref().unwrap_or(&syntax);
                for (module, map) in zil_expand::expand_program_with_limits(
                    &project.modules,
                    syntax,
                    &imports,
                    &options.limits,
                )? {
                    modules.push(module);
                    maps.push(map);
                }
                Ok(RawSnapshot::Project(Arc::new(ExpandedProject {
                    core: CoreProgram {
                        root: project.root,
                        source_fingerprint: fingerprint,
                        modules,
                    },
                    maps,
                    selection: selected
                        .map_or_else(super::ct_selection::Receipt::default, |selected| {
                            selected.receipt
                        }),
                })))
            },
        )?;
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
        if options.stop_after == Stage::Core {
            return Ok(());
        }
        let (raw, ct_snapshot) = self.query(
            key(Phase::Ct, true),
            std::slice::from_ref(&expand_snapshot),
            options,
            report,
            observer,
            || {
                let inferred =
                    zil_types::infer_program_closures_borrowed(&expanded.core, &options.limits)?;
                let mut remaining = expanded.selection.remaining(&options.limits)?;
                let context = zil_ct::SourceProgramContext::new(&inferred, &remaining)?;
                let mut modules = Vec::new();
                let mut recorded_usage = checkpoint.as_ref().map(|_| Vec::new());
                for module in &inferred.modules {
                    // Restrict execution to the remaining graph allowance BEFORE
                    // entering the CT verifier/VM, then deduct actual usage.
                    let compiler = super::ct_program::Compiler {
                        program: &inferred,
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
                        core.functions.len(),
                        Phase::Ct,
                        "project CT function budget exceeded",
                    )?;
                    if let Some(rows) = &mut recorded_usage {
                        rows.push(CtUsage {
                            module: module.core.module_id,
                            steps: usage.steps,
                            nodes: usage.nodes,
                            functions: core.functions.len(),
                        });
                    }
                    modules.push(ProgramModule {
                        core,
                        imports: module.imports.clone(),
                        exports: module.exports.clone(),
                    });
                }
                Ok(RawSnapshot::ProjectCore(Arc::new(EvaluatedProject {
                    core: CoreProgram {
                        root: project.root,
                        source_fingerprint: fingerprint,
                        modules,
                    },
                    usage: recorded_usage,
                })))
            },
        )?;
        let RawSnapshot::ProjectCore(evaluated) = raw else {
            return Err(infra(
                "E-QUERY-KIND",
                "project CT query returned another snapshot kind",
            ));
        };
        let core = &evaluated.core;
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
        let (raw, typed_snapshot) = self.query(
            key(Phase::Types, true),
            std::slice::from_ref(&ct_snapshot),
            options,
            report,
            observer,
            || {
                zil_types::check_program(core, &options.target, &options.limits)
                    .map(|typed| RawSnapshot::ProjectTyped(Arc::new(typed)))
            },
        )?;
        let RawSnapshot::ProjectTyped(typed) = raw else {
            return Err(infra(
                "E-QUERY-KIND",
                "project types query returned another snapshot kind",
            ));
        };
        report.phase = Phase::Types;
        report.scalar_interfaces = typed.interfaces.clone();
        artifacts.dump_lazy(report, options, "typed", |_| format!("{:#?}", typed.module))?;
        artifacts.dump_lazy(report, options, "interfaces", |_| {
            format!("{:#?}", typed.interfaces)
        })?;
        if options.stop_after == Stage::Types {
            return Ok(());
        }
        let candidate = self.finish_typed(
            options,
            report,
            observer,
            TypedInput::Program(typed.clone()),
            typed_snapshot,
            artifacts,
        )?;
        if let Some(cache) = checkpoint {
            cache.store(
                &typed,
                &candidate,
                evaluated.usage.as_deref(),
                project,
                options,
                report,
            );
        }
        Ok(())
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
fn valid_segment(segment: &str) -> bool {
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
) -> CompileResult<Vec<Vec<CoreImport>>> {
    let mut remaining = limits.max_nodes;
    let mut graph = Vec::new();
    for module in parsed {
        let mut imports = Vec::new();
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
            let id = project.paths.get(&import.path).copied().ok_or_else(|| {
                reject(
                    "E-MODULE-NOT-FOUND",
                    Some(item.span),
                    format!("module {} was not supplied", import.path.join(".")),
                )
            })?;
            let alias = import
                .alias
                .clone()
                .or_else(|| import.path.last().cloned())
                .ok_or_else(|| reject("E-MODULE-PATH", Some(item.span), "empty import path"))?;
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

fn unsupported_import(item: &ast::Item, import: &ast::Import) -> CompileResult<()> {
    if item.public || import.syntax {
        return Err(vec![Diagnostic::unsupported(
            Phase::Expand,
            "public reexports and syntax imports require a later module interface",
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

fn resolve_file_module(root: &Path, logical: &[String], span: Span) -> CompileResult<PathBuf> {
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

fn load_sources(
    project: &mut ProjectSources,
    path: &Path,
    options: &CompileOptions,
    session: &CompilationSession,
    trace: &TimingTrace,
) -> CompileResult<Vec<Arc<ParseResult>>> {
    options
        .cancellation
        .check(Phase::Driver)
        .map_err(|e| vec![e])?;
    let root_path = std::fs::canonicalize(path)
        .map_err(|e| infra("E-SOURCE-READ", format!("{}: {e}", path.display())))?;
    let directory = root_path
        .parent()
        .ok_or_else(|| infra("E-SOURCE-READ", "entry must have a parent directory"))?;
    let mut remaining = options.limits.clone();
    if remaining.max_nodes == 0 {
        return Err(limit(Phase::Parse, "project module budget exceeded"));
    }
    project.modules[0] =
        trace.measure("source_read", "stage", |_| read_source(&root_path, options))?;
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
    let mut known = BTreeMap::from([(root_path.clone(), project.root)]);
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
        query_options.limits.max_tokens = parse_limits.max_tokens;
        query_options.limits.max_nodes = remaining.max_nodes;
        let key = query_key(&project.modules[index], &query_options, Phase::Parse, false);
        let cached = if trace
            .measure("discovery_query", "stage", |_| {
                session.database.read(&key).map_err(|e| vec![e])
            })?
            .is_some()
        {
            match session.raw.get(&key) {
                Some(RawSnapshot::Parse(p)) => Some(p.clone()),
                _ => None,
            }
        } else {
            None
        };
        let parsed = cached.unwrap_or_else(|| {
            let mut timing = trace.scope("parse", "stage");
            let parsed = Arc::new(zil_syntax::parse_with_limits(
                &project.modules[index],
                &parse_limits,
            ));
            timing.finish(if parsed.diagnostics.is_empty() {
                "ok"
            } else {
                "error"
            });
            parsed
        });
        if !parsed.diagnostics.is_empty() {
            return Err(parsed.diagnostics.clone());
        }
        charge_frontend(&mut remaining, &parsed)?;
        for item in &parsed.module.items {
            let ast::ItemKind::Import(import) = &item.kind else {
                continue;
            };
            unsupported_import(item, import)?;
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
            // Resolve even an already registered path: the root is preseeded
            // in `paths`, but that must not hide a second directory entry.
            let canonical = resolve_file_module(directory, &import.path, item.span)?;
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
                let mut source = trace
                    .measure("source_read", "stage", |_| {
                        read_source(&canonical, &read_options)
                    })
                    .map_err(|errors| {
                        errors
                            .into_iter()
                            .map(|e| {
                                e.with_note(format!(
                                    "imported from {}",
                                    project.modules[index].path
                                ))
                            })
                            .collect::<Vec<_>>()
                    })?;
                take(
                    &mut remaining.max_source_bytes,
                    source.text.len(),
                    Phase::Parse,
                    "project source byte budget exceeded",
                )?;
                source.module_id = ModuleId(number);
                source.file_id = FileId(number);
                known.insert(canonical, source.module_id);
                project.modules.push(source);
                ModuleId(number)
            };
            project.paths.insert(import.path.clone(), id);
        }
        syntax.push(parsed);
        index += 1;
    }
    Ok(syntax)
}

#[cfg(test)]
mod discovery158_tests {
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
        let mut p = ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "placeholder", "")],
            paths: BTreeMap::new(),
        };
        load_sources(&mut p, path, options, session, &TimingTrace::default())
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
    fn capacity_clear_keeps_immutable_lent_syntax_but_not_query_authority() {
        let (_dir, path, mut session) = setup();
        session.capacity = 1;
        assert!(session.raw.len() > session.capacity);
        let before = discover(&session, &path, &CompileOptions::default()).unwrap();
        let r = session.compile_path(&path, &CompileOptions::default());
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        assert!(!r.cache_hit); // Old query authority was actually cleared.
        let after = discover(&session, &path, &CompileOptions::default()).unwrap();
        for (a, b) in before.iter().zip(after.iter()) {
            assert!(Arc::ptr_eq(a, b));
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
}
