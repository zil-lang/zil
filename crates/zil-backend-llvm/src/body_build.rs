//! Native build driver for `zil_check::VerifiedModule` using LLVM 23 clang at O0/O2.

use crate::body_emit::{BodyEmitError, emit_verified_body};
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
}

#[derive(Clone, Debug)]
pub struct BodyBuildOutput {
    pub ir: String,
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
/// For `NativeBackend::LlvmApi` (when built with feature `llvm-api`), compiles
/// the IR in-process through `zil-llvm-api` to a native `.o` object file and
/// invokes `clang` strictly for linking the `.o` into the final executable.
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
                let api_opt = match options.optimization {
                    Optimization::O0 => zil_llvm_api::Optimization::O0,
                    Optimization::O2 => zil_llvm_api::Optimization::O2,
                };
                let config = zil_llvm_api::Config {
                    triple: host_triple(),
                    optimization: api_opt,
                };
                let object =
                    zil_llvm_api::compile(&ir, &config, &|| false).map_err(|e| BodyBuildError {
                        message: format!("zil-llvm-api failed: {e}"),
                    })?;
                let obj_path = PathBuf::from(format!("{}.o", options.output.display()));
                fs::write(&obj_path, &object.bytes).map_err(|e| BodyBuildError {
                    message: format!("failed to write object file: {e}"),
                })?;

                let mut cmd = Command::new(&clang);
                let mut argv = vec![clang.display().to_string(), obj_path.display().to_string()];
                cmd.arg(&obj_path);
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
                    commands: vec![argv],
                    object_reports: vec![receipt_to_json(&object.receipt)],
                    actual_units: 1,
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

            let mut cmd = Command::new(&clang);
            let mut argv = vec![
                clang.display().to_string(),
                opt_flag.into(),
                "-Wno-override-module".into(),
                ir_path.display().to_string(),
            ];
            cmd.arg(opt_flag).arg("-Wno-override-module").arg(&ir_path);
            for c_src in &options.extra_c_sources {
                cmd.arg(c_src);
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
