//! Ordinary POD hash growth keeps the old owner until rehash completes.
use super::*;

const MAP: &str = include_str!("growing-hash-collections/map.zil");
const SET: &str = include_str!("growing-hash-collections/set.zil");

#[test]
fn growing_generic_copy_key_set_source_completes_independent_verification() {
    let dir = directory("growing-generic-copy-key-set-source");
    fs::write(dir.join("main.zil"), SET).unwrap();
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
const CALLABLE_REFILL: &str =
    include_str!("growing-hash-collections/callable-store-field-refill.zil");

#[test]
fn callable_store_field_refill_source_preserves_the_mutable_input_effect() {
    let dir = directory("callable-store-field-refill-source");
    fs::write(dir.join("main.zil"), CALLABLE_REFILL).unwrap();
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
