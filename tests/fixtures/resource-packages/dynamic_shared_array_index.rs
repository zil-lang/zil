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
    let record = "struct Batch<life a>{items:[&'a i32;2]}fn make<life a>(first:&'a i32,second:&'a i32)->Batch<'a>{return Batch::<'a>{items:[first,second]}}fn main()->i32{let first=7;let second=9;let batch=make(&first,&second);let index=1;return *batch.items[index]-9}";
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

#[test]
fn dynamic_selection_keeps_both_sources_live_until_its_last_use() {
    for target in ["first", "second"] {
        let program = format!(
            "fn main()->i32{{var first=7;var second=9;let items=[&first,&second];let index=1;let pointer=items[index];{target}=11;return *pointer-9}}"
        );
        let report = compile_source(
            &source("dynamic-live-source-write", &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{}",
            report.to_json()
        );
    }
    verify_source(
        "after last dynamic use",
        "fn main()->i32{var first=7;var second=9;let items=[&first,&second];let index=1;let pointer=items[index];let value=*pointer;first=11;second=12;return value-9}",
    );
    verify_source(
        "constant selection keeps precision",
        "fn main()->i32{var first=7;var second=9;let items=[&first,&second];let pointer=items[1];first=11;return *pointer-9}",
    );
}

#[test]
fn raw_and_wire_reject_live_writes_to_each_dynamic_alternative() {
    let program = "fn main()->i32{var first=7;var second=9;var spare=0;let items=[&first,&second];let index=1;let pointer=items[index];spare=11;return *pointer-9}";
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let good = typed_source("dynamic-actual-source", program, &target, &limits);
    let prior = zil_analyze::analyze(&good, &limits).unwrap();
    zil_verify::verify_module(good.clone(), prior.clone(), &target, &limits).unwrap();
    for target_name in ["first", "second"] {
        let mut bad = good.clone();
        let f = bad.functions.iter_mut().find(|f| f.name == "main").unwrap();
        let spare = f.locals.iter().find(|l| l.name == "spare").unwrap().id;
        let selected = f.locals.iter().find(|l| l.name == target_name).unwrap().id;
        let store = f
            .blocks
            .iter_mut()
            .flat_map(|b| &mut b.instructions)
            .filter(|i| matches!(i.kind,zil_ir::InstructionKind::Store{local,..}if local==spare))
            .last()
            .unwrap();
        let zil_ir::InstructionKind::Store { local, .. } = &mut store.kind else {
            unreachable!()
        };
        *local = selected;
        let analyzed = zil_analyze::analyze(&bad, &limits).unwrap_err();
        assert!(
            analyzed
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{target_name}: {analyzed:?}"
        );
        let mut candidate = prior.clone();
        candidate.module_fingerprint = bad.fingerprint();
        let errors = zil_verify::verify_module(bad.clone(), candidate.clone(), &target, &limits)
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{target_name}: {errors:?}"
        );
        let wire = zil_ir::wire::encode_module_candidate(&bad, &candidate, &limits).unwrap();
        let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|d| d.message.contains("live incompatible loan")),
            "{target_name}: {errors:?}"
        );
        println!(
            "dynamic alternative {target_name}: actual live-loan rejection in Analyze/Verify/Wire"
        );
    }
}

#[test]
fn index_side_effects_cannot_write_a_snapshot_referent() {
    for target in ["first", "second"] {
        let program = format!(
            "fn next(counter:&mut i32)->i32{{*counter=11;return 1}}fn main()->i32{{var first=7;var second=9;let items=[&first,&second];let pointer=items[next(&mut {target})];return *pointer-9}}"
        );
        let report = compile_source(
            &source("snapshot-index-gap", &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("actual stored target/parent loan")),
            "{}",
            report.to_json()
        );
    }
}

#[test]
fn each_integer_index_type_uses_the_actual_shared_array() {
    for suffix in ["u8", "u16", "i32", "i64", "u64", "usize"] {
        let program = format!(
            "fn main()->i32{{let first=7;let second=9;let items=[&first,&second];let index=1{suffix};return *items[index]-9}}"
        );
        verify_source(suffix, &program);
    }
}
