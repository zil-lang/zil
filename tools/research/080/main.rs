#![forbid(unsafe_code)]
//! Whole public file-entry requests, with one session across four real edits.
//! No production timers, alternate solver flags, or verifier bypasses.
use std::{path::PathBuf, time::Instant};
use zil_base::{ResultCategory, json_string};
use zil_driver::{CompilationSession, CompileOptions, Stage};

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(
            "usage: pipeline080 NEW_OUTPUT verify|build|build-emit ROOT.zil EDIT_TARGET.zil".into(),
        );
    }
    let output = PathBuf::from(&args[0]);
    std::fs::create_dir(&output).map_err(|e| e.to_string())?;
    let stage = match args[1].as_str() {
        "verify" | "verify-emit" => Stage::Verify,
        "build" | "build-emit" => Stage::Build,
        _ => return Err("unknown request mode".into()),
    };
    let root = PathBuf::from(&args[2]);
    let edit = PathBuf::from(&args[3]);
    let original = std::fs::read_to_string(&edit).map_err(|e| e.to_string())?;
    let private = format!("{original}\nfn impl080_private_edit() -> i32 => 17\n");
    let public = format!("{private}\npub fn impl080_public_edit() -> i32 => 29\n");
    let mut session = CompilationSession::with_capacity(1024);
    let mut records = Vec::new();
    let mut complete = true;
    for (scenario, source) in [
        ("cold", &original),
        ("unchanged", &original),
        ("private_edit", &private),
        ("public_edit", &public),
    ] {
        // Editing is outside the request timer. Reading/reparsing/invalidating
        // the edited file remains inside the actual compile_path request.
        std::fs::write(&edit, source).map_err(|e| e.to_string())?;
        let options = CompileOptions {
            stop_after: stage,
            emit_dir: args[1]
                .ends_with("-emit")
                .then(|| output.join(format!("{scenario}-dumps"))),
            output: (stage == Stage::Build).then(|| output.join(scenario)),
            ..CompileOptions::default()
        };
        let began = Instant::now();
        let report = session.compile_path(&root, &options);
        let api_ns = began.elapsed().as_nanos();
        complete &= report.status == ResultCategory::Success
            && report.verification_runs == 1
            && session.active_query_count() == 0;
        // Record exact source bytes for each scenario while edits are live.
        let mut sources = Vec::new();
        for (id, path) in &report.source_files {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let saved = output.join(format!("{scenario}-source-{}.zil", id.0));
            std::fs::write(&saved, &bytes).map_err(|e| e.to_string())?;
            sources.push(format!(
                "{{\"id\":{},\"path\":{},\"bytes_path\":{}}}",
                json_string(&id.0.to_string()),
                json_string(path),
                json_string(&saved.to_string_lossy())
            ));
        }
        records.push(format!("{{\"scenario\":{},\"api_wall_ns\":{},\"cached_queries\":{},\"active_queries\":{},\"sources\":[{}],\"report\":{}}}",
            json_string(scenario), api_ns, session.cached_query_count(), session.active_query_count(), sources.join(","), report.to_json()));
    }
    // Full process wall includes source recording, cleanup and report output.
    std::fs::write(&edit, original).map_err(|e| e.to_string())?;
    drop(session);
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let peak_rss_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<usize>().ok());
    let json = format!(
        "{{\"production_commit\":{},\"schema\":{},\"mode\":{},\"complete\":{},\"vmhwm_kib\":{},\"records\":[{}]}}",
        json_string(option_env!("ZIL080_PRODUCTION_COMMIT").unwrap_or("UNBOUND")),
        zil_base::SCHEMA_VERSION,
        json_string(&args[1]),
        complete,
        peak_rss_kib.map_or("null".into(), |n| n.to_string()),
        records.join(",")
    );
    std::fs::write(output.join("report.json"), &json).map_err(|e| e.to_string())?;
    println!("{json}");
    if complete {
        Ok(())
    } else {
        Err("one or more requests did not complete; full reports retained".into())
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
