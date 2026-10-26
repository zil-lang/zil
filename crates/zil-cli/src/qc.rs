//! First-party quality-tool entry. It starts a named executable directly; queue
//! contents and forwarded arguments are never evaluated by a shell.
use std::path::PathBuf;
use std::process::{Command, ExitCode};

const HELP: &str = "Usage: zil qc [--tool PATH] COMMAND [OPTIONS]\n\nCommands: hunt, issue list|claim|replay|reduce|verify|release, campaign, replay, generate\nTool: --tool PATH, then ZIL_QC_TOOL, then a sibling roam executable.\nBuild once: cargo build --locked --manifest-path tools/roam-rs/Cargo.toml\nContinuous example: zil qc --tool tools/roam-rs/target/debug/roam hunt --queue=out/qc/issues --out=out/qc/run-001 --cases=0 --stop-file=out/qc/stop\nA defect is retained in the queue; fixing source and verifying it are separate actions.\n";

pub fn run(args: &[String]) -> ExitCode {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let mut first = 0;
    let explicit = if args[0] == "--tool" {
        let Some(path) = args.get(1) else {
            return fail("--tool requires a path", 64);
        };
        first = 2;
        Some(PathBuf::from(path))
    } else if let Some(path) = args[0].strip_prefix("--tool=") {
        if path.is_empty() {
            return fail("--tool requires a path", 64);
        }
        first = 1;
        Some(PathBuf::from(path))
    } else {
        None
    };
    if first == args.len() {
        return fail("quality command required", 64);
    }
    let tool = explicit
        .or_else(|| {
            std::env::var_os("ZIL_QC_TOOL")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| {
            let sibling = std::env::current_exe()
                .ok()?
                .with_file_name(if cfg!(windows) { "roam.exe" } else { "roam" });
            sibling.is_file().then_some(sibling)
        });
    let Some(tool) = tool else {
        return fail(
            "quality tool is not installed; use --tool PATH or ZIL_QC_TOOL",
            2,
        );
    };
    match Command::new(&tool).args(&args[first..]).status() {
        Ok(status) => {
            if let Some(code) = status.code() {
                return ExitCode::from(code.clamp(0, 255) as u8);
            }
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = status.signal() {
                    return ExitCode::from((128 + signal).clamp(1, 255) as u8);
                }
            }
            fail("quality tool terminated without an exit code", 2)
        }
        Err(error) => fail(
            &format!("cannot start quality tool {}: {error}", tool.display()),
            2,
        ),
    }
}

fn fail(message: &str, code: u8) -> ExitCode {
    eprintln!("E-QC-TOOL: {message}\n{HELP}");
    ExitCode::from(code)
}
