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
    /// Best-effort soft on-disk byte budget for `object_cache` (`None` = unlimited).
    /// When exceeded, oldest inactive cached entries (and paired `.meta` sidecars)
    /// by mtime are evicted down to an 80% watermark. Active objects needed by the
    /// current build are never evicted, so cache size may temporarily exceed the
    /// budget when the active working set itself is larger than the budget.
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
    pub emit_ns: u64,
    pub link_ns: u64,
}

/// Locates the bundled LLVM `ld.lld` sitting next to `clang` (without
/// canonicalizing the symlink so multicall `lld` keeps `argv[0] = ld.lld`).
fn resolve_sibling_lld(clang: &Path) -> Option<PathBuf> {
    if let Some(parent) = clang.parent()
        && !parent.as_os_str().is_empty()
    {
        let candidate = parent.join("ld.lld");
        if candidate.is_file() {
            return Some(candidate);
        }
        return None;
    }
    let path_env = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_env) {
        if dir.join(clang).is_file() {
            let candidate = dir.join("ld.lld");
            if candidate.is_file() {
                return Some(candidate);
            }
            return None;
        }
    }
    None
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
static CACHE_STAGE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[cfg(feature = "llvm-api")]
struct StageDirGuard {
    path: PathBuf,
}

#[cfg(feature = "llvm-api")]
impl Drop for StageDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
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
fn cache_hit_receipt_json(triple: &str, pipeline: &str) -> String {
    format!(
        "{{\"backend\":\"llvm-api\",\"llvm_version\":{},\"triple\":{},\"cpu\":\"generic\",\"features\":\"\",\"data_layout\":\"\",\"pipeline\":{},\"relocation\":\"pic\",\"code_model\":\"default\",\"verify_before_after\":true,\"input_kind\":\"object-cache-hit\",\"preflight_ns\":0,\"builder_ns\":0,\"parse_ns\":0,\"optimize_ns\":0,\"emit_ns\":0,\"total_ns\":0}}",
        zil_base::json_string(zil_llvm_api::LLVM_VERSION),
        zil_base::json_string(triple),
        zil_base::json_string(pipeline),
    )
}

#[cfg(feature = "llvm-api")]
fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = zil_base::FingerprintBuilder::new();
    h.update(bytes);
    h.digest().to_hex()
}

/// Explicit backend codegen ABI version bound into object-cache keys and `.meta`
/// sidecars. Bump whenever `body_structured`, `zil-llvm-api` builder lowering, or
/// machine object emission changes in a way that can alter output bytes for identical IR.
pub const OBJECT_CACHE_CODEGEN_ABI: u32 = 1;

#[cfg(feature = "llvm-api")]
fn unit_cache_key(triple: &str, obj_ext: &str, pipeline: &str, unit_ir: &str) -> String {
    let mut h = zil_base::FingerprintBuilder::new();
    h.update(b"zil-obj-cache-v4\0");
    h.update(OBJECT_CACHE_CODEGEN_ABI.to_le_bytes().as_slice());
    h.update(b"\0");
    h.update(env!("CARGO_PKG_VERSION").as_bytes());
    h.update(b"\0");
    h.update(zil_llvm_api::LLVM_VERSION.as_bytes());
    h.update(b"\0");
    h.update(triple.as_bytes());
    h.update(b"\0");
    h.update(obj_ext.as_bytes());
    h.update(b"\0");
    h.update(pipeline.as_bytes());
    h.update(b"\0cpu=generic|features=|reloc=pic|cm=default|verify=true\0");
    h.update(unit_ir.as_bytes());
    h.digest().to_hex()
}

#[cfg(feature = "llvm-api")]
fn cache_meta_path(obj_cache_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.meta", obj_cache_path.display()))
}

#[cfg(feature = "llvm-api")]
fn format_cache_meta(
    key: &str,
    len: usize,
    obj_sha256: &str,
    llvm_version: &str,
    triple: &str,
    pipeline: &str,
) -> String {
    format!(
        "v4\nabi={OBJECT_CACHE_CODEGEN_ABI}\nkey={key}\nlen={len}\nsha256={obj_sha256}\nllvm={llvm_version}\ntriple={triple}\npipeline={pipeline}\n"
    )
}

