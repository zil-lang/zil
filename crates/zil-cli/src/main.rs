#![forbid(unsafe_code)]
mod qc;
use std::env;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zil_base::{Diagnostic, Phase, ResultCategory, TargetProfile, json_string};
use zil_driver::{
    CancellationToken, CompileOptions, CompileReport, ExecutionProfile, NativeBackend,
    Optimization, Stage, WindowsLinkOptions, compile_path,
};

// The executable owns the default policy; library callers choose explicitly.
const DEFAULT_BACKEND: NativeBackend = if cfg!(feature = "llvm-api") {
    NativeBackend::LlvmApi
} else {
    NativeBackend::Clang
};

const HELP: &str = "ZIL 0.1.0 — compiler draft\n\nUsage:\n  zil check FILE [--stop-after parse|core|types|analyze|verify] [--json]\n  zil build FILE [-o OUTPUT] [--entry NAME] [--emit-dir DIR] [--json]\n  zil run FILE [--entry NAME] [--emit-dir DIR] [--json]\n  zil qc [--tool PATH] COMMAND [OPTIONS]\n  zil version\n\nOptions:\n  --cache-dir DIR           Store unverified source candidates\n  --object-cache DIR        Trusted local LLVM API object cache (opt-in)\n  --backend clang|llvm-api  Object generation strategy (standard build: llvm-api)\n  --clang PATH              LLVM clang executable (default: ZIL_CLANG or clang)\n  --emit-dir DIR            Write actual phase artifacts\n  --target native|win64     Select the checked target ABI\n  --windows-lib PATH        Explicit Win64 import archive (repeatable)\n  --linker PATH             Win64 lld-link executable\n  --opt O0|O2               Native LLVM optimization level\n  --codegen-units N         Maximum native units, 1..64 (default: 1)\n  --codegen-jobs N          Concurrent native compiles, 1..64 (default: 1)\n  --compile-timeout-ms N    Cooperative deadline checked between compiler phases\n  --profile native|managed  Managed is explicitly Unsupported\n\nNative run requires an explicit main; it adds no implicit module execution or GC.\nUnsupported language features return Unsupported; they are never silently accepted.\n";
fn usage(message: &str, json: bool) -> ExitCode {
    let diagnostic = Diagnostic::error(Phase::Driver, "E-CLI-ARGUMENT", None, message);
    if json {
        println!(
            "{{\"status\":\"SafeReject\",\"phase\":\"driver\",\"diagnostics\":[{}],\"artifacts\":{{}}}}",
            diagnostic.to_json()
        );
    } else {
        eprintln!("{message}\n\n{HELP}");
    }
    ExitCode::from(64)
}
fn main() -> ExitCode {
    let raw = env::args_os().skip(1).collect::<Vec<_>>();
    let json = raw.iter().any(|argument| argument == "--json");
    let args = match raw
        .into_iter()
        .map(|argument| argument.into_string())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(args) => args,
        Err(_) => return usage("command arguments must be valid UTF-8", json),
    };
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if args[0] == "qc" {
        return qc::run(&args[1..]);
    }
    if matches!(args[0].as_str(), "version" | "--version") {
        println!(
            "zil {} ({}; Rust 2024; compiler draft; default backend: {})",
            env!("CARGO_PKG_VERSION"),
            zil_base::RULE_VERSION,
            DEFAULT_BACKEND.name()
        );
        return ExitCode::SUCCESS;
    }
    let action = &args[0];
    if !matches!(action.as_str(), "check" | "build" | "run") {
        return usage("unknown command", json);
    }
    let mut options = CompileOptions {
        backend: DEFAULT_BACKEND,
        stop_after: if action == "check" {
            Stage::Verify
        } else {
            Stage::Build
        },
        ..CompileOptions::default()
    };
    let mut input = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {}
            "-o"
            | "--output"
            | "--entry"
            | "--emit-dir"
            | "--stop-after"
            | "--cache-dir"
            | "--object-cache"
            | "--backend"
            | "--clang"
            | "--target"
            | "--windows-lib"
            | "--linker"
            | "--opt"
            | "--codegen-units"
            | "--codegen-jobs"
            | "--profile"
            | "--compile-timeout-ms" => {
                if index + 1 == args.len() {
                    return usage("option requires a value", json);
                }
                let flag = &args[index];
                index += 1;
                let value = &args[index];
                match flag.as_str() {
                    "-o" | "--output" => options.output = Some(PathBuf::from(value)),
                    "--entry" => options.entry = Some(value.clone()),
                    "--emit-dir" => options.emit_dir = Some(PathBuf::from(value)),
                    "--cache-dir" => options.cache_dir = Some(PathBuf::from(value)),
                    "--object-cache" => options.object_cache = Some(PathBuf::from(value)),
                    "--backend" => {
                        options.backend = match value.as_str() {
                            "clang" => NativeBackend::Clang,
                            "llvm-api" => NativeBackend::LlvmApi,
                            _ => return usage("backend must be clang or llvm-api", json),
                        }
                    }
                    "--clang" => options.clang = Some(PathBuf::from(value)),
                    "--target" => {
                        options.target = match value.as_str() {
                            "native" => TargetProfile::host_llvm(),
                            "win64" | "x86_64-pc-windows-msvc" => TargetProfile::windows_x64_llvm(),
                            "x86_64-unknown-linux-gnu" => {
                                let mut target = TargetProfile::host_llvm();
                                target.triple = value.clone();
                                target.pointer_width = 64;
                                target.endian = zil_base::Endian::Little;
                                target
                            }
                            _ => return usage("unsupported target profile", json),
                        };
                    }
                    "--windows-lib" => options
                        .windows
                        .get_or_insert_with(WindowsLinkOptions::default)
                        .import_libraries
                        .push(PathBuf::from(value)),
                    "--linker" => {
                        options
                            .windows
                            .get_or_insert_with(WindowsLinkOptions::default)
                            .linker = PathBuf::from(value)
                    }
                    "--codegen-units" | "--codegen-jobs" => {
                        let Ok(count @ 1..=64) = value.parse::<usize>() else {
                            return usage("codegen units and jobs must be in 1..=64", json);
                        };
                        if flag == "--codegen-units" {
                            options.codegen.units = count;
                        } else {
                            options.codegen.jobs = count;
                        }
                    }
                    "--opt" => {
                        options.optimization = match value.as_str() {
                            "O0" => Optimization::O0,
                            "O2" => Optimization::O2,
                            _ => return usage("optimization must be O0 or O2", json),
                        }
                    }
                    "--profile" => {
                        options.profile = match value.as_str() {
                            "native" => ExecutionProfile::Native,
                            "managed" => ExecutionProfile::Managed,
                            _ => return usage("unknown execution profile", json),
                        }
                    }
                    "--compile-timeout-ms" => {
                        let millis = match value.parse::<u64>() {
                            Ok(value) => value,
                            Err(_) => {
                                return usage("compile timeout must be an unsigned integer", json);
                            }
                        };
                        options.cancellation =
                            CancellationToken::with_timeout(Duration::from_millis(millis));
                    }
                    "--stop-after" => {
                        if action != "check" {
                            return usage("--stop-after is valid only for check", json);
                        }
                        options.stop_after = match Stage::from_name(value) {
                            Some(stage) if stage <= Stage::Verify => stage,
                            _ => return usage("invalid check stage", json),
                        };
                    }
                    _ => unreachable!(),
                }
            }
            flag if flag.starts_with('-') => return usage("unknown option", json),
            path => {
                if input.replace(PathBuf::from(path)).is_some() {
                    return usage("exactly one input file is required", json);
                }
            }
        }
        index += 1;
    }
    if options.target.triple == "x86_64-pc-windows-msvc" && options.windows.is_none() {
        options.windows = Some(WindowsLinkOptions::default());
    }
    if action == "run" && options.target.triple != TargetProfile::host_llvm().triple {
        return usage(
            "cross-target output must use build and an explicit external runner",
            json,
        );
    }
    let Some(input) = input else {
        return usage("input file is required", json);
    };
    let _temporary = if action == "run" && options.output.is_none() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = env::temp_dir().join(format!("zil-run-{}-{nonce}", std::process::id()));
        if let Err(error) = std::fs::create_dir(&directory) {
            return runtime_infrastructure("E-RUN-DIRECTORY", &error.to_string(), json, None);
        }
        options.output = Some(directory.join(if cfg!(windows) {
            "program.exe"
        } else {
            "program"
        }));
        Some(RunDirectory(directory))
    } else {
        None
    };
    let report = compile_path(&input, &options);
    if report.status != ResultCategory::Success {
        if json {
            println!("{}", report.to_json());
        } else {
            for diagnostic in &report.diagnostics {
                if let Some(path) = diagnostic
                    .span
                    .and_then(|span| report.source_files.get(&span.file))
                {
                    eprintln!("{path}: {diagnostic}");
                } else {
                    eprintln!("{diagnostic}");
                }
            }
        }
        return ExitCode::from(match report.status {
            ResultCategory::Unsupported => 3,
            ResultCategory::ResourceLimit => 4,
            ResultCategory::InfraError => 2,
            ResultCategory::Cancelled => 5,
            _ => 1,
        });
    }
    if action == "run" {
        return run_native(&report, json);
    }
    if json {
        println!("{}", report.to_json());
    } else if let Some(path) = report.artifacts.get("executable") {
        println!("{}", path.display());
    } else {
        println!("{}: {}", input.display(), report.phase);
    }
    ExitCode::SUCCESS
}
fn run_native(report: &CompileReport, json: bool) -> ExitCode {
    let Some(path) = report.artifacts.get("executable") else {
        return runtime_infrastructure(
            "E-RUN-ARTIFACT",
            "successful build omitted executable",
            json,
            Some(report),
        );
    };
    let status = if json {
        let output = match Command::new(path).stdin(Stdio::inherit()).output() {
            Ok(output) => output,
            Err(error) => {
                return runtime_infrastructure(
                    "E-RUN-EXEC",
                    &error.to_string(),
                    true,
                    Some(report),
                );
            }
        };
        let signal = signal(&output.status);
        println!(
            "{{\"compile\":{},\"run\":{{\"exit_code\":{},\"signal\":{},\"stdout\":{},\"stderr\":{}}}}}",
            report.to_json(),
            output
                .status
                .code()
                .map_or("null".into(), |code| code.to_string()),
            signal.map_or("null".into(), |signal| signal.to_string()),
            json_string(&String::from_utf8_lossy(&output.stdout)),
            json_string(&String::from_utf8_lossy(&output.stderr))
        );
        output.status
    } else {
        match Command::new(path).status() {
            Ok(status) => status,
            Err(error) => {
                return runtime_infrastructure(
                    "E-RUN-EXEC",
                    &error.to_string(),
                    false,
                    Some(report),
                );
            }
        }
    };
    let code = status
        .code()
        .unwrap_or_else(|| signal(&status).map_or(1, |signal| 128 + signal));
    ExitCode::from((code & 255) as u8)
}
fn signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}
fn runtime_infrastructure(
    code: &str,
    message: &str,
    json: bool,
    compile: Option<&CompileReport>,
) -> ExitCode {
    let diagnostic = Diagnostic::new(Phase::Run, ResultCategory::InfraError, code, message, None);
    if json {
        let result = format!(
            "{{\"status\":\"InfraError\",\"phase\":\"run\",\"diagnostics\":[{}],\"exit_code\":null}}",
            diagnostic.to_json()
        );
        if let Some(compile) = compile {
            println!("{{\"compile\":{},\"run\":{result}}}", compile.to_json());
        } else {
            println!("{result}");
        }
    } else {
        eprintln!("{diagnostic}");
    }
    ExitCode::from(2)
}
struct RunDirectory(PathBuf);
impl Drop for RunDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
