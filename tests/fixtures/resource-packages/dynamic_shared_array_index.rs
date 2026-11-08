//! Dynamic selection conservatively retains every actual fixed-array source.
use super::*;
const WINDOW: &str = include_str!("dynamic-shared-address-array.zil");

fn verify_source(label: &str, program: &str) {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let report = compile_source(
        &source(label, program),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let raw = typed_source(label, program, &target, &limits);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    println!("{label}: Source/raw/Wire Success");
}

#[test]
fn dynamic_indices_keep_all_actual_element_sources() {
    let bare = "fn main()->i32{var first=7;var second=9;let items=[&first,&second];let index=1;let pointer=items[index];return *pointer-9}";
    let record = "struct Batch<life a>{items:[&'a i32;2]}fn main()->i32{let first=7;let second=9;let batch=Batch{items:[&first,&second]};let index=1;return *batch.items[index]-9}";
    let package = "package Job<life a>{manages bytes:ByteBuffer;items:[&'a i32;2]}fn make<life a>(first:&'a i32,second:&'a i32)->Job<'a>{var bytes=bytes_new();return Job::<'a>{bytes:give bytes,items:[first,second]}}fn main()->i32{let first=7;let second=9;let job=make(&first,&second);let window=&job;let index=1;return *window.items[index]-9}";
    for (label, program) in [
        ("bare", bare),
        ("window-helper", WINDOW),
        ("record", record),
        ("owning-package", package),
    ] {
        verify_source(label, program);
    }
}

#[test]
fn rigid_element_uppers_have_explicit_literal_and_dynamic_qualification() {
    let prefix = "struct Refs<life a>{pointer:&'a i32}fn read<life a,life b>(left:Refs<'a>,right:Refs<'b>,index:i32)->i32{let items=[left.pointer,right.pointer];return *items[";
    for index in ["0", "1", "index"] {
        let program = format!("{prefix}{index}]}}fn main()->i32{{return 0}}");
        let report = compile_source(
            &source("rigid-upper-qualification", &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        println!("rich distinct rigid index={index}: {}", report.to_json());
        if index != "index" {
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
        } else {
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
        }
    }
}