#[cfg(feature = "llvm-api")]
fn has_valid_object_magic(bytes: &[u8], obj_ext: &str) -> bool {
    if obj_ext == "obj" {
        // x86_64 Windows COFF header: IMAGE_FILE_MACHINE_AMD64 (0x8664 little-endian)
        bytes.len() >= 20 && bytes[0] == 0x64 && bytes[1] == 0x86
    } else {
        // ELF magic header: 0x7f 'E' 'L' 'F'
        bytes.len() >= 16 && &bytes[..4] == b"\x7fELF"
    }
}

#[cfg(feature = "llvm-api")]
fn verify_cache_integrity(
    meta_text: &str,
    staged_bytes: &[u8],
    obj_ext: &str,
    expected_key: &str,
    expected_llvm: &str,
    expected_triple: &str,
    expected_pipeline: &str,
) -> bool {
    let mut lines = meta_text.lines();
    if lines.next() != Some("v4") {
        return false;
    }
    let mut abi = None;
    let mut key = None;
    let mut len = None;
    let mut digest = None;
    let mut llvm = None;
    let mut triple = None;
    let mut pipeline = None;
    for line in lines {
        if let Some((k, v)) = line.split_once('=') {
            match k {
                "abi" => abi = v.parse::<u32>().ok(),
                "key" => key = Some(v),
                "len" => len = v.parse::<usize>().ok(),
                "sha256" => digest = Some(v),
                "llvm" => llvm = Some(v),
                "triple" => triple = Some(v),
                "pipeline" => pipeline = Some(v),
                _ => {}
            }
        }
    }
    if abi != Some(OBJECT_CACHE_CODEGEN_ABI)
        || key != Some(expected_key)
        || llvm != Some(expected_llvm)
        || triple != Some(expected_triple)
        || pipeline != Some(expected_pipeline)
    {
        return false;
    }
    let Some(expected_len) = len else {
        return false;
    };
    if expected_len == 0 || staged_bytes.len() != expected_len {
        return false;
    }
    if !has_valid_object_magic(staged_bytes, obj_ext) {
        return false;
    }
    digest == Some(sha256_hex(staged_bytes).as_str())
}

#[cfg(feature = "llvm-api")]
fn stage_object_snapshot(src: &Path, dst: &Path) -> std::io::Result<()> {
    match fs::hard_link(src, dst) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(err),
        Err(_) => fs::copy(src, dst).map(|_| ()),
    }
}

#[cfg(feature = "llvm-api")]
fn touch_cache_mtime(path: &Path) {
    if let Ok(file) = fs::OpenOptions::new().append(true).open(path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
}

#[cfg(feature = "llvm-api")]
fn should_scan_cache_budget(cache_dir: &Path, max_bytes: u64, cache_misses: usize) -> bool {
    let budget_file = cache_dir.join(".budget");
    let prev = fs::read_to_string(&budget_file)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok());
    let lowered = prev.is_none_or(|p| max_bytes < p);
    if lowered || prev != Some(max_bytes) {
        let _ = fs::write(&budget_file, max_bytes.to_string());
    }
    cache_misses > 0 || lowered
}

