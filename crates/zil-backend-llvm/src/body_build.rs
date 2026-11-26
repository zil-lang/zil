//! Native build driver for `zil_check::VerifiedModule` using LLVM 23 clang at O0/O2.

use crate::body_emit::{BodyEmitError, emit_verified_body_units_with_entry_for_target};
use crate::{NativeBackend, Optimization, WindowsLinkOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zil_base::TargetProfile;
use zil_check::VerifiedModule;

#[derive(Clone, Debug)]
pub struct BodyBuildOptions {
    pub output: PathBuf,
    pub optimization: Optimization,
    pub backend: NativeBackend,
    pub target: TargetProfile,
    pub windows: Option<WindowsLinkOptions>,
    pub clang: Option<PathBuf>,
    pub extra_c_sources: Vec<PathBuf>,
    pub emit_c_main: bool,
    pub entry: Option<String>,
    pub codegen_units: usize,
    pub codegen_jobs: usize,
    pub object_cache: Option<PathBuf>,
    /// Maximum on-disk byte budget for `object_cache` (`None` = unlimited).
    /// When exceeded after new misses, oldest entries by mtime (excluding active
    /// objects of the current build) are evicted down to an 80% watermark.
    pub object_cache_max_bytes: Option<u64>,
    /// Whether `NativeBackend::LlvmApi` should write the diagnostic `<output>.ll`
    /// text dump to disk. Disabling it avoids multi-MB IR text I/O on cached builds.
    pub emit_ir_file: bool,
}

#[derive(Clone, Debug)]
pub struct BodyBuildOutput {
    pub ir: String,
    pub ir_path: PathBuf,
    pub commands: Vec<Vec<String>>,
    pub object_reports: Vec<String>,
    pub actual_units: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub cache_evicted_entries: usize,
    pub cache_evicted_bytes: u64,
    pub cache_evict_ns: u64,
}

#[derive(Clone, Debug)]
pub struct BodyBuildError {
    pub message: String,
    pub unsupported: bool,
}

impl BodyBuildError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            unsupported: false,
        }
    }
}

impl From<BodyEmitError> for BodyBuildError {
    fn from(e: BodyEmitError) -> Self {
        Self {
            message: e.message,
            unsupported: e.unsupported,
        }
    }
}

#[cfg(feature = "llvm-api")]
fn receipt_to_json(receipt: &zil_llvm_api::Receipt) -> String {
    format!(
        "{{\"backend\":\"llvm-api\",\"llvm_version\":{},\"triple\":{},\"cpu\":{},\"features\":{},\"data_layout\":{},\"pipeline\":{},\"relocation\":{},\"code_model\":{},\"verify_before_after\":{},\"input_kind\":{},\"preflight_ns\":{},\"builder_ns\":{},\"parse_ns\":{},\"optimize_ns\":{},\"emit_ns\":{},\"total_ns\":{}}}",
        zil_base::json_string(&receipt.llvm_version),
        zil_base::json_string(&receipt.triple),
        zil_base::json_string(&receipt.cpu),
        zil_base::json_string(&receipt.features),
        zil_base::json_string(&receipt.data_layout),
        zil_base::json_string(&receipt.pipeline),
        zil_base::json_string(&receipt.relocation),
        zil_base::json_string(&receipt.code_model),
        receipt.verify_before_after,
        zil_base::json_string(&receipt.input_kind),
        receipt.preflight_ns,
        receipt.builder_ns,
        receipt.parse_ns,
        receipt.optimize_ns,
        receipt.emit_ns,
        receipt.total_ns,
    )
}

#[cfg(feature = "llvm-api")]
fn cache_hit_receipt_json(triple: &str, opt_flag: &str) -> String {
    format!(
        "{{\"backend\":\"llvm-api\",\"llvm_version\":\"cached\",\"triple\":{},\"cpu\":\"generic\",\"features\":\"\",\"data_layout\":\"\",\"pipeline\":{},\"relocation\":\"pic\",\"code_model\":\"default\",\"verify_before_after\":false,\"input_kind\":\"object-cache-hit\",\"preflight_ns\":0,\"builder_ns\":0,\"parse_ns\":0,\"optimize_ns\":0,\"emit_ns\":0,\"total_ns\":0}}",
        zil_base::json_string(triple),
        zil_base::json_string(opt_flag),
    )
}

