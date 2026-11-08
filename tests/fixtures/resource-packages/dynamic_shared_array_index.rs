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
            "fn main()->i32{{let first=7;let second=9;let items=[&first,&second];let index:{suffix}=1;return *items[index]-9}}"
        );
        verify_source(suffix, &program);
    }
}

fn snapshot_then_dynamic_extract(refill: bool) -> zil_ir::TypedModule {
    let body = "let initial=&left;let unused=initial.items[0];let selected=if flag{&left}else{&right};let snapshot=selected.items;return *snapshot[1]";
    let mut raw = super::shared_array_snapshot_initialization::local_snapshot_body(body, refill);
    let f = raw.functions.iter_mut().find(|f| f.name == "read").unwrap();
    let aggregate = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find(|i| {
            matches!(i.kind, zil_ir::InstructionKind::ReadAddress { .. })
                && i.result
                    .is_some_and(|v| matches!(f.values[v.0 as usize], Type::Array { .. }))
        })
        .unwrap()
        .result
        .unwrap();
    let (block,at)=f.blocks.iter().enumerate().find_map(|(b,block)|block.instructions.iter().rposition(|i|matches!(&i.kind,zil_ir::InstructionKind::LoadStoredBorrow{path,mode:zil_ir::StoredBorrowLoad::CopyShared,..}if path==&[zil_ir::PlaceProjection::Field(1)])).map(|i|(b,i))).unwrap();
    let index = zil_ir::ValueId(f.values.len() as u32);
    f.values.push(Type::I32);
    let span = f.blocks[block].instructions[at].span;
    f.blocks[block].instructions[at].kind =
        zil_ir::InstructionKind::ExtractIndex { aggregate, index };
    f.blocks[block].instructions.insert(
        at,
        zil_ir::Instruction {
            result: Some(index),
            kind: zil_ir::InstructionKind::Constant(zil_ir::Constant::I32(1)),
            span,
        },
    );
    raw
}

#[test]
fn dynamic_extract_cannot_hide_an_incomplete_whole_snapshot() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let good = snapshot_then_dynamic_extract(true);
    let plan = zil_analyze::analyze(&good, &limits).unwrap();
    zil_verify::verify_module(good.clone(), plan.clone(), &target, &limits).unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&good, &plan, &limits).unwrap();
    zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    let bad = snapshot_then_dynamic_extract(false);
    let errors = zil_analyze::analyze(&bad, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{errors:?}"
    );
    let mut prior = plan;
    prior.module_fingerprint = bad.fingerprint();
    let errors =
        zil_verify::verify_module(bad.clone(), prior.clone(), &target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{errors:?}"
    );
    let wire = zil_ir::wire::encode_module_candidate(&bad, &prior, &limits).unwrap();
    let errors = zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|d| d.message.contains("Array snapshot reads a vacant slot")),
        "{errors:?}"
    );
    println!(
        "dynamic Extract after partial two-root snapshot: Analyze/Verify/Wire actual vacancy rejection"
    );
}

#[test]
fn native_object_and_index_evaluate_once_in_order_and_bounds_trap() {
    use std::os::unix::process::ExitStatusExt;
    let ordered = "extern \"C\" fn putchar(c:i32)->i32;fn make<life a>(first:&'a i32,second:&'a i32)->[&'a i32;2]{unsafe{putchar(65)}return [first,second]}fn next(counter:&mut i32)->i32{*counter=*counter+1;unsafe{putchar(73)}return 1}fn main()->i32{let first=7;let second=9;var counter=0;let pointer=make(&first,&second)[next(&mut counter)];if counter!=1{return 1}return *pointer-9}";
    let mut runs = 0;
    let mut traps = 0;
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
            continue;
        }
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label,program) in [("ordered",ordered.to_string()),("negative","fn main()->i32{let first=7;let second=9;let items=[&first,&second];let index=-1;return *items[index]}".to_string()),("past-end","fn main()->i32{let first=7;let second=9;let items=[&first,&second];let index=2;return *items[index]}".to_string())] {
                let dir=std::env::temp_dir().join(format!("zil-dynamic-shared-array-{}-{backend:?}-{optimization:?}-{label}",std::process::id()));std::fs::create_dir_all(&dir).unwrap();
                let executable=dir.join("program");
                let report=compile_source(&source(label,&program),&CompileOptions{stop_after:Stage::Build,backend,optimization,output:Some(executable.clone()),..CompileOptions::default()});
                assert_eq!(report.status,ResultCategory::Success,"{label}: {}",report.to_json());
                assert_eq!(report.verification_runs,1);
                let native=report.native_codegen.as_ref().unwrap();assert_eq!(native.backend,backend);assert!(native.fallback.is_none());
                let result=std::process::Command::new(&executable).output().unwrap();
                if label=="ordered" { assert_eq!(result.status.code(),Some(0),"{result:?}");assert_eq!(result.stdout,b"AI");assert!(result.stderr.is_empty());runs+=1; }
                else { assert_eq!(result.status.signal(),Some(4),"{label}: {result:?}");assert!(result.stdout.is_empty());traps+=1; }
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
    assert_eq!(runs, 2 + 2 * usize::from(cfg!(feature = "llvm-api")));
    assert_eq!(traps, 2 * runs);
    println!(
        "dynamic Array: {runs} ordered once-only native successes and {traps} actual bounds traps"
    );
}
