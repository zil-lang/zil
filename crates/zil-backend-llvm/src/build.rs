use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use zil_base::{CompileResult, Diagnostic, Phase, ResultCategory};
use zil_ir::LirModule;
use zil_verify::VerifiedModule;

use crate::{EmitOptions, EntryPoint, emit_with_options};

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildOptions {
    pub clang: PathBuf,
    pub output: PathBuf,
    pub optimization: Optimization,
    pub emit: EmitOptions,
    pub windows: Option<WindowsLinkOptions>,
}

impl BuildOptions {
    pub fn new(output: impl Into<PathBuf>) -> Self {
        Self {
            clang: PathBuf::from("clang"),
            output: output.into(),
            optimization: Optimization::O0,
            windows: None,
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
    if options.emit.entry == EntryPoint::None {
        return Err(vec![Diagnostic::error(
            Phase::Link,
            "E-LINK-ENTRY",
            None,
            "native build requires an explicit executable entry wrapper",
        )]);
    }
    let llvm = emit_with_options(verified, lir, &options.emit)?;
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
    let recorded_command = run_command(&mut command, "E-LLVM-COMPILE")?;
    let mut commands = vec![recorded_command.clone()];
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
        .arg(&object_path)
        .args(imports);
        commands.push(run_command(&mut link, "E-WIN64-LINK")?);
    }
    if !executable_path.is_file() {
        return Err(infra(
            "E-LINK-MISSING-OUTPUT",
            "LLVM clang reported success without producing an executable",
        ));
    }
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
    })
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