#[cfg(feature = "llvm-api")]
fn unit_cache_key(triple: &str, opt_flag: &str, unit_ir: &str) -> String {
    let mut h = zil_base::FingerprintBuilder::new();
    h.update(b"zil-obj-cache-v2\0");
    h.update(env!("CARGO_PKG_VERSION").as_bytes());
    h.update(b"\0");
    h.update(triple.as_bytes());
    h.update(b"\0");
    h.update(opt_flag.as_bytes());
    h.update(b"\0");
    h.update(unit_ir.as_bytes());
    h.digest().to_hex()
}

#[cfg(feature = "llvm-api")]
fn touch_cache_mtime(path: &Path) {
    if let Ok(file) = fs::OpenOptions::new().append(true).open(path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
}

#[cfg(feature = "llvm-api")]
fn enforce_object_cache_budget(
    cache_dir: &Path,
    max_bytes: u64,
    obj_ext: &str,
    active_paths: &std::collections::HashSet<PathBuf>,
) -> (usize, u64, u64) {
    let start = std::time::Instant::now();
    let suffix = format!(".{obj_ext}");
    let Ok(read_dir) = fs::read_dir(cache_dir) else {
        return (0, 0, start.elapsed().as_nanos() as u64);
    };
    let mut total_bytes = 0u64;
    let mut candidates: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(&suffix) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let sz = meta.len();
        total_bytes = total_bytes.saturating_add(sz);
        if !active_paths.contains(&path) {
            let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            candidates.push((mtime, sz, path));
        }
    }
    if total_bytes <= max_bytes {
        return (0, 0, start.elapsed().as_nanos() as u64);
    }
    let watermark = (max_bytes.saturating_mul(4)) / 5;
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
    let mut evicted_entries = 0usize;
    let mut evicted_bytes = 0u64;
    for (_, sz, path) in candidates {
        if total_bytes <= watermark {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total_bytes = total_bytes.saturating_sub(sz);
            evicted_entries += 1;
            evicted_bytes = evicted_bytes.saturating_add(sz);
        }
    }
    (
        evicted_entries,
        evicted_bytes,
        start.elapsed().as_nanos() as u64,
    )
}