#[cfg(feature = "llvm-api")]
fn enforce_object_cache_budget(
    cache_dir: &Path,
    max_bytes: u64,
    obj_ext: &str,
    active_paths: &std::collections::HashSet<PathBuf>,
) -> (usize, u64, u64) {
    let start = std::time::Instant::now();
    let obj_suffix = format!(".{obj_ext}");
    let meta_suffix = format!(".{obj_ext}.meta");
    let Ok(read_dir) = fs::read_dir(cache_dir) else {
        return (0, 0, start.elapsed().as_nanos() as u64);
    };
    let now = std::time::SystemTime::now();
    let mut obj_entries: Vec<(String, std::time::SystemTime, u64, PathBuf)> = Vec::new();
    let mut meta_map: std::collections::HashMap<String, (u64, std::time::SystemTime, PathBuf)> =
        std::collections::HashMap::new();
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // Never disturb per-build staging directories (`.stage-*`) or `.budget`.
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        // Opportunistically sweep stale abandoned temp files (> 120s old).
        if name.contains(".tmp.") {
            if now.duration_since(mtime).is_ok_and(|d| d.as_secs() > 120) {
                let _ = fs::remove_file(&path);
            }
            continue;
        }
        if let Some(stem) = name.strip_suffix(&meta_suffix) {
            meta_map.insert(stem.to_owned(), (meta.len(), mtime, path));
        } else if let Some(stem) = name.strip_suffix(&obj_suffix) {
            obj_entries.push((stem.to_owned(), mtime, meta.len(), path));
        }
    }

    let mut total_bytes = 0u64;
    let mut candidates: Vec<(std::time::SystemTime, u64, PathBuf, Option<PathBuf>)> =
        Vec::with_capacity(obj_entries.len());
    for (stem, mtime, obj_sz, obj_path) in obj_entries {
        let (meta_sz, meta_path) = match meta_map.remove(&stem) {
            Some((sz, _, mp)) => (sz, Some(mp)),
            None => (0, None),
        };
        let entry_sz = obj_sz.saturating_add(meta_sz);
        total_bytes = total_bytes.saturating_add(entry_sz);
        if !active_paths.contains(&obj_path) {
            candidates.push((mtime, entry_sz, obj_path, meta_path));
        }
    }
    // Sweep orphan `.meta` files left in `meta_map` only when older than 120s.
    for (_, (_, mtime, mp)) in meta_map {
        if now.duration_since(mtime).is_ok_and(|d| d.as_secs() > 120) {
            let _ = fs::remove_file(mp);
        }
    }
    if total_bytes <= max_bytes {
        return (0, 0, start.elapsed().as_nanos() as u64);
    }
    let watermark = (max_bytes.saturating_mul(4)) / 5;
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
    let mut evicted_entries = 0usize;
    let mut evicted_bytes = 0u64;
    for (_, entry_sz, obj_path, meta_path) in candidates {
        if total_bytes <= watermark {
            break;
        }
        if let Some(mp) = meta_path {
            let _ = fs::remove_file(mp);
        }
        if fs::remove_file(&obj_path).is_ok() {
            total_bytes = total_bytes.saturating_sub(entry_sz);
            evicted_entries += 1;
            evicted_bytes = evicted_bytes.saturating_add(entry_sz);
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
        .or_else(|| {
            std::env::var_os("ZIL_CLANG")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
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
                let emit_started = std::time::Instant::now();
                let unit_irs = emit_verified_body_units_with_entry_for_target(
                    verified,
                    options.emit_c_main,
                    requested,
                    entry,
                    triple,
                )?;
                let emit_ns = emit_started.elapsed().as_nanos() as u64;
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
                let pid = std::process::id();
                let stage_seq = CACHE_STAGE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let stage_guard = if let Some(dir) = cache_dir {
                    fs::create_dir_all(dir).map_err(|e| {
                        BodyBuildError::new(format!(
                            "failed to create object cache directory {}: {e}",
                            dir.display()
                        ))
                    })?;
                    let stage_dir = dir.join(format!(".stage-{pid}-{stage_seq}"));
                    fs::create_dir_all(&stage_dir).map_err(|e| {
                        BodyBuildError::new(format!(
                            "failed to create object cache staging directory {}: {e}",
                            stage_dir.display()
                        ))
                    })?;
                    Some(StageDirGuard { path: stage_dir })
                } else {
                    None
                };
                let stage_dir_ref = stage_guard.as_ref().map(|g| g.path.as_path());

                let api_opt = match options.optimization {
                    Optimization::O0 => zil_llvm_api::Optimization::O0,
                    Optimization::O2 => zil_llvm_api::Optimization::O2,
                };
                let pipeline_id = zil_llvm_api::pipeline_identity(
                    api_opt,
                    zil_llvm_api::FunctionMerging::Disabled,
                );
                let jobs = options.codegen_jobs.clamp(1, actual_units);

                struct UnitCompiled {
                    /// Canonical cache entry path in `cache_dir` (for active-set protection during LRU).
                    cache_entry_path: Option<PathBuf>,
                    /// Private hard-linked/staged object path safe from concurrent cross-process eviction.
                    staged_path: Option<PathBuf>,
                    /// In-memory bytes when `cache_dir` is disabled (or staging fallback).
                    bytes: Vec<u8>,
                    report: String,
                    cache_hit: bool,
                }
                let results: Vec<std::sync::Mutex<Option<Result<UnitCompiled, String>>>> = (0
                    ..actual_units)
                    .map(|_| std::sync::Mutex::new(None))
                    .collect();
                let next_idx = std::sync::atomic::AtomicUsize::new(0);

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
                                let cache_info = cache_dir.map(|dir| {
                                    let key = unit_cache_key(
                                        triple,
                                        obj_ext,
                                        pipeline_id,
                                        &unit_irs[idx],
                                    );
                                    let cp = dir.join(format!("{key}.{obj_ext}"));
                                    let mp = cache_meta_path(&cp);
                                    (key, cp, mp)
                                });
                                let staged_target =
                                    stage_dir_ref.map(|sd| sd.join(format!("u{idx}.{obj_ext}")));

                                // Pin a private hard-link snapshot BEFORE verifying integrity so a
                                // concurrent process running LRU eviction cannot unlink our link input.
                                let mut hit_unit: Option<UnitCompiled> = None;
                                if let (Some((key, cp, mp)), Some(staged)) =
                                    (cache_info.as_ref(), staged_target.as_ref())
                                {
                                    match stage_object_snapshot(cp, staged) {
                                        Ok(()) => {
                                            let verified_ok = fs::read_to_string(mp)
                                                .ok()
                                                .zip(fs::read(staged).ok())
                                                .is_some_and(|(meta_txt, bytes)| {
                                                    verify_cache_integrity(
                                                        &meta_txt,
                                                        &bytes,
                                                        obj_ext,
                                                        key,
                                                        zil_llvm_api::LLVM_VERSION,
                                                        triple,
                                                        pipeline_id,
                                                    )
                                                });
                                            if verified_ok {
                                                touch_cache_mtime(cp);
                                                hit_unit = Some(UnitCompiled {
                                                    cache_entry_path: Some(cp.clone()),
                                                    staged_path: Some(staged.clone()),
                                                    bytes: Vec::new(),
                                                    report: cache_hit_receipt_json(
                                                        triple,
                                                        pipeline_id,
                                                    ),
                                                    cache_hit: true,
                                                });
                                            } else {
                                                // Corrupt or mismatched cache entry: drop staged copy and
                                                // best-effort purge corrupt entry, then recompile cleanly.
                                                let _ = fs::remove_file(staged);
                                                let _ = fs::remove_file(mp);
                                                let _ = fs::remove_file(cp);
                                            }
                                        }
                                        Err(_) => {
                                            // Not present or concurrently evicted; compile fresh.
                                        }
                                    }
                                }

                                let compiled = if let Some(hit) = hit_unit {
                                    Ok(hit)
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
                                        let mut cache_entry_path = None;
                                        let mut staged_path = None;
                                        if let (Some((key, cp, mp)), Some(staged)) =
                                            (cache_info.as_ref(), staged_target.as_ref())
                                        {
                                            let obj_sha = sha256_hex(&object.bytes);
                                            let meta_text = format_cache_meta(
                                                key,
                                                object.bytes.len(),
                                                &obj_sha,
                                                &object.receipt.llvm_version,
                                                &object.receipt.triple,
                                                &object.receipt.pipeline,
                                            );
                                            let tmp_obj = cp.with_extension(format!(
                                                "{obj_ext}.tmp.{pid}.{stage_seq}.{idx}"
                                            ));
                                            let tmp_meta = PathBuf::from(format!(
                                                "{}.tmp.{pid}.{stage_seq}.{idx}",
                                                mp.display()
                                            ));
                                            if fs::write(&tmp_obj, &object.bytes).is_ok() {
                                                // Pin our private staged hard-link BEFORE publishing into
                                                // cache_dir so neither concurrent eviction nor Windows rename
                                                // contention can ever disrupt this build's link input.
                                                if stage_object_snapshot(&tmp_obj, staged).is_ok() {
                                                    staged_path = Some(staged.clone());
                                                }
                                                // Publish `.meta` BEFORE `.o` so concurrent readers never
                                                // observe an object file whose paired `.meta` is missing.
                                                let meta_published =
                                                    fs::write(&tmp_meta, &meta_text).is_ok()
                                                        && fs::rename(&tmp_meta, mp).is_ok();
                                                if !meta_published {
                                                    let _ = fs::remove_file(&tmp_meta);
                                                }
                                                if meta_published
                                                    && fs::rename(&tmp_obj, cp).is_ok()
                                                {
                                                    cache_entry_path = Some(cp.clone());
                                                } else {
                                                    let _ = fs::remove_file(&tmp_obj);
                                                }
                                            }
                                            if staged_path.is_none()
                                                && fs::write(staged, &object.bytes).is_ok()
                                            {
                                                staged_path = Some(staged.clone());
                                            }
                                        }
                                        let bytes = if staged_path.is_some() {
                                            Vec::new()
                                        } else {
                                            object.bytes
                                        };
                                        UnitCompiled {
                                            cache_entry_path,
                                            staged_path,
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
                    if let Some(cp) = unit.cache_entry_path {
                        active_cache_paths.insert(cp);
                    }
                    if let Some(sp) = unit.staged_path {
                        obj_paths.push(sp);
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
                let link_started = std::time::Instant::now();
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
                    if !is_windows
                        && !triple.contains("apple")
                        && let Some(lld_path) = resolve_sibling_lld(&clang)
                    {
                        let ld_flag = format!("--ld-path={}", lld_path.display());
                        cmd.arg(&ld_flag);
                        argv.push(ld_flag);
                    }
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
                let link_ns = link_started.elapsed().as_nanos() as u64;
                // Release our private hard-link staging directory immediately after linking.
                drop(stage_guard);
                for tmp_obj in temp_obj_paths {
                    let _ = fs::remove_file(tmp_obj);
                }
                let (cache_evicted_entries, cache_evicted_bytes, cache_evict_ns) = if let Some(dir) =
                    cache_dir
                    && let Some(max_bytes) = options.object_cache_max_bytes
                    && max_bytes > 0
                    && should_scan_cache_budget(dir, max_bytes, cache_misses)
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
                    emit_ns,
                    link_ns,
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
            let emit_started = std::time::Instant::now();
            let ir = emit_verified_body_units_with_entry_for_target(
                verified,
                options.emit_c_main,
                1,
                entry,
                triple,
            )?
            .pop()
            .unwrap_or_default();
            let emit_ns = emit_started.elapsed().as_nanos() as u64;
            fs::write(&ir_path, &ir)
                .map_err(|e| BodyBuildError::new(format!("failed to write LLVM IR: {e}")))?;
            let mut commands = Vec::new();
            let link_started = std::time::Instant::now();
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
                if !is_windows
                    && !triple.contains("apple")
                    && let Some(lld_path) = resolve_sibling_lld(&clang)
                {
                    let ld_flag = format!("--ld-path={}", lld_path.display());
                    cmd.arg(&ld_flag);
                    argv.push(ld_flag);
                }
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
            let link_ns = link_started.elapsed().as_nanos() as u64;
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
                emit_ns,
                link_ns,
            })
        }
    }
}

/// Convenience helper for checking that `clang` is reachable.
pub fn default_clang_path() -> &'static Path {
    Path::new("clang")
}
