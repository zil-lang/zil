use std::{fs, path::PathBuf};
use zil_base::ResultCategory;
use zil_driver::{CompilationSession, CompileOptions, Optimization, Stage};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "source output-dir O0|O2");
    let source = PathBuf::from(&args[1]);
    let root = PathBuf::from(&args[2]);
    fs::create_dir(&root).unwrap();
    let optimization = match args[3].as_str() {
        "O0" => Optimization::O0,
        "O2" => Optimization::O2,
        _ => panic!("unknown optimization"),
    };
    let mut session = CompilationSession::default();
    let report = session.compile_path(
        &source,
        &CompileOptions {
            stop_after: Stage::Build,
            optimization,
            output: Some(root.join("program")),
            emit_dir: Some(root.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(root.join("compile.json"), report.to_json()).unwrap();
    println!("{}", report.to_json());
    if report.status != ResultCategory::Success
        || report.verification_runs != 1
        || session.active_query_count() != 0
    {
        std::process::exit(1)
    }
}
