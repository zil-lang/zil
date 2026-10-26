//! Observe production results for the design lab; this does not translate or
//! trust the lab's safety summaries. Actual source goes through every stage.
use std::path::Path;
use zil_base::{FileId, ModuleId, SourceUnit, json_string};
use zil_driver::{CompilationSession, CompileOptions, Stage};

pub fn run(out: &Path, inputs: &Path) -> Result<(), String> {
    std::fs::create_dir(out).map_err(|e| e.to_string())?;
    let mut paths = std::fs::read_dir(inputs)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|x| x.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|p| p.extension().is_some_and(|x| x == "zil"));
    paths.sort();
    let mut rows = Vec::new();
    for path in paths {
        let name = path.file_stem().unwrap().to_string_lossy();
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let source = SourceUnit::new(FileId(0), ModuleId(0), path.to_string_lossy(), text);
        let mut session = CompilationSession::default();
        let config = CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: Some(out.join(format!("{name}-dumps"))),
            ..CompileOptions::default()
        };
        #[cfg(zil093_count)]
        let _ = zil_base::research093_take_counts();
        let report = session.compile_source(&source, &config);
        #[cfg(zil093_count)]
        let counts = zil_base::research093_take_counts();
        #[cfg(not(zil093_count))]
        let counts = [0usize; 16];
        if session.active_query_count() != 0 {
            return Err(format!("bridge left active query: {name}"));
        }
        rows.push(format!(
            "{{\"case\":{},\"counts\":{:?},\"report\":{}}}",
            json_string(&name),
            counts,
            report.to_json()
        ));
    }
    if rows.is_empty() {
        return Err("empty bridge corpus".into());
    }
    let json = format!(
        "{{\"schema\":{},\"production_commit\":{},\"counting\":{},\"cases\":[{}]}}",
        zil_base::SCHEMA_VERSION,
        json_string(option_env!("ZIL093_PRODUCTION_COMMIT").unwrap_or("UNBOUND")),
        cfg!(zil093_count),
        rows.join(",")
    );
    std::fs::write(out.join("report.json"), &json).map_err(|e| e.to_string())?;
    println!("{json}");
    Ok(())
}