fn run_checked_command(mut cmd: Command, label: &str, tool: &Path) -> Result<(), BodyBuildError> {
    let out = cmd.output().map_err(|e| {
        BodyBuildError::new(format!(
            "failed to invoke {label} ({}): {e}",
            tool.display()
        ))
    })?;
    if !out.status.success() {
        return Err(BodyBuildError::new(format!(
            "{label} failed ({}):\nstdout: {}\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

fn link_windows_lld(
    clang: &Path,
    target_flag: &str,
    opt_flag: &str,
    mut obj_paths: Vec<PathBuf>,
    options: &BodyBuildOptions,
    windows: &WindowsLinkOptions,
    commands: &mut Vec<Vec<String>>,
) -> Result<(), BodyBuildError> {
    for (idx, c_src) in options.extra_c_sources.iter().enumerate() {
        let c_obj = PathBuf::from(format!("{}.c{idx}.obj", options.output.display()));
        let mut c_cmd = Command::new(clang);
        let c_argv = vec![
            clang.display().to_string(),
            target_flag.to_owned(),
            "-x".into(),
            "c".into(),
            opt_flag.into(),
            "-c".into(),
            c_src.display().to_string(),
            "-o".into(),
            c_obj.display().to_string(),
        ];
        c_cmd
            .arg(target_flag)
            .arg("-x")
            .arg("c")
            .arg(opt_flag)
            .arg("-c")
            .arg(c_src)
            .arg("-o")
            .arg(&c_obj);
        run_checked_command(c_cmd, "clang", clang)?;
        commands.push(c_argv);
        obj_paths.push(c_obj);
    }

    let out_arg = format!("/out:{}", options.output.display());
    let mut link_cmd = Command::new(&windows.linker);
    let mut link_argv = vec![
        windows.linker.display().to_string(),
        "/nologo".into(),
        "/subsystem:console".into(),
        out_arg.clone(),
    ];
    link_cmd
        .arg("/nologo")
        .arg("/subsystem:console")
        .arg(&out_arg);
    for obj in &obj_paths {
        link_cmd.arg(obj);
        link_argv.push(obj.display().to_string());
    }
    for lib in &windows.import_libraries {
        link_cmd.arg(lib);
        link_argv.push(lib.display().to_string());
    }
    run_checked_command(link_cmd, "linker", &windows.linker)?;
    commands.push(link_argv);
    Ok(())
}

/// Emits LLVM 23 IR from `verified` and compiles/links a native executable.
///
/// For `NativeBackend::LlvmApi` (when built with feature `llvm-api`), partitions
/// the module into `actual_units = codegen_units.clamp(1, fn_count.max(1))`
/// independent IR units, compiles each unit in parallel (up to `codegen_jobs`)
/// through `zil-llvm-api` to its own object file (`.o` on ELF, `.obj` on COFF),
/// and invokes `clang` (or explicit `WindowsLinkOptions::linker`) strictly for
/// linking the produced object files into the final executable.
pub fn build_verified_body(
    verified: &VerifiedModule,
    options: &BodyBuildOptions,
) -> Result<BodyBuildOutput, BodyBuildError> {
    let triple = options.target.triple.as_str();
    let is_windows = triple.contains("windows");
    let obj_ext = if is_windows { "obj" } else { "o" };
    let entry = options.entry.as_deref();
    let ir_path = PathBuf::from(format!("{}.ll", options.output.display()));
    if let Some(parent) = ir_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|e| BodyBuildError::new(format!("failed to create output directory: {e}")))?;
    }

    let clang = options
        .clang
        .clone()
        .unwrap_or_else(|| PathBuf::from("clang"));
    let target_flag = format!("--target={triple}");
    let opt_flag = match options.optimization {
        Optimization::O0 => "-O0",
        Optimization::O2 => "-O2",
    };

    match options.backend {
        NativeBackend::LlvmApi => {
            #[cfg(feature = "llvm-api")]
            {
                let requested = options.codegen_units.max(1);
                let unit_irs = emit_verified_body_units_with_entry_for_target(
                    verified,
                    options.emit_c_main,
                    requested,
                    entry,
                    triple,
                )?;
                let actual_units = unit_irs.len();
                let ir = if options.emit_ir_file {
                    let text = if actual_units == 1 {
                        unit_irs[0].clone()
                    } else {
                        unit_irs.join("\n; --- unit boundary ---\n")
                    };
                    fs::write(&ir_path, &text).map_err(|e| {
                        BodyBuildError::new(format!("failed to write LLVM IR: {e}"))
                    })?;
                    text
                } else {
                    String::new()
                };

                let cache_dir = options.object_cache.as_deref();
                if let Some(dir) = cache_dir {
                    fs::create_dir_all(dir).map_err(|e| {
                        BodyBuildError::new(format!(
                            "failed to create object cache directory {}: {e}",
                            dir.display()
                        ))
                    })?;
                }

                let api_opt = match options.optimization {
                    Optimization::O0 => zil_llvm_api::Optimization::O0,
                    Optimization::O2 => zil_llvm_api::Optimization::O2,
                };
                let jobs = options.codegen_jobs.clamp(1, actual_units);

                struct UnitCompiled {
                    /// Ready-to-link cached object path (zero-copy when `cache_dir` is enabled).
                    cached_path: Option<PathBuf>,
                    /// In-memory bytes when `cache_dir` is disabled (or fallback).
                    bytes: Vec<u8>,
                    report: String,
                    cache_hit: bool,
                }
                let results: Vec<std::sync::Mutex<Option<Result<UnitCompiled, String>>>> = (0
                    ..actual_units)
                    .map(|_| std::sync::Mutex::new(None))
                    .collect();
                let next_idx = std::sync::atomic::AtomicUsize::new(0);
                let pid = std::process::id();

                std::thread::scope(|s| {
                    for _ in 0..jobs {
                        s.spawn(|| {
                            let config = zil_llvm_api::Config {
                                triple,
                                optimization: api_opt,
                            };
                            loop {
                                let idx =
                                    next_idx.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                if idx >= actual_units {
                                    break;
                                }
                                let cache_path = cache_dir.map(|dir| {
                                    let key = unit_cache_key(triple, opt_flag, &unit_irs[idx]);
                                    dir.join(format!("{key}.{obj_ext}"))
                                });
                                let compiled = if let Some(ref cp) = cache_path
                                    && cp.metadata().is_ok_and(|m| m.is_file() && m.len() > 0)
                                {
                                    touch_cache_mtime(cp);
                                    Ok(UnitCompiled {
                                        cached_path: Some(cp.clone()),
                                        bytes: Vec::new(),
                                        report: cache_hit_receipt_json(triple, opt_flag),
                                        cache_hit: true,
                                    })
                                } else {
                                    crate::body_structured::build_structured_module(
                                        &unit_irs[idx],
                                        triple,
                                        &format!("unit-{idx}"),
                                    )
                                    .map_err(|e| {
                                        format!(
                                            "zil-llvm-api unit {idx} structured build failed: {}",
                                            e.message
                                        )
                                    })
                                    .and_then(|structured| {
                                        zil_llvm_api::compile_module(&structured, &config, &|| {
                                            false
                                        })
                                        .map_err(|e| format!("zil-llvm-api unit {idx} failed: {e}"))
                                    })
                                    .map(|object| {
                                        let mut linked_cache_path = None;
                                        if let Some(ref cp) = cache_path {
                                            let tmp = cp.with_extension(format!(
                                                "{obj_ext}.tmp.{pid}.{idx}"
                                            ));
                                            if fs::write(&tmp, &object.bytes).is_ok()
                                                && fs::rename(&tmp, cp).is_ok()
                                            {
                                                linked_cache_path = Some(cp.clone());
                                            }
                                        }
                                        let bytes = if linked_cache_path.is_some() {
                                            Vec::new()
                                        } else {
                                            object.bytes
                                        };
                                        UnitCompiled {
                                            cached_path: linked_cache_path,
                                            bytes,
                                            report: receipt_to_json(&object.receipt),
                                            cache_hit: false,
                                        }
                                    })
                                };
                                if let Ok(mut slot) = results[idx].lock() {
                                    *slot = Some(compiled);
                                }
                            }
                        });
                    }
                });

                let mut obj_paths = Vec::with_capacity(actual_units);
                let mut temp_obj_paths = Vec::new();
                let mut active_cache_paths = std::collections::HashSet::new();
                let mut object_reports = Vec::with_capacity(actual_units);
                let mut cache_hits = 0usize;
                let mut cache_misses = 0usize;
                for (idx, slot) in results.into_iter().enumerate() {
                    let unit = slot
                        .into_inner()
                        .ok()
                        .flatten()
                        .ok_or_else(|| {
                            BodyBuildError::new(format!(
                                "missing compilation result for unit {idx}"
                            ))
                        })?
                        .map_err(BodyBuildError::new)?;
                    if unit.cache_hit {
                        cache_hits += 1;
                    } else if cache_dir.is_some() {
                        cache_misses += 1;
                    }
                    if let Some(cp) = unit.cached_path {
                        active_cache_paths.insert(cp.clone());
                        obj_paths.push(cp);
                    } else {
                        let obj_path = if actual_units == 1 {
                            PathBuf::from(format!("{}.{obj_ext}", options.output.display()))
                        } else {
                            PathBuf::from(format!("{}.u{idx}.{obj_ext}", options.output.display()))
                        };
                        fs::write(&obj_path, &unit.bytes).map_err(|e| {
                            BodyBuildError::new(format!(
                                "failed to write object file {}: {e}",
                                obj_path.display()
                            ))
                        })?;
                        temp_obj_paths.push(obj_path.clone());
                        obj_paths.push(obj_path);
                    }
                    object_reports.push(unit.report);
                }

                let mut commands = Vec::new();
                if is_windows && let Some(windows) = &options.windows {
                    link_windows_lld(
                        &clang,
                        &target_flag,
                        opt_flag,
                        obj_paths,
                        options,
                        windows,
                        &mut commands,
                    )?;
                } else {
                    let mut cmd = Command::new(&clang);
                    let mut argv = vec![
                        clang.display().to_string(),
                        target_flag.clone(),
                        opt_flag.into(),
                    ];
                    cmd.arg(&target_flag).arg(opt_flag);
                    for obj_path in &obj_paths {
                        cmd.arg(obj_path);
                        argv.push(obj_path.display().to_string());
                    }
                    for c_src in &options.extra_c_sources {
                        cmd.arg(c_src);
                        argv.push(c_src.display().to_string());
                    }
                    cmd.arg("-o").arg(&options.output);
                    argv.push("-o".into());
                    argv.push(options.output.display().to_string());
                    if is_windows {
                        cmd.arg("-llegacy_stdio_definitions").arg("-lws2_32");
                        argv.push("-llegacy_stdio_definitions".into());
                        argv.push("-lws2_32".into());
                    }
                    run_checked_command(cmd, "linker", &clang)?;
                    commands.push(argv);
                }
                for tmp_obj in temp_obj_paths {
                    let _ = fs::remove_file(tmp_obj);
                }
                let (cache_evicted_entries, cache_evicted_bytes, cache_evict_ns) = if cache_misses
                    > 0
                    && let Some(dir) = cache_dir
                    && let Some(max_bytes) = options.object_cache_max_bytes
                    && max_bytes > 0
                {
                    enforce_object_cache_budget(dir, max_bytes, obj_ext, &active_cache_paths)
                } else {
                    (0, 0, 0)
                };
                Ok(BodyBuildOutput {
                    ir,
                    ir_path,
                    commands,
                    object_reports,
                    actual_units,
                    cache_hits,
                    cache_misses,
                    cache_evicted_entries,
                    cache_evicted_bytes,
                    cache_evict_ns,
                })
            }
            #[cfg(not(feature = "llvm-api"))]
            {
                let _ = (obj_ext, target_flag, opt_flag, clang, is_windows);
                Err(BodyBuildError::new(
                    "llvm-api backend is not enabled in this build",
                ))
            }
        }
        NativeBackend::Clang => {
            let ir = emit_verified_body_units_with_entry_for_target(
                verified,
                options.emit_c_main,
                1,
                entry,
                triple,
            )?
            .pop()
            .unwrap_or_default();
            fs::write(&ir_path, &ir)
                .map_err(|e| BodyBuildError::new(format!("failed to write LLVM IR: {e}")))?;
            let mut commands = Vec::new();
            if is_windows && let Some(windows) = &options.windows {
                let obj_path = PathBuf::from(format!("{}.{obj_ext}", options.output.display()));
                let mut compile_cmd = Command::new(&clang);
                let compile_argv = vec![
                    clang.display().to_string(),
                    target_flag.clone(),
                    "-x".into(),
                    "ir".into(),
                    opt_flag.into(),
                    "-c".into(),
                    ir_path.display().to_string(),
                    "-o".into(),
                    obj_path.display().to_string(),
                ];
                compile_cmd
                    .arg(&target_flag)
                    .arg("-x")
                    .arg("ir")
                    .arg(opt_flag)
                    .arg("-c")
                    .arg(&ir_path)
                    .arg("-o")
                    .arg(&obj_path);
                run_checked_command(compile_cmd, "clang", &clang)?;
                commands.push(compile_argv);
                link_windows_lld(
                    &clang,
                    &target_flag,
                    opt_flag,
                    vec![obj_path],
                    options,
                    windows,
                    &mut commands,
                )?;
            } else {
                let mut cmd = Command::new(&clang);
                let mut argv = vec![
                    clang.display().to_string(),
                    target_flag.clone(),
                    "-x".into(),
                    "ir".into(),
                    opt_flag.into(),
                    ir_path.display().to_string(),
                ];
                cmd.arg(&target_flag)
                    .arg("-x")
                    .arg("ir")
                    .arg(opt_flag)
                    .arg(&ir_path);
                for c_src in &options.extra_c_sources {
                    cmd.arg("-x").arg("c").arg(c_src);
                    argv.push("-x".into());
                    argv.push("c".into());
                    argv.push(c_src.display().to_string());
                }
                cmd.arg("-o").arg(&options.output);
                argv.push("-o".into());
                argv.push(options.output.display().to_string());
                if is_windows {
                    cmd.arg("-x")
                        .arg("none")
                        .arg("-llegacy_stdio_definitions")
                        .arg("-lws2_32");
                    argv.push("-x".into());
                    argv.push("none".into());
                    argv.push("-llegacy_stdio_definitions".into());
                    argv.push("-lws2_32".into());
                }
                run_checked_command(cmd, "clang", &clang)?;
                commands.push(argv);
            }
            Ok(BodyBuildOutput {
                ir,
                ir_path,
                commands,
                object_reports: Vec::new(),
                actual_units: 1,
                cache_hits: 0,
                cache_misses: 0,
                cache_evicted_entries: 0,
                cache_evicted_bytes: 0,
                cache_evict_ns: 0,
            })
        }
    }
}

/// Convenience helper for checking that `clang` is reachable.
pub fn default_clang_path() -> &'static Path {
    Path::new("clang")
}
