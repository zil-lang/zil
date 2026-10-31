//! Link against each frozen release compiler with rustc --edition 2024 -O,
//! -L dependency=TARGET/release/deps --extern zil_driver=... --extern zil_base=...
//! Add --cfg cp29 only for a compiler exposing CP29's optional memo statistics.
//! Arguments: CASE_DIRECTORY LIVE_DIRECTORY default|CAPACITY measure|control.
use std::{collections::BTreeMap, env, fs, io, path::Path, time::Instant};
use zil_base::{Fingerprint, Phase, ResultCategory, json_string};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, QueryDisposition, Stage};

fn copy_tree(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let destination = target.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), destination)?;
        } else {
            return Err(io::Error::other("fixtures must contain ordinary files/directories"));
        }
    }
    Ok(())
}
fn equivalent(a: &CompileReport, b: &CompileReport) -> bool {
    a.status == b.status && a.phase == b.phase && a.diagnostics == b.diagnostics
        && a.input_fingerprint == b.input_fingerprint && a.source_files == b.source_files
        && a.scalar_interfaces == b.scalar_interfaces
}
fn valid(report: &CompileReport, session: &CompilationSession) -> bool {
    report.status == ResultCategory::Success && report.phase == Phase::Verify
        && report.verification_runs == 1 && session.active_query_count() == 0
        && report.query_events.iter().filter(|event| event.phase == Phase::Verify
            && event.disposition == QueryDisposition::Executed).count() == 1
}
fn memo_json(_session: &CompilationSession) -> String {
    #[cfg(cp29)]
    {
        let current = _session.type_memo_stats();
        let retained = _session.retained_type_memo_stats();
        return format!("{{\"eligible\":{},\"reused_functions\":{},\"checked_functions\":{},\"retained_modules\":{},\"retained_weight\":{}}}",
            current.eligible, current.reused_functions, current.checked_functions,
            retained.retained_modules, retained.retained_weight);
    }
    #[cfg(not(cp29))]
    "null".into()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 5 || !matches!(args[4].as_str(), "measure" | "control") {
        return Err("usage: session CASE LIVE default|CAPACITY measure|control".into());
    }
    let fixture = Path::new(&args[1]).canonicalize()?;
    let requested_live = Path::new(&args[2]);
    let live = requested_live.parent().ok_or("live needs a parent")?.canonicalize()?
        .join(requested_live.file_name().ok_or("live needs a directory name")?);
    // LIVE must be a dedicated empty directory, never an existing user tree.
    if live.starts_with(&fixture) || fixture.starts_with(&live) {
        return Err("live and fixture trees must be separate".into());
    }
    fs::create_dir(&live)?;
    let start = Instant::now();
    let mut session = if args[3] == "default" { CompilationSession::default() }
        else { CompilationSession::with_capacity(args[3].parse()?) };
    let options = CompileOptions { stop_after: Stage::Verify, ..CompileOptions::default() };
    println!("{{\"kind\":\"session\",\"capacity\":{},\"mode\":{},\"initialization_ns\":{}}}",
        json_string(&args[3]), json_string(&args[4]), start.elapsed().as_nanos());
    let sequence = [("cold", "base"), ("unchanged", "base"), ("private", "private"),
        ("public", "public"), ("restore", "base"), ("private_again", "private"),
        ("restore_again", "base")];
    let mut previous = "";
    let mut passed = true;
    for (request, snapshot) in sequence {
        let start = Instant::now();
        if previous != snapshot {
            fs::remove_dir_all(&live)?;
            copy_tree(&fixture.join(snapshot).join("zil"), &live)?;
        }
        let preparation_ns = start.elapsed().as_nanos();
        previous = snapshot;
        let start = Instant::now();
        let report = session.compile_path(&live.join("main.zil"), &options);
        let compile_ns = start.elapsed().as_nanos();
        let fresh_equal = if args[4] == "control" {
            let mut fresh = CompilationSession::default();
            let fresh_report = fresh.compile_path(&live.join("main.zil"), &options);
            Some(valid(&fresh_report, &fresh) && equivalent(&report, &fresh_report))
        } else { None };
        let start = Instant::now();
        let ok = valid(&report, &session) && fresh_equal.unwrap_or(true);
        passed &= ok;
        let mut phases = BTreeMap::<&str, [usize; 2]>::new();
        for event in &report.query_events {
            phases.entry(event.phase.as_str()).or_default()
                [usize::from(event.disposition == QueryDisposition::Reused)] += 1;
        }
        let phases = phases.iter().map(|(phase, counts)| format!(
            "{}:{{\"executed\":{},\"reused\":{}}}", json_string(phase), counts[0], counts[1]))
            .collect::<Vec<_>>().join(",");
        let diagnostics = report.diagnostics.iter().map(|d| d.to_json()).collect::<Vec<_>>().join(",");
        // Observable report equality only; this is not a Typed IR/native oracle.
        let digest = Fingerprint::of(format!("{:?}{:?}{:?}{:?}{:?}{:?}", report.status,
            report.phase, report.diagnostics, report.input_fingerprint, report.source_files,
            report.scalar_interfaces).as_bytes());
        let memo = memo_json(&session);
        println!("{{\"kind\":\"request\",\"request\":{},\"snapshot\":{},\"compile_ns\":{},\"preparation_ns\":{},\"report_preparation_ns\":{},\"valid\":{},\"fresh_equal\":{},\"status\":{},\"phase\":{},\"verification_runs\":{},\"active_query_count\":{},\"cached_query_count\":{},\"input_fingerprint\":{},\"observable_digest\":{},\"type_memo\":{},\"phases\":{{{}}},\"diagnostics\":[{}]}}",
            json_string(request), json_string(snapshot), compile_ns, preparation_ns,
            start.elapsed().as_nanos(), ok, fresh_equal.map_or("null", |v| if v { "true" } else { "false" }),
            json_string(report.status.as_str()), json_string(report.phase.as_str()), report.verification_runs,
            session.active_query_count(), session.cached_query_count(),
            json_string(&report.input_fingerprint.to_hex()), json_string(&digest.to_hex()), memo, phases, diagnostics);
    }
    println!("{{\"kind\":\"end\",\"pass\":{passed}}}");
    if !passed { std::process::exit(1); }
    Ok(())
}
