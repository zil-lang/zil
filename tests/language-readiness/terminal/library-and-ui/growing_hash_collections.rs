//! Ordinary POD hash growth keeps the old owner until rehash completes.
use super::*;

const MAP: &str = include_str!("growing-hash-collections/map.zil");

#[test]
fn growing_pod_hash_map_source_completes_independent_verification() {
    let dir = directory("growing-pod-hash-map-source");
    fs::write(dir.join("main.zil"), MAP).unwrap();
    let report = CompilationSession::default().compile_path(
        &dir.join("main.zil"),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}
