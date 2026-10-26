#![forbid(unsafe_code)]
//! Observe the actual production driver through its public progress API.
//! Intervals end at a scheduling event, not exclusive compiler pass timings.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Instant,
};
use zil_base::{FileId, ModuleId, SourceUnit, json_string};
use zil_driver::{CompilationSession, CompileOptions, ProjectSources, Stage};
fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 3 {
        return Err(
            "usage: pipeline063 OUTPUT_DIRECTORY verify|build|verify-emit|build-emit ROOT.zil [MODULE.zil ...]".into(),
        );
    }
    let directory = PathBuf::from(&args[0]);
    if directory.exists() {
        return Err("new output directory required".into());
    }
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let emit = args[1].ends_with("-emit");
    let stage = match args[1].trim_end_matches("-emit") {
        "verify" => Stage::Verify,
        "build" => Stage::Build,
        _ => return Err("only verify/build stages are profiled".into()),
    };
    let io_started = Instant::now();
    let mut project = ProjectSources {
        root: ModuleId(0),
        modules: vec![],
        paths: BTreeMap::new(),
    };
    for (i, name) in args[2..].iter().enumerate() {
        let path = Path::new(name);
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let source = SourceUnit::new(FileId(i as u32), ModuleId(i as u32), name, text.as_str());
        project.paths.insert(
            vec![
                path.file_stem()
                    .ok_or("source filename")?
                    .to_string_lossy()
                    .to_string(),
            ],
            source.module_id,
        );
        project.modules.push(source);
    }
    let io_ns = io_started.elapsed().as_nanos();
    let mut session = CompilationSession::with_capacity(128);
    let mut records = vec![];
    let mut all_ok = true;
    let mut scenarios = vec!["cold", "unchanged", "comment_edit"];
    if project
        .modules
        .iter()
        .any(|s| s.text.contains("return 7 // 063 private edit"))
    {
        scenarios.push("private_body_edit");
    }
    for scenario in scenarios {
        if scenario == "comment_edit" {
            let old = project.modules.last().ok_or("empty project")?;
            let new = SourceUnit::new(
                old.file_id,
                old.module_id,
                &old.path,
                format!("{}\n// 063 semantic no-op edit\n", old.text),
            );
            *project.modules.last_mut().unwrap() = new;
        }
        if scenario == "private_body_edit" {
            let at = project
                .modules
                .iter()
                .position(|s| s.text.contains("return 7 // 063 private edit"))
                .unwrap();
            let old = &project.modules[at];
            project.modules[at] = SourceUnit::new(
                old.file_id,
                old.module_id,
                &old.path,
                old.text.replace(
                    "return 7 // 063 private edit",
                    "return 9 // 063 private edit",
                ),
            );
        }
        let options = CompileOptions {
            stop_after: stage,
            emit_dir: emit.then(|| directory.join(format!("{scenario}-dumps"))),
            output: (stage == Stage::Build).then(|| directory.join(scenario)),
            ..CompileOptions::default()
        };
        let mut intervals = vec![];
        let began = Instant::now();
        let mut last = began;
        let report = session.compile_project_with_observer(&project, &options, &mut |event| {
            let now = Instant::now();
            intervals.push(format!(
                "{{\"ended_at\":{},\"disposition\":{},\"elapsed_ns\":{}}}",
                json_string(event.phase.as_str()),
                json_string(event.disposition.as_str()),
                now.duration_since(last).as_nanos()
            ));
            last = now;
        });
        let end = Instant::now();
        all_ok &= report.diagnostics.is_empty();
        records.push(format!("{{\"scenario\":{},\"wall_ns\":{},\"tail_ns\":{},\"cached_queries\":{},\"intervals\":[{}],\"report\":{}}}", json_string(scenario), end.duration_since(began).as_nanos(), end.duration_since(last).as_nanos(), session.cached_query_count(), intervals.join(","), report.to_json()));
    }
    let json = format!(
        "{{\"kind\":\"actual-production-driver-observer\",\"schema\":9,\"stage\":{},\"io_ns\":{},\"interval_semantics\":\"wall intervals end at progress events; include preceding publication, fingerprints and formatting; Link includes LLVM emission plus external clang/link\",\"records\":[{}]}}",
        json_string(&args[1]),
        io_ns,
        records.join(",")
    );
    std::fs::write(directory.join("report.json"), &json).map_err(|e| e.to_string())?;
    println!("{json}");
    if !all_ok {
        return Err("one or more actual compiler invocations failed; see complete report".into());
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
