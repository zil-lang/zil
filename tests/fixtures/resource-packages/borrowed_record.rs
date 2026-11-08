//! Complete owning carriers retain the actual targets of an inline Record.
use super::*;

const NORMAL: &str = include_str!("owned-package-borrowed-record.zil");
const MAPPING: &str = include_str!("qualified-record-class-mapping.zil");
const FAILURE: &str = include_str!("owned-package-borrowed-record-failure.zil");

#[test]
fn qualified_record_source_raw_and_wire() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (name, program) in [
        ("record-normal", NORMAL),
        ("record-failure", FAILURE),
        ("record-mapping", MAPPING),
    ] {
        let report = compile_source(
            &source(name, program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = typed_source(name, program, &target, &limits);
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
}

#[test]
fn qualified_record_source_preserves_external_loan_and_rejects_local_escape() {
    for (label, program) in [
        (
            "live source overwrite",
            NORMAL.replacen(
                "if read(&moved)!=7",
                "value=8\n        if read(&moved)!=7",
                1,
            ),
        ),
        (
            "live Record window prevents parent movement",
            NORMAL.replacen("if read(&moved)!=7", "let child=&moved.refs\n        let again=relay(moved)\n        if *child.pointer!=7", 1),
        ),
        (
            "local source escape",
            NORMAL
                .replacen(
                    "var bytes=bytes_new()",
                    "let local=11\n    var bytes=bytes_new()",
                    1,
                )
                .replacen(
                    "refs:Refs::<'a>{pointer:pointer}",
                    "refs:Refs::<'a>{pointer:&local}",
                    1,
                ),
        ),
        (
            "wrong result lifetime",
            NORMAL.replacen(
                "fn relay<life a>(job:Job<'a>)->Job<'a>",
                "fn relay<life a,life b>(job:Job<'a>)->Job<'b>",
                1,
            ),
        ),
    ] {
        assert_ne!(program, NORMAL);
        let report = compile_source(
            &source(label, &program),
            &CompileOptions {
                stop_after: Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn qualified_record_raw_and_wire_reject_wrong_slot_mapping() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let original = typed_source("record-mapping", NORMAL, &target, &limits);
    let plan = zil_analyze::analyze(&original, &limits).unwrap();
    for wrong_class in [false, true] {
        let mut raw = original.clone();
        let mut changed = 0;
        for f in &mut raw.functions {
            for ty in f
                .locals
                .iter_mut()
                .map(|l| &mut l.ty)
                .chain(&mut f.values)
                .chain(std::iter::once(&mut f.return_type))
            {
                let Type::BorrowedStorage { storage, life } = ty else {
                    continue;
                };
                if !matches!(storage.as_ref(), Type::Package { .. }) {
                    continue;
                }
                assert_eq!(life.slots.len(), 1);
                assert_eq!(life.retained.len(), 1);
                if wrong_class {
                    life.retained[0].binder = life.binder_count;
                } else {
                    life.slots[0].path = vec![zil_ir::PlaceProjection::Field(0)];
                }
                changed += 1;
            }
        }
        assert!(changed > 0);
        assert!(zil_analyze::analyze(&raw, &limits).is_err());
        assert!(zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err());
        let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
    }
}

#[cfg(unix)]
#[test]
fn qualified_record_native_cleanup_on_normal_and_error_exit() {
    use std::os::unix::fs::PermissionsExt;
    let clang = std::env::var_os("ZIL_CLANG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "clang".into());
    let shim = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/composed-store-closures/allocator_probe.c");
    let quote = |p: &std::path::Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        for optimization in [Optimization::O0, Optimization::O2] {
            for (label, program, stdout, allocations) in [
                ("normal", NORMAL, b"R".as_slice(), 1),
                ("error", FAILURE, b"F".as_slice(), 1),
                ("mapping", MAPPING, b"".as_slice(), 0),
            ] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-borrowed-record-{}-{backend:?}-{optimization:?}-{label}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let wrapper = dir.join("probe-clang.sh");
                std::fs::write(
                    &wrapper,
                    format!(
                        "#!/bin/sh\nexec {} \"$@\" -x c {} -Wl,--wrap=realloc -Wl,--wrap=free\n",
                        quote(&clang),
                        quote(&shim)
                    ),
                )
                .unwrap();
                std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
                let executable = dir.join("program");
                let report = compile_source(
                    &source(label, program),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        clang: Some(wrapper),
                        output: Some(executable.clone()),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                let codegen = report.native_codegen.as_ref().unwrap();
                assert_eq!(codegen.backend, backend);
                assert!(codegen.fallback.is_none());
                let run = std::process::Command::new(executable)
                    .env("ZIL_TEST_FAIL_AT", "0")
                    .output()
                    .unwrap();
                assert_eq!(
                    run.status.code(),
                    Some(0),
                    "{backend:?}/{optimization:?}/{label}: {run:?}"
                );
                assert_eq!(run.stdout, stdout);
                let stats = String::from_utf8(run.stderr).unwrap();
                for field in [
                    format!("\"allocated\":{allocations}"),
                    format!("\"frees\":{allocations}"),
                    "\"live\":0".into(),
                    "\"failures\":0".into(),
                ] {
                    assert!(
                        stats.contains(&field),
                        "{backend:?}/{optimization:?}/{label}: {stats}"
                    );
                }
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn qualified_record_raw_and_wire_cannot_return_an_activation_local_target() {
    use zil_ir::InstructionKind;
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let program = NORMAL.replacen(
        "var bytes=bytes_new()",
        "let decoy=11\n    let unused=&decoy\n    var bytes=bytes_new()",
        1,
    );
    let mut raw = typed_source("record-local-forgery", &program, &target, &limits);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
    let f = raw.functions.iter_mut().find(|f| f.name == "make").unwrap();
    let decoy = f.locals.iter().find(|l| l.name == "decoy").unwrap().id;
    let address = f
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            matches!(i.kind, InstructionKind::BorrowLocal { local } if local == decoy)
                .then_some(i.result)
                .flatten()
        })
        .unwrap();
    let mut changed = 0;
    for i in f.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
        if let InstructionKind::MakeAggregate { fields } = &mut i.kind {
            let output = i.result.unwrap();
            if matches!(&f.values[output.0 as usize], Type::BorrowedStorage { storage, .. } if matches!(storage.as_ref(), Type::Record { .. }))
            {
                assert_eq!(fields.len(), 1);
                fields[0] = address;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    assert!(zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err());
    let bytes = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&bytes, &target, &limits).is_err());
}

#[test]
fn qualified_record_child_does_not_gain_an_exclusive_window() {
    let program = NORMAL.replacen(
        "fn main()->i32{",
        "fn mutate<life a>(job:&mut Job<'a>){let child=&mut job.refs}\nfn main()->i32{",
        1,
    );
    let report = compile_source(
        &source("record-exclusive-child", &program),
        &CompileOptions {
            stop_after: Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
}

#[test]
fn qualified_record_raw_and_wire_reject_an_in_range_child_class_swap() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    let mut raw = typed_source("record-class-swap", MAPPING, &target, &limits);
    let plan = zil_analyze::analyze(&raw, &limits).unwrap();
    let mut changed = 0;
    for f in &mut raw.functions {
        for ty in f
            .locals
            .iter_mut()
            .map(|l| &mut l.ty)
            .chain(&mut f.values)
            .chain(std::iter::once(&mut f.return_type))
        {
            if let Type::BorrowedStorage { storage, life } = ty
                && matches!(storage.as_ref(), Type::Package { .. })
            {
                assert_eq!(life.binder_count, 2);
                assert_eq!(life.retained.len(), 2);
                life.retained[0].binder = 1 - life.retained[0].binder;
                changed += 1;
            }
        }
    }
    assert!(changed > 0);
    assert!(zil_analyze::analyze(&raw, &limits).is_err());
    assert!(zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).is_err());
    let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
    assert!(zil_verify::verify_module_bytes(&wire, &target, &limits).is_err());
}
