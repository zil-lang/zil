//! Native build driver for `zil_check::VerifiedModule` using LLVM 23 clang at O0/O2.

use crate::body_emit::{BodyEmitError, emit_verified_body};
use crate::build::Optimization;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use zil_check::VerifiedModule;

#[derive(Clone, Debug)]
pub struct BodyBuildOptions {
    pub output: PathBuf,
    pub optimization: Optimization,
    pub clang: Option<PathBuf>,
    pub extra_c_sources: Vec<PathBuf>,
    pub emit_c_main: bool,
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

/// Emits LLVM 23 IR from `verified` and compiles/links a native executable with
/// `clang` at `options.optimization` (`-O0` or `-O2`).
pub fn build_verified_body(
    verified: &VerifiedModule,
    options: &BodyBuildOptions,
) -> Result<String, BodyBuildError> {
    let ir = emit_verified_body(verified, options.emit_c_main)?;
    let ir_path = options.output.with_extension("ll");
    if let Some(parent) = ir_path.parent() {
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
    let opt_flag = match options.optimization {
        Optimization::O0 => "-O0",
        Optimization::O2 => "-O2",
    };

    let mut cmd = Command::new(&clang);
    cmd.arg(opt_flag).arg("-Wno-override-module").arg(&ir_path);
    for c_src in &options.extra_c_sources {
        cmd.arg(c_src);
    }
    cmd.arg("-o").arg(&options.output);

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
    Ok(ir)
}

/// Convenience helper for checking that `clang` is reachable.
pub fn default_clang_path() -> &'static Path {
    Path::new("clang")
}
