use std::path::Path;
use zil_base::{FileId, Fingerprint, ModuleId, Phase, ResultCategory, SourceUnit, json_string};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, QueryDisposition, Stage};
#[path = "guards_extra.rs"]
mod extra;

const SOURCE: &str = "resource Cell { n: i32 }\nfn bump(r: Cell) { r.n += 1 }\nfn constant() -> i32 => 2\nfn main() -> i32 { let r = Cell { n: 0 }; bump(r); return r.n + #(constant()) - 3 }\n";
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "089-guards.zil", text)
}
fn options() -> CompileOptions {
    CompileOptions {
        stop_after: Stage::Lower,
        ..CompileOptions::default()
    }
}
fn record(
    out: &Path,
    records: &mut Vec<String>,
    name: &str,
    session: &CompilationSession,
    report: CompileReport,
    expected: ResultCategory,
) -> Result<(), String> {
    let row = format!(
        "{{\"scenario\":{},\"active_queries\":{},\"cached_queries\":{},\"report\":{}}}",
        json_string(name),
        session.active_query_count(),
        session.cached_query_count(),
        report.to_json()
    );
    std::fs::write(out.join(format!("{name}.json")), &row).map_err(|e| e.to_string())?;
    if report.status != expected
        || session.active_query_count() != 0
        || (expected == ResultCategory::Success && report.verification_runs != 1)
    {
        return Err(format!(
            "guard {name}: unexpected complete result; actual report retained"
        ));
    }
    records.push(row);
    Ok(())
}
fn request(
    out: &Path,
    records: &mut Vec<String>,
    name: &str,
    session: &mut CompilationSession,
    input: &SourceUnit,
    mut config: CompileOptions,
    expected: ResultCategory,
) -> Result<(), String> {
    config.emit_dir = Some(out.join(format!("{name}-dumps")));
    let report = session.compile_source(input, &config);
    record(out, records, name, session, report, expected)
}

fn boundary(out: &Path, records: &mut Vec<String>, ct: bool) -> Result<u64, String> {
    let input = source(SOURCE);
    let label = if ct { "ct" } else { "analysis" };
    let mut low = 0u64;
    let mut high = if ct {
        options().limits.max_ct_steps
    } else {
        options().limits.max_analysis_steps as u64
    };
    let mut attempts = 0;
    while high - low > 1 {
        let n = low + (high - low) / 2;
        let mut config = options();
        if ct {
            config.limits.max_ct_steps = n;
        } else {
            config.limits.max_analysis_steps = n as usize;
        }
        let mut session = CompilationSession::default();
        let report = session.compile_source(&input, &config);
        let expected = match report.status {
            ResultCategory::Success => {
                high = n;
                ResultCategory::Success
            }
            ResultCategory::ResourceLimit => {
                low = n;
                ResultCategory::ResourceLimit
            }
            _ => {
                return Err(format!(
                    "unexpected status finding {label} budget: {}",
                    report.to_json()
                ));
            }
        };
        record(
            out,
            records,
            &format!("search-{label}-{attempts:02}-{n}"),
            &session,
            report,
            expected,
        )?;
        attempts += 1;
    }
    // Actual complete-request N/N-1, including a populated session and recovery.
    let mut session = CompilationSession::default();
    for (tag, n, expected) in [
        ("n", high, ResultCategory::Success),
        ("n-minus-one", high - 1, ResultCategory::ResourceLimit),
        ("recovery", high, ResultCategory::Success),
    ] {
        let mut config = options();
        if ct {
            config.limits.max_ct_steps = n;
        } else {
            config.limits.max_analysis_steps = n as usize;
        }
        request(
            out,
            records,
            &format!("budget-{label}-{tag}"),
            &mut session,
            &input,
            config,
            expected,
        )?;
    }
    Ok(high)
}

