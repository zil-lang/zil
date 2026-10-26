use super::artifacts::ArtifactPlan;
use super::*;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use zil_backend_llvm::{BuildOptions, EmitOptions, EntryPoint};
use zil_base::{
    CompileResult, Diagnostic, Fingerprint, Phase, RULE_VERSION, ResultCategory, SCHEMA_VERSION,
    SourceUnit,
};
use zil_db::{
    ArtifactCache, CacheRead, CacheWriteError, Database, Dependency, FingerprintKind, QueryKey,
    QuerySnapshot, ResultFingerprints,
};
use zil_ir::{CandidatePlan, CoreModule, Type, TypedModule};
use zil_syntax::ParseResult;

/// Only unverified, immutable data can be retained across requests. There is no
/// VerifiedModule/VerifiedCTIR variant and no disk deserializer for this enum.
#[derive(Clone, Debug)]
pub(super) enum RawSnapshot {
    Parse(Arc<ParseResult>),
    Expanded(Arc<zil_expand::ExpandedModule>),
    Core(Arc<CoreModule>),
    Typed(Arc<TypedModule>),
    Candidate(Arc<CandidatePlan>),
    Project(Arc<super::project::ExpandedProject>),
    ProjectCore(Arc<zil_ir::CoreProgram>),
    ProjectTyped(Arc<zil_ir::TypedProgram>),
}
impl RawSnapshot {
    fn digest(&self) -> Fingerprint {
        match self {
            Self::Typed(typed) => typed.fingerprint(),
            other => Fingerprint::of(format!("{other:?}").as_bytes()),
        }
    }
}

pub struct CompilationSession {
    pub(super) database: Database,
    pub(super) raw: BTreeMap<QueryKey, RawSnapshot>,
    pub(super) capacity: usize,
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
        }
    }
    pub fn clear(&mut self) {
        self.database.clear();
        self.raw.clear();
    }
    pub fn cached_query_count(&self) -> usize {
        self.raw.len()
    }
    pub fn active_query_count(&self) -> usize {
        self.database.active_count()
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
        if self.raw.len() > self.capacity {
            self.clear();
        }
        let start = Instant::now();
        let mut report = CompileReport::new(source);
        if let Err(errors) = self.pipeline(source, options, &mut report, observer) {
            report.fail(errors);
        }
        report.seconds = start.elapsed().as_secs_f64();
        report
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
        let digest = raw.digest();
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
        let parse_key = query_key(source, options, Phase::Parse, false);
        disk_source(source, options, report, &parse_key)?;
        // These immutable identities describe this request, not a proof. Reuse
        // them vertically instead of rehashing source bytes for every phase.
        let target = options.target.fingerprint();
        let key =
            |phase, depends_on_target| phase_key(&parse_key, phase, depends_on_target, target);
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

        let (core, core_snapshot) = self.query(
            key(Phase::Expand, false),
            std::slice::from_ref(&parse_snapshot),
            options,
            report,
            observer,
            || {
                let expanded =
                    zil_expand::expand_with_map_limits(source, &parsed.module, &options.limits)?;
                expanded
                    .source_map
                    .validate(std::slice::from_ref(source), &options.limits)?;
                Ok(RawSnapshot::Expanded(Arc::new(expanded)))
            },
        )?;
        let RawSnapshot::Expanded(expanded) = core else {
            return Err(infra(
                "E-QUERY-KIND",
                "Core query returned another snapshot kind",
            ));
        };
        let core = &expanded.core;
        report.phase = Phase::Expand;
        artifacts.dump_lazy(report, options, "core", |_| format!("{core:#?}"))?;
        artifacts.dump_lazy(report, options, "source-map", |_| {
            format!("{:#?}", expanded.source_map)
        })?;
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
                super::ct_program::single(&inferred, &options.target, &options.limits)
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
            &typed,
            typed_snapshot,
            artifacts,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish_typed(
        &mut self,
        options: &CompileOptions,
        report: &mut CompileReport,
        observer: &mut dyn FnMut(&QueryEvent),
        typed: &TypedModule,
        typed_snapshot: Arc<QuerySnapshot>,
        artifacts: ArtifactPlan,
        project: Option<&zil_ir::TypedProgram>,
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
                zil_analyze::analyze(typed, &options.limits)
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
        let verified = zil_verify::verify_module(
            typed.clone(),
            (*candidate).clone(),
            &options.target,
            &options.limits,
        );
        event(Phase::Verify, QueryDisposition::Executed, report, observer);
        options
            .cancellation
            .check(Phase::Verify)
            .map_err(|error| vec![error])?;
        let verified = verified?;
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
        let lir = zil_lower::lower(&verified)?;
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
        build.optimization = options.optimization;
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
        let artifact = zil_backend_llvm::build(&verified, &lir, &build)?;
        event(Phase::Link, QueryDisposition::Executed, report, observer);
        if let Err(error) = options.cancellation.check(Phase::Link) {
            return Err(vec![error]);
        }
        let llvm = fs::read_to_string(&artifact.llvm_ir)
            .map_err(|error| infra("E-ARTIFACT-IO", error.to_string()))?;
        report.phase = Phase::Link;
        artifacts.dump(report, options, "llvm", &llvm)?;
        artifacts.validate()?;
        let artifact = publication.commit(artifact, &options.cancellation)?;
        report.artifacts.insert("llvm".into(), artifact.llvm_ir);
        report
            .artifacts
            .insert("executable".into(), artifact.executable);
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
    let limits = &options.limits;
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
