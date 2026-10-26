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
        return Err("usage: pipeline097 NEW_OUTPUT verify|build[-emit] ROOT.zil EDIT_MANIFEST_DIR CAPACITY; or guards NEW_OUTPUT".into());
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
    let mut session = CompilationSession::with_capacity(capacity);
    let mut restore = Restore::default();
    let mut records = Vec::new();
    let mut complete = true;
    for scenario in ["cold", "unchanged", "private_edit", "public_edit"] {
        // The editor's write is outside the request timer. Discovery, file
        // reads, hashing, invalidation and every Verify are inside compile_path.
        if scenario.ends_with("_edit") {
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
        #[cfg(zil097_count)]
        let _ = zil_base::research097_take_counts();
        let began = Instant::now();
        let report = session.compile_path(&root, &options);
        let api_ns = began.elapsed().as_nanos();
        #[cfg(zil097_count)]
        let counts = zil_base::research097_take_counts();
        #[cfg(not(zil097_count))]
        let counts = [0usize; 8];
        complete &= report.status == ResultCategory::Success
            && report.verification_runs == 1
            && session.active_query_count() == 0;
        // Inputs and edited variants are retained once by the controller.
        // Record actual observed bytes; do not write N source copies per run.
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
        records.push(format!("{{\"scenario\":{},\"api_wall_ns\":{},\"counts\":{:?},\"cached_queries\":{},\"active_queries\":{},\"sources\":[{}],\"report\":{}}}",
            json_string(scenario), api_ns, counts, session.cached_query_count(), session.active_query_count(), sources.join(","), report.to_json()));
    }
    restore.finish()?;
    drop(session);
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let peak_rss_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<usize>().ok());
    let json = format!(
        "{{\"production_identity\":{},\"schema\":{},\"counting\":{},\"mode\":{},\"capacity\":{},\"complete\":{},\"vmhwm_kib\":{},\"records\":[{}]}}",
        json_string(option_env!("ZIL097_PRODUCTION_IDENTITY").unwrap_or("UNBOUND")),
        zil_base::SCHEMA_VERSION,
        cfg!(zil097_count),
        json_string(&args[1]),
        capacity,
        complete,
        peak_rss_kib.map_or("null".into(), |n| n.to_string()),
        records.join(",")
    );
    std::fs::write(output.join("report.json"), &json).map_err(|e| e.to_string())?;
    println!(
        "pipeline097 {} {} complete={complete}",
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
