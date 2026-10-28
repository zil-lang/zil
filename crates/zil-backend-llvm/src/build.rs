use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use zil_base::{CompileResult, Diagnostic, Phase, ResultCategory};
use zil_ir::LirModule;
use zil_verify::VerifiedModule;

use crate::object_cache::{
    Access, ObjectCacheSummary, PendingObject, PendingObjectCache, PreparedCache,
};
use crate::{EmitOptions, EntryPoint};

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
}
impl CodegenSummary {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"backend\":{},\"object_reports\":[{}],\"object_cache\":{},\"requested_units\":{},\"actual_units\":{},\"jobs\":{},\"fallback\":{}}}",
            zil_base::json_string(self.backend.name()),
            self.object_reports.join(","),
            self.object_cache.to_json(),
            self.requested_units,
            self.actual_units,
            self.jobs,
            self.fallback
                .as_ref()
                .map_or_else(|| "null".into(), |s| zil_base::json_string(s))
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildOptions {
    pub backend: NativeBackend,
    pub clang: PathBuf,
    pub output: PathBuf,
    pub optimization: Optimization,
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
    let machine = crate::emit::lower_with_options(verified, lir, &options.emit)?;
    // Diagnostic/reference output only. API preflight runs once per owned unit,
    // immediately before its LLVMBuilder calls; it cannot be bypassed.
    let llvm = machine.to_text();
    let plan =
        crate::machine_cgu::partition(machine, lir, options.emit.entry, options.codegen.units);
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
    let mut cache_summary = ObjectCacheSummary {
        requested: options.object_cache.is_some(),
        ..ObjectCacheSummary::default()
    };
    let cache = options.object_cache.as_ref().and_then(|path| {
        match PreparedCache::new(path, &lir.target, options.optimization) {
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
    let mut pending_cache = PendingObjectCache {
        directory: cache.as_ref().map(|c| c.directory.clone()),
        ..PendingObjectCache::default()
    };
    let actual_units = plan.units.len().max(1);
    let mut codegen = CodegenSummary {
        backend: options.backend,
        object_reports: Vec::new(),
        object_cache: cache_summary,
        requested_units: options.codegen.units,
        actual_units,
        jobs: options.codegen.jobs.min(actual_units),
        fallback: plan.fallback.map(str::to_owned),
    };
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
    fs::write(&ir_path, llvm.as_bytes())
        .map_err(|error| io_error("write LLVM IR", &ir_path, error))?;

    let optimization = match options.optimization {
        Optimization::O0 => "-O0",
        Optimization::O2 => "-O2",
    };
    let mut commands = Vec::new();
    let split = plan.units.len() > 1;
    let objects = if !split && options.backend == NativeBackend::Clang {
        // Keep the legacy one-process Unix path exactly, including its flags.
        drop(plan.units);
        let mut command = Command::new(&options.clang);
        command
            .arg(format!("--target={}", lir.target.triple))
            .arg("-x")
            .arg("ir")
            .arg(optimization);
        if windows {
            command.arg("-c");
        }
        command.arg(&ir_path).arg("-o").arg(if windows {
            &object_path
        } else {
            &executable_path
        });
        commands.push(run_command(&mut command, "E-LLVM-COMPILE")?);
        vec![object_path]
    } else {
        let units = plan.units;
        let inputs = units
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
                })
            })
            .collect::<CompileResult<Vec<_>>>()?;
        let compiled = compile_units(
            &inputs,
            &lir.target.triple,
            optimization,
            options,
            cache.as_ref(),
            cancelled,
        )?;
        for result in compiled {
            if let Some(command) = result.command {
                commands.push(command);
            }
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
        inputs
            .into_iter()
            .map(|unit| unit.object)
            .collect::<Vec<_>>()
    };
    check_cancelled(cancelled)?;
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
        commands.push(run_command(&mut link, "E-LLVM-COMPILE")?);
    }
    check_cancelled(cancelled)?;
    let recorded_command = commands[0].clone();
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
}
struct UnitResult {
    command: Option<Vec<String>>,
    receipt: Option<String>,
    access: Option<Access>,
    pending: Option<PendingObject>,
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
                                    let command = run_command(&mut command, "E-LLVM-COMPILE")?;
                                    if !unit.object.is_file() {
                                        return Err(infra(
                                            "E-LINK-MISSING-OUTPUT",
                                            "LLVM did not produce a codegen object",
                                        ));
                                    }
                                    Ok(UnitResult {
                                        command: Some(command),
                                        receipt: None,
                                        access: None,
                                        pending: None,
                                    })
                                }
                                NativeBackend::LlvmApi => compile_api_unit(
                                    unit,
                                    target,
                                    options.optimization,
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
        let (mut lookup, bytes) = cache.lookup(&unit.module);
        check_cancelled(cancelled)?;
        if let Some(bytes) = bytes {
            // A hit is not a native Builder invocation. Current machine checks
            // still run, independently of the checks performed by the producer.
            let t = std::time::Instant::now();
            zil_llvm_api::validate_module(&unit.module).map_err(api_error)?;
            lookup.preflight_ns = crate::object_cache::elapsed_ns(t);
            check_cancelled(cancelled)?;
            fs::write(&unit.object, &bytes)
                .map_err(|e| io_error("write reused object", &unit.object, e))?;
            let q = zil_base::json_string;
            let receipt = format!(
                "{{\"backend\":\"llvm-api\",\"input_kind\":\"object-cache\",\"llvm_version\":{},\"triple\":{},\"object_bytes\":{},\"cache\":{},\"llvm_verification_this_request\":false,\"producer_verified_before_after\":true,\"preflight_ns\":{},\"builder_ns\":0,\"parse_ns\":0,\"optimize_ns\":0,\"emit_ns\":0,\"total_ns\":{}}}",
                q(&cache.llvm_version),
                q(target),
                bytes.len(),
                lookup.to_json(),
                lookup.preflight_ns,
                lookup.key_ns + lookup.lookup_ns + lookup.preflight_ns
            );
            return Ok(UnitResult {
                command: None,
                receipt: Some(receipt),
                access: Some(lookup),
                pending: None,
            });
        }
        access = Some(lookup);
    }
    let object =
        zil_llvm_api::compile_module(&unit.module, &config, cancelled).map_err(api_error)?;
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
    let pending = access.as_ref().map(|a| PendingObject {
        key: a.key,
        bytes: object.bytes,
    });
    Ok(UnitResult {
        command: None,
        receipt: Some(receipt),
        access,
        pending,
    })
}
#[cfg(not(feature = "llvm-api"))]
fn compile_api_unit(
    _unit: &UnitInput,
    _target: &str,
    _opt: Optimization,
    _cache: Option<&PreparedCache>,
    _cancelled: &(dyn Fn() -> bool + Sync),
) -> CompileResult<UnitResult> {
    Err(infra(
        "E-LLVM-API-UNAVAILABLE",
        "LLVM API support was not compiled in",
    ))
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
