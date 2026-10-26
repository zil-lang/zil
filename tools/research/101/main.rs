#![forbid(unsafe_code)]
//! Identical complete production requests for the frozen baseline/candidate.
#[path = "../089/guards.rs"]
mod guards;
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
use zil_base::{Fingerprint, ResultCategory, json_string};
use zil_driver::{CompilationSession, CompileOptions, Stage};

#[derive(Default)]
struct Restore(BTreeMap<PathBuf, Vec<u8>>);
impl Restore {
    fn apply(&mut self, manifest: &std::path::Path) -> Result<(), String> {
        for line in std::fs::read_to_string(manifest)
            .map_err(|e| e.to_string())?
            .lines()
        {
            let (target, replacement) = line.split_once('\t').ok_or("invalid edit manifest")?;
            let target = PathBuf::from(target);
            if !self.0.contains_key(&target) {
                self.0.insert(
                    target.clone(),
                    std::fs::read(&target).map_err(|e| e.to_string())?,
                );
            }
            let bytes = std::fs::read(replacement).map_err(|e| e.to_string())?;
            std::fs::write(target, bytes).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<(), String> {
        for (path, bytes) in &self.0 {
            std::fs::write(path, bytes).map_err(|e| e.to_string())?;
        }
        self.0.clear();
        Ok(())
    }
}
impl Drop for Restore {
    fn drop(&mut self) {
        for (path, bytes) in &self.0 {
            let _ = std::fs::write(path, bytes);
        }
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "guards") && args.len() == 3 {
        return guards::run(&PathBuf::from(&args[1]), &PathBuf::from(&args[2]));
    }
    if args.len() != 5 {
        return Err("usage: pipeline101 NEW_OUTPUT verify|build[-emit] ROOT.zil EDIT_MANIFEST_DIR CAPACITY; or guards NEW_OUTPUT".into());
    }
    let output = PathBuf::from(&args[0]);
    std::fs::create_dir(&output).map_err(|e| e.to_string())?;
    let stage = match args[1].as_str() {
        "verify" | "verify-emit" => Stage::Verify,
        "build" | "build-emit" => Stage::Build,
        _ => return Err("unknown request mode".into()),
    };
    let root = PathBuf::from(&args[2]);
    let edits = PathBuf::from(&args[3]);
    let capacity = args[4].parse::<usize>().map_err(|e| e.to_string())?;
    let scenarios =
        std::fs::read_to_string(edits.join("scenarios.txt")).map_err(|e| e.to_string())?;
    #[cfg(zil101_diag)]
    zil_base::research101_set_algorithm(
        std::env::var("ZIL101_ALGORITHM")
            .unwrap_or_else(|_| "0".into())
            .parse::<u8>()
            .map_err(|e| e.to_string())?,
    );
    let mut session = CompilationSession::with_capacity(capacity);
    let mut restore = Restore::default();
    let mut records = Vec::new();
    let mut complete = true;
    for scenario in scenarios.lines() {
        // The editor's write is outside the request timer. Discovery, file
        // reads, hashing, invalidation and every Verify are inside compile_path.
        if scenario.ends_with("_edit") || scenario == "body_restore" {
            restore.apply(&edits.join(format!("{scenario}.tsv")))?;
        }
        let options = CompileOptions {
            stop_after: stage,
            emit_dir: args[1]
                .ends_with("-emit")
                .then(|| output.join(format!("{scenario}-dumps"))),
            output: (stage == Stage::Build).then(|| output.join(scenario)),
            ..CompileOptions::default()
        };
        #[cfg(any(zil101_count, zil101_diag))]
        let _ = zil_base::research101_take_counts();
        #[cfg(zil101_diag)]
        let _ = zil_base::research101_take_times();
        let began = Instant::now();
        let report = session.compile_path(&root, &options);
        let api_ns = began.elapsed().as_nanos();
        #[cfg(any(zil101_count, zil101_diag))]
        let counts = zil_base::research101_take_counts();
        #[cfg(not(any(zil101_count, zil101_diag)))]
        let counts = [0usize; 16];
        #[cfg(zil101_diag)]
        let (phase_ns, phase_calls) = zil_base::research101_take_times();
        #[cfg(not(zil101_diag))]
        let (phase_ns, phase_calls) = ([0u128; 21], [0usize; 21]);
        complete &= report.status == ResultCategory::Success
            && report.verification_runs == 1
            && session.active_query_count() == 0;
        // Inputs and edited variants are retained once by the controller.
        // Record actual observed bytes; do not write N source copies per run.
        let observation_start = Instant::now();
        let sources = report
            .source_files
            .iter()
            .map(|(id, path)| {
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                Ok(format!(
                    "{{\"id\":{},\"path\":{},\"sha256\":{}}}",
                    id.0,
                    json_string(path),
                    json_string(&Fingerprint::of(&bytes).to_hex())
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let report_json = report.to_json();
        let observation_ns = observation_start.elapsed().as_nanos();
        records.push(format!("{{\"scenario\":{},\"api_wall_ns\":{},\"counts\":{:?},\"phase_ns\":{:?},\"phase_calls\":{:?},\"observation_ns\":{},\"cached_queries\":{},\"active_queries\":{},\"sources\":[{}],\"report\":{}}}",
            json_string(scenario), api_ns, counts, phase_ns, phase_calls, observation_ns, session.cached_query_count(), session.active_query_count(), sources.join(","), report_json));
    }
    restore.finish()?;
    let drop_start = Instant::now();
    drop(session);
    let session_drop_ns = drop_start.elapsed().as_nanos();
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let peak_rss_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<usize>().ok());
    let json = format!(
        "{{\"production_identity\":{},\"schema\":{},\"counting\":{},\"diagnostic\":{},\"mode\":{},\"capacity\":{},\"complete\":{},\"vmhwm_kib\":{},\"session_drop_ns\":{},\"records\":[{}]}}",
        json_string(option_env!("ZIL101_PRODUCTION_IDENTITY").unwrap_or("UNBOUND")),
        zil_base::SCHEMA_VERSION,
        cfg!(zil101_count),
        cfg!(zil101_diag),
        json_string(&args[1]),
        capacity,
        complete,
        peak_rss_kib.map_or("null".into(), |n| n.to_string()),
        session_drop_ns,
        records.join(",")
    );
    std::fs::write(output.join("report.json"), &json).map_err(|e| e.to_string())?;
    println!(
        "pipeline101 {} {} complete={complete}",
        args[1],
        output.display()
    );
    if complete {
        Ok(())
    } else {
        Err("non-complete request; full report retained".into())
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
