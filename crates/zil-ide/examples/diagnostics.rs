//! A process adapter for testing/embedding the actual editor query API.
use std::path::Path;
fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("expected one source path");
        std::process::exit(64)
    };
    let report = zil_ide::EditorSession::default()
        .analyze_file(Path::new(&path), &zil_driver::CompileOptions::default());
    println!("{}", report.to_json());
    if report.status != zil_base::ResultCategory::Success {
        std::process::exit(1)
    }
}
