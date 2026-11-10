//! Research worker: preserve raw compiler state between explicit requests.
//! No listener, file watcher, background compiler, cached Verify or JIT.
use std::io::{self, BufRead, Read, Write};
use std::path::PathBuf;
use std::time::Instant;
use zil_driver::{CodegenOptions, CompilationSession, CompileOptions, FunctionMerging, NativeBackend, Optimization, Stage};

fn memory(key: &str) -> usize {
    std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)).and_then(|n| n.parse().ok())).unwrap_or(0)
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 5 { return Err("usage: resident_session check|O0|O2 source output cache capacity".into()); }
    let capacity = args[4].parse::<usize>()?;
    if !(8..=256).contains(&capacity) { return Err("capacity must be 8..256 raw query entries (not a byte memory guarantee)".into()); }
    let mode = args[0].as_str();
    if !matches!(mode, "check"|"O0"|"O2") { return Err("invalid mode".into()); }
    let cache = PathBuf::from(&args[3]);
    let options = CompileOptions {
        stop_after: if mode == "check" { Stage::Verify } else { Stage::Build },
        backend: NativeBackend::LlvmApi,
        output: (mode != "check").then(|| PathBuf::from(&args[2])),
        semantic_cache: Some(cache.join("semantic")),
        object_cache: Some(cache.join("objects")),
        clang: Some(std::env::var_os("ZIL_CLANG").ok_or("ZIL_CLANG missing")?.into()),
        optimization: if mode == "O2" { Optimization::O2 } else { Optimization::O0 },
        incremental_o2: mode == "O2",
        function_merging: if mode == "O2" { FunctionMerging::AfterOptimization } else { FunctionMerging::Disabled },
        codegen: CodegenOptions { units: 8, jobs: 2 },
        ..CompileOptions::default()
    };
    let mut session = CompilationSession::with_capacity(capacity);
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    writeln!(output,"{{\"ready\":true,\"protocol\":1,\"pid\":{},\"capacity_entries\":{capacity}}}",std::process::id())?;
    output.flush()?;
    loop {
        let mut line=Vec::new();
        let n=(&mut input).take(33).read_until(b'\n', &mut line)?;
        if n==0 { break; }
        if n>32 || !line.ends_with(b"\n") { return Err("invalid or oversized protocol command".into()); }
        match line.as_slice() {
            b"quit\n" => break,
            b"clear\n" => { session.clear(); writeln!(output,"{{\"cleared\":true,\"cached_queries\":{}}}",session.cached_query_count())?; }
            b"compile\n" => {
                let start=Instant::now();
                let report=session.compile_path(&PathBuf::from(&args[1]), &options);
                let elapsed=start.elapsed().as_secs_f64()*1000.0;
                // Reading memory statistics is outside the inner compiler timer;
                // the harness also records full request/response wall time.
                writeln!(output,"{{\"elapsed_ms\":{elapsed},\"cached_queries\":{},\"active_queries\":{},\"rss_kib\":{},\"peak_kib\":{},\"report\":{}}}",session.cached_query_count(),session.active_query_count(),memory("VmRSS:"),memory("VmHWM:"),report.to_json())?;
            }
            _ => return Err("unknown protocol command".into()),
        }
        output.flush()?;
    }
    Ok(())
}
fn main() {
    if let Err(error)=run() { eprintln!("{error}"); std::process::exit(2); }
}
