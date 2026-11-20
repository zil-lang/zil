//! Native build driver for `zil_check::VerifiedModule` using LLVM 23 clang at O0/O2.

use crate::body_emit::{BodyEmitError, emit_verified_body, emit_verified_body_units};
use crate::{NativeBackend, Optimization};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zil_check::VerifiedModule;

#[derive(Clone, Debug)]
pub struct BodyBuildOptions {
    pub output: PathBuf,
    pub optimization: Optimization,
    pub backend: NativeBackend,
    pub clang: Option<PathBuf>,
    pub extra_c_sources: Vec<PathBuf>,
    pub emit_c_main: bool,
    pub codegen_units: usize,
    pub codegen_jobs: usize,
}

#[derive(Clone, Debug)]
pub struct BodyBuildOutput {
    pub ir: String,
    pub ir_path: PathBuf,
    pub commands: Vec<Vec<String>>,
    pub object_reports: Vec<String>,
    pub actual_units: usize,
}

#[derive(Clone, Debug)]
pub struct BodyBuildError {
    pub message: String,
}

impl From<BodyEmitError> for BodyBuildError {
    fn from(e: BodyEmitError) -> Self {
        Self { message: e.message }
    }
}

#[cfg(feature = "llvm-api")]
fn host_triple() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-gnu"
    } else {
        "x86_64-unknown-linux-gnu"
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

/// Emits LLVM 23 IR from `verified` and compiles/links a native executable.
///
/// For `NativeBackend::LlvmApi` (when built with feature `llvm-api`), partitions
/// the module into `actual_units = codegen_units.clamp(1, fn_count.max(1))`
/// independent IR units, compiles each unit in parallel (up to `codegen_jobs`)
/// through `zil-llvm-api` to its own `.o` object file, and invokes `clang`
/// strictly for linking the produced `.o` files into the final executable.
pub fn build_verified_body(
    verified: &VerifiedModule,
    options: &BodyBuildOptions,
) -> Result<BodyBuildOutput, BodyBuildError> {
    let ir = emit_verified_body(verified, options.emit_c_main)?;
    let ir_path = PathBuf::from(format!("{}.ll", options.output.display()));
    if let Some(parent) = ir_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|e| BodyBuildError {
            message: format!("failed to create output directory: {e}"),
        })?;
    }
    fs::write(&ir_path, &ir).map_err(|e| BodyBuildError {
        message: format!("failed to write LLVM IR: {e}"),
    })?;

    let clang = options
        .clang
        .clone()
        .unwrap_or_else(|| PathBuf::from("clang"));

    match options.backend {
        NativeBackend::LlvmApi => {
            #[cfg(feature = "llvm-api")]
            {
                let requested = options.codegen_units.max(1);
                let unit_irs = if requested == 1 {
                    vec![ir.clone()]
                } else {
                    emit_verified_body_units(verified, options.emit_c_main, requested)?
                };
                let actual_units = unit_irs.len();
                let api_opt = match options.optimization {
                    Optimization::O0 => zil_llvm_api::Optimization::O0,
                    Optimization::O2 => zil_llvm_api::Optimization::O2,
                };
                let triple = host_triple();
                let jobs = options.codegen_jobs.clamp(1, actual_units);

                let results: Vec<std::sync::Mutex<Option<Result<zil_llvm_api::Object, String>>>> =
                    (0..actual_units)
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
                                let compiled =
                                    zil_llvm_api::compile(&unit_irs[idx], &config, &|| false)
                                        .map_err(|e| {
                                            format!("zil-llvm-api unit {idx} failed: {e}")
                                        });
                                if let Ok(mut slot) = results[idx].lock() {
                                    *slot = Some(compiled);
                                }
                            }
                        });
                    }
                });

                let mut obj_paths = Vec::with_capacity(actual_units);
                let mut object_reports = Vec::with_capacity(actual_units);
                for (idx, slot) in results.into_iter().enumerate() {
                    let object = slot
                        .into_inner()
                        .ok()
                        .flatten()
                        .ok_or_else(|| BodyBuildError {
                            message: format!("missing compilation result for unit {idx}"),
                        })?
                        .map_err(|message| BodyBuildError { message })?;
                    let obj_path = if actual_units == 1 {
                        PathBuf::from(format!("{}.o", options.output.display()))
                    } else {
                        PathBuf::from(format!("{}.u{idx}.o", options.output.display()))
                    };
                    fs::write(&obj_path, &object.bytes).map_err(|e| BodyBuildError {
                        message: format!("failed to write object file {}: {e}", obj_path.display()),
                    })?;
                    obj_paths.push(obj_path);
                    object_reports.push(receipt_to_json(&object.receipt));
                }

                let target_flag = format!("--target={triple}");
                let opt_flag = match options.optimization {
                    Optimization::O0 => "-O0",
                    Optimization::O2 => "-O2",
                };
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

                let out = cmd.output().map_err(|e| BodyBuildError {
                    message: format!("failed to invoke linker ({}): {e}", clang.display()),
                })?;
                if !out.status.success() {
                    return Err(BodyBuildError {
                        message: format!(
                            "linker failed ({}):\nstdout: {}\nstderr: {}",
                            out.status,
                            String::from_utf8_lossy(&out.stdout),
                            String::from_utf8_lossy(&out.stderr)
                        ),
                    });
                }
                Ok(BodyBuildOutput {
                    ir,
                    ir_path,
                    commands: vec![argv],
                    object_reports,
                    actual_units,
                })
            }
            #[cfg(not(feature = "llvm-api"))]
            {
                Err(BodyBuildError {
                    message: "llvm-api backend is not enabled in this build".into(),
                })
            }
        }
        NativeBackend::Clang => {
            let opt_flag = match options.optimization {
                Optimization::O0 => "-O0",
                Optimization::O2 => "-O2",
            };
            let target_flag = if cfg!(target_arch = "aarch64") {
                "--target=aarch64-unknown-linux-gnu"
            } else {
                "--target=x86_64-unknown-linux-gnu"
            };

            let mut cmd = Command::new(&clang);
            let mut argv = vec![
                clang.display().to_string(),
                target_flag.into(),
                "-x".into(),
                "ir".into(),
                opt_flag.into(),
                ir_path.display().to_string(),
            ];
            cmd.arg(target_flag)
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

            let out = cmd.output().map_err(|e| BodyBuildError {
                message: format!("failed to invoke clang ({}): {e}", clang.display()),
            })?;
            if !out.status.success() {
                return Err(BodyBuildError {
                    message: format!(
                        "clang failed ({}):\nstdout: {}\nstderr: {}",
                        out.status,
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    ),
                });
            }
            Ok(BodyBuildOutput {
                ir,
                ir_path,
                commands: vec![argv],
                object_reports: Vec::new(),
                actual_units: 1,
            })
        }
    }
}

/// Convenience helper for checking that `clang` is reachable.
pub fn default_clang_path() -> &'static Path {
    Path::new("clang")
}
