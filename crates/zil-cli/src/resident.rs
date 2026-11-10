//! Explicit, single-project stdio requests over the existing raw session.
//! No listener, watcher, cached Verify authority, program execution or JIT.
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};
use zil_base::{Diagnostic, Phase, ResultCategory};
use zil_driver::{CancellationToken, CompilationSession, CompileOptions, ProjectManifest, SyntaxLock};

pub(super) struct Config {
    pub capacity: usize,
    pub reset_after: usize,
    pub timeout: Option<Duration>,
    pub syntax_lock: Option<PathBuf>,
}
impl Default for Config {
    fn default() -> Self {
        Self { capacity: 32, reset_after: 64, timeout: None, syntax_lock: None }
    }
}

fn refresh(input: &Path, fixed: &CompileOptions, config: &Config) -> Result<(PathBuf, CompileOptions), Vec<Diagnostic>> {
    let mut options = fixed.clone();
    // A cooperative deadline belongs to one request, not to the worker's age.
    options.cancellation = config.timeout.map_or_else(CancellationToken::default, CancellationToken::with_timeout);
    if let Some(path) = &config.syntax_lock {
        let text = std::fs::read_to_string(path).map_err(|error| vec![Diagnostic::new(
            Phase::Driver, ResultCategory::InfraError, "E-RESIDENT-SYNTAX-READ",
            format!("{}: {error}", path.display()), None,
        )])?;
        options.syntax_lock = Some(SyntaxLock::parse(&text)?);
    }
    // Do not retain a previously parsed manifest/lock as current filesystem input.
    if input.is_dir() || input.file_name().is_some_and(|name| name == "Zil.toml") {
        let manifest = ProjectManifest::open(input, &options.limits)?;
        manifest.check_lock()?;
        options.build_inputs = manifest.fingerprint;
        let entry = manifest.entry.clone();
        options.project = Some(Arc::new(manifest));
        Ok((entry, options))
    } else {
        Ok((input.to_path_buf(), options))
    }
}

fn memory(key: &str) -> Option<usize> {
    std::fs::read_to_string("/proc/self/status").ok().and_then(|text| {
        text.lines().find(|line| line.starts_with(key))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
    })
}
fn memory_json(key: &str) -> String {
    memory(key).map_or_else(|| "null".into(), |value| value.to_string())
}
fn failed_report(errors: &[Diagnostic]) -> String {
    let status = errors.first().map_or(ResultCategory::InfraError, |error| error.category);
    let phase = errors.first().map_or(Phase::Driver, |error| error.phase);
    format!("{{\"status\":\"{}\",\"phase\":\"{}\",\"diagnostics\":[{}],\"artifacts\":{{}},\"query_events\":[],\"verification_runs\":0}}",
        status.as_str(), phase.as_str(), errors.iter().map(Diagnostic::to_json).collect::<Vec<_>>().join(","))
}

fn serve(input_path: &Path, options: &CompileOptions, config: &Config,
    mut input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut session = CompilationSession::with_capacity(config.capacity);
    let (mut requests, mut retained, mut epoch) = (0u64, 0usize, 0u64);
    writeln!(output, "{{\"ready\":true,\"protocol\":1,\"pid\":{},\"capacity_entries\":{},\"reset_after\":{}}}",
        std::process::id(), config.capacity, config.reset_after)?;
    output.flush()?;
    loop {
        let mut line = Vec::new();
        let n = (&mut input).take(33).read_until(b'\n', &mut line)?;
        if n == 0 { return Ok(()); }
        if n > 32 || !line.ends_with(b"\n") {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "resident command must be a complete line of at most 32 bytes"));
        }
        if line.ends_with(b"\r\n") { line.remove(line.len() - 2); }
        match line.as_slice() {
            b"quit\n" => return Ok(()),
            b"clear\n" => {
                // Drop all owned histories, not only the raw query LRU.
                session = CompilationSession::with_capacity(config.capacity);
                retained = 0; epoch += 1;
                writeln!(output, "{{\"cleared\":true,\"cached_queries\":0,\"session_epoch\":{epoch}}}")?;
            }
            b"stats\n" => writeln!(output, "{{\"requests\":{requests},\"session_epoch\":{epoch},\"retained_requests\":{retained},\"cached_queries\":{},\"active_queries\":{},\"rss_kib\":{},\"peak_kib\":{}}}",
                session.cached_query_count(), session.active_query_count(), memory_json("VmRSS:"), memory_json("VmHWM:"))?,
            b"compile\n" => {
                requests += 1;
                let start = Instant::now();
                let report = match refresh(input_path, options, config) {
                    Ok((path, current)) => session.compile_path(&path, &current).to_json(),
                    Err(errors) => failed_report(&errors),
                };
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                retained += 1;
                let cleared = retained >= config.reset_after;
                if cleared {
                    session = CompilationSession::with_capacity(config.capacity);
                    retained = 0; epoch += 1;
                }
                // Full request latency also includes eviction, metrics and I/O.
                // Entry/epoch bounds are not an exact process byte/RSS guarantee.
                writeln!(output, "{{\"elapsed_ms\":{elapsed},\"request\":{requests},\"session_epoch\":{epoch},\"session_cleared\":{cleared},\"cached_queries\":{},\"active_queries\":{},\"rss_kib\":{},\"peak_kib\":{},\"report\":{report}}}",
                    session.cached_query_count(), session.active_query_count(), memory_json("VmRSS:"), memory_json("VmHWM:"))?;
            }
            _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "unknown resident command")),
        }
        output.flush()?;
    }
}

pub(super) fn run(input: &Path, options: &CompileOptions, config: Config) -> ExitCode {
    match serve(input, options, &config, io::stdin().lock(), io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("resident protocol: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_is_bounded_and_eof_is_not_a_compile_request() {
        for bytes in [b"unknown\n".as_slice(), b"compile".as_slice(), &[b'x'; 33]] {
            let mut output = Vec::new();
            assert!(serve(Path::new("not-read.zil"), &CompileOptions::default(), &Config::default(), io::Cursor::new(bytes), &mut output).is_err());
            let text = String::from_utf8(output).unwrap();
            assert!(text.contains("\"ready\":true"));
            assert!(!text.contains("\"report\""));
        }
        let mut output = Vec::new();
        serve(Path::new("not-read.zil"), &CompileOptions::default(), &Config::default(), io::Cursor::new(b"clear\r\nstats\nquit\ncompile\n"), &mut output).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert_eq!(text.lines().count(), 3);
        assert!(text.contains("\"session_epoch\":1"));
        assert!(text.contains("\"requests\":0"));
        assert!(!text.contains("\"report\""));
    }
}
