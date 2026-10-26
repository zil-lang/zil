#![forbid(unsafe_code)]
//! Uninstrumented production compile_path, including discovery and file reads.
use std::{path::PathBuf, time::Instant};
use zil_base::{ResultCategory, json_string};
use zil_driver::{CompilationSession, CompileOptions, Stage};

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("usage: pipeline068 NEW_OUTPUT verify|build|build-emit ROOT.zil".into());
    }
    let directory = PathBuf::from(&args[0]);
    std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let stage = match args[1].as_str() {
        "verify" => Stage::Verify,
        "build" | "build-emit" => Stage::Build,
        _ => return Err("unknown stage".into()),
    };
    let root = PathBuf::from(&args[2]);
    let mut session = CompilationSession::with_capacity(1024);
    let mut records = Vec::new();
    let mut passed = true;
    for scenario in ["cold", "unchanged"] {
        let options = CompileOptions {
            stop_after: stage,
            emit_dir: (args[1] == "build-emit")
                .then(|| directory.join(format!("{scenario}-dumps"))),
            output: (stage == Stage::Build).then(|| directory.join(scenario)),
            ..CompileOptions::default()
        };
        let start = Instant::now();
        let report = session.compile_path(&root, &options);
        let wall_ns = start.elapsed().as_nanos();
        passed &= report.status == ResultCategory::Success
            && report.verification_runs == 1
            && session.active_query_count() == 0;
        records.push(format!(
            "{{\"scenario\":{},\"api_wall_ns\":{},\"cached_queries\":{},\"active_queries\":{},\"report\":{}}}",
            json_string(scenario), wall_ns, session.cached_query_count(), session.active_query_count(), report.to_json()
        ));
    }
    let json = format!(
        "{{\"kind\":\"full-compile-path\",\"mode\":{},\"timing_scope\":\"compile_path includes source reads, import discovery, quotas, fingerprints, requested dumps, independent Verify, and external clang/link for Build\",\"records\":[{}]}}",
        json_string(&args[1]),
        records.join(",")
    );
    std::fs::write(directory.join("report.json"), &json).map_err(|e| e.to_string())?;
    println!("{json}");
    if passed {
        Ok(())
    } else {
        Err("a production request failed; raw report retained".into())
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