pub fn run(out: &Path, common: &Path) -> Result<(), String> {
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    std::fs::write(out.join("source.zil"), SOURCE).map_err(|e| e.to_string())?;
    let mut records = Vec::new();
    let mut session = CompilationSession::with_capacity(1024);
    let input = source(SOURCE);
    for name in ["cold", "unchanged"] {
        request(
            out,
            &mut records,
            name,
            &mut session,
            &input,
            options(),
            ResultCategory::Success,
        )?;
    }
    for (name, text) in [
        ("ct-dependency-edit", SOURCE.replace("i32 => 2", "i32 => 4")),
        ("private-write-edit", SOURCE.replace("r.n += 1", "r.n += 2")),
        ("id-shift", format!("fn before() -> i32 => 91\n{SOURCE}")),
    ] {
        request(
            out,
            &mut records,
            name,
            &mut session,
            &source(&text),
            options(),
            ResultCategory::Success,
        )?;
    }
    let mut target = options();
    target.target.id = "089-alternate-explicit-profile".into();
    request(
        out,
        &mut records,
        "target",
        &mut session,
        &input,
        target,
        ResultCategory::Success,
    )?;
    let mut build = options();
    build.build_inputs = Fingerprint::of(b"089-other-build-input");
    request(
        out,
        &mut records,
        "build-input",
        &mut session,
        &input,
        build,
        ResultCategory::Success,
    )?;
    let file = SourceUnit::new(FileId(19), ModuleId(17), "089-other-identity.zil", SOURCE);
    request(
        out,
        &mut records,
        "file-identity",
        &mut session,
        &file,
        options(),
        ResultCategory::Success,
    )?;
    let mut limited = options();
    limited.limits.max_tokens = 1;
    request(
        out,
        &mut records,
        "token-limit",
        &mut session,
        &input,
        limited,
        ResultCategory::ResourceLimit,
    )?;
    for (name, text) in [
        ("diagnostic", "fn main() -> i32 => true\n"),
        ("diagnostic-position", "\n\nfn main() -> i32 => true\n"),
    ] {
        request(
            out,
            &mut records,
            name,
            &mut session,
            &source(text),
            options(),
            ResultCategory::SafeReject,
        )?;
    }
    let cancelled = options();
    cancelled.cancellation.cancel();
    request(
        out,
        &mut records,
        "cancel-before",
        &mut session,
        &input,
        cancelled,
        ResultCategory::Cancelled,
    )?;
    let config = options();
    let token = config.cancellation.clone();
    let report = session.compile_source_with_observer(&input, &config, &mut |event| {
        if event.phase == Phase::Parse && event.disposition == QueryDisposition::Reused {
            token.cancel();
        }
    });
    record(
        out,
        &mut records,
        "cancel-after-hit",
        &session,
        report,
        ResultCategory::Cancelled,
    )?;
    request(
        out,
        &mut records,
        "cancel-hit-recovery",
        &mut session,
        &input,
        options(),
        ResultCategory::Success,
    )?;
    let mut fresh = CompilationSession::default();
    let config = options();
    let token = config.cancellation.clone();
    let report = fresh.compile_source_with_observer(&input, &config, &mut |event| {
        if event.phase == Phase::Types {
            token.cancel();
        }
    });
    record(
        out,
        &mut records,
        "cancel-before-publish",
        &fresh,
        report,
        ResultCategory::Cancelled,
    )?;
    request(
        out,
        &mut records,
        "cancel-publication-recovery",
        &mut fresh,
        &input,
        options(),
        ResultCategory::Success,
    )?;
    let analysis_n = boundary(out, &mut records, false)?;
    let ct_n = boundary(out, &mut records, true)?;
    extra::run(out, common, &mut records)?;
    let json = format!(
        "{{\"production_commit\":{},\"schema\":{},\"status\":\"Pass\",\"analysis_n\":{},\"ct_n\":{},\"requests\":{},\"records\":[{}]}}",
        json_string(option_env!("ZIL107_PRODUCTION_IDENTITY").unwrap_or("UNBOUND")),
        zil_base::SCHEMA_VERSION,
        analysis_n,
        ct_n,
        records.len(),
        records.join(",")
    );
    std::fs::write(out.join("report.json"), json).map_err(|e| e.to_string())?;
    println!(
        "{{\"guards\":\"Pass\",\"requests\":{},\"analysis_n\":{},\"ct_n\":{}}}",
        records.len(),
        analysis_n,
        ct_n
    );
    Ok(())
}
