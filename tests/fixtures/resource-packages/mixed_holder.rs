//! A real mixed Package keeps resource responsibility and an external loan.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, Stage, compile_path, compile_source,
};

const PROGRAM: &str = include_str!("mixed-owned-borrowed-package.zil");
const TWO_LEAF: &str = include_str!("two-leaf-stored-borrow.zil");
const OWNED_STORE_HEAD: &str = include_str!("owned-store-package-head.zil");
const LOCAL_EXCLUSIVE_SLOT: &str = include_str!("local-exclusive-package-slot.zil");
const WHOLE_TARGET_REPLACEMENT: &str = include_str!("whole-package-target-replacement.zil");
const MULTI_LEAF_PACKAGE: &str = include_str!("multi-leaf-mixed-package.zil");
const SLOT_REPLACEMENT: &str = include_str!("mutable-package-slot-replacement.zil");
const LOCAL_SLOT_REPLACEMENT: &str = include_str!("local-package-slot-replacement.zil");
const CONDITIONAL_SLOT_REPLACEMENT: &str = include_str!("conditional-package-slot-replacement.zil");

fn stdlib_store_head_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/resource-packages/stdlib-owned-store-package-head.zil")
}

fn source(name: &str, program: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), name, program)
}

#[test]
fn mixed_package_source_raw_and_wire() {
    let limits = Limits::default();
    let target = TargetProfile::host_llvm();
    for (name, program) in [
        ("mixed-owned-borrowed-package.zil", PROGRAM),
        ("two-leaf-stored-borrow.zil", TWO_LEAF),
        ("owned-store-package-head.zil", OWNED_STORE_HEAD),
        ("local-exclusive-package-slot.zil", LOCAL_EXCLUSIVE_SLOT),
        (
            "whole-package-target-replacement.zil",
            WHOLE_TARGET_REPLACEMENT,
        ),
        ("multi-leaf-mixed-package.zil", MULTI_LEAF_PACKAGE),
        ("mutable-package-slot-replacement.zil", SLOT_REPLACEMENT),
        ("local-package-slot-replacement.zil", LOCAL_SLOT_REPLACEMENT),
    ] {
        let source = source(name, program);
        let report = compile_source(
            &source,
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

        let syntax = zil_syntax::parse(&source).into_result().unwrap();
        let core = zil_expand::expand_with_map_limits_target_context(
            &source,
            &syntax,
            &limits,
            &target,
            zil_expand::source_context::SourceIdentityContext::ClosedSnapshot {
                root: source.module_id,
                aliases: &Default::default(),
                declarations: zil_expand::source_context::DeclarationPlacement::Original,
            },
        )
        .unwrap()
        .core;
        let raw = zil_types::check(&core, &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
    }
}

#[test]
#[cfg(target_os = "linux")]
fn mixed_package_native_both_optimizations() {
    for (name, program, expected) in [
        (
            "mixed-owned-borrowed-package.zil",
            PROGRAM,
            b"42\n".as_slice(),
        ),
        ("two-leaf-stored-borrow.zil", TWO_LEAF, b"42\n".as_slice()),
        (
            "owned-store-package-head.zil",
            OWNED_STORE_HEAD,
            b"1\n".as_slice(),
        ),
        (
            "local-exclusive-package-slot.zil",
            LOCAL_EXCLUSIVE_SLOT,
            b"0\n".as_slice(),
        ),
        (
            "whole-package-target-replacement.zil",
            WHOLE_TARGET_REPLACEMENT,
            b"0\n".as_slice(),
        ),
        (
            "multi-leaf-mixed-package.zil",
            MULTI_LEAF_PACKAGE,
            b"0\n".as_slice(),
        ),
        (
            "mutable-package-slot-replacement.zil",
            SLOT_REPLACEMENT,
            b"0\n".as_slice(),
        ),
        (
            "local-package-slot-replacement.zil",
            LOCAL_SLOT_REPLACEMENT,
            b"0\n".as_slice(),
        ),
    ] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let output = std::env::temp_dir().join(format!(
                "zil-mixed-package-{}-{name}-{optimization:?}",
                std::process::id()
            ));
            let report = compile_source(
                &source(name, program),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend: NativeBackend::Clang,
                    optimization,
                    entry: Some("main".into()),
                    output: Some(output.clone()),
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
            let output_result = std::process::Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(output_result.status.success());
            assert_eq!(output_result.stdout, expected);
            std::fs::remove_file(output).unwrap();
        }
    }
    for optimization in [Optimization::O0, Optimization::O2] {
        let output = std::env::temp_dir().join(format!(
            "zil-stdlib-store-head-{}-{optimization:?}",
            std::process::id()
        ));
        let report = compile_path(
            &stdlib_store_head_path(),
            &CompileOptions {
                stop_after: Stage::Build,
                backend: NativeBackend::Clang,
                optimization,
                entry: Some("main".into()),
                output: Some(output.clone()),
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
        let output_result = std::process::Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert!(output_result.status.success());
        assert_eq!(output_result.stdout, b"1\n");
        std::fs::remove_file(output).unwrap();
    }
}

#[test]
fn stdlib_owned_store_head_verifies() {
    let report = compile_path(
        &stdlib_store_head_path(),
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
}

#[test]
fn same_stored_leaf_and_same_exclusive_input_reject() {
    for (before, after) in [
        ("let right=pair.right;", "let right=pair.left;"),
        ("hold(&mut left,&mut right)", "hold(&mut left,&mut left)"),
    ] {
        assert_eq!(TWO_LEAF.matches(before).count(), 1);
        let changed = TWO_LEAF.replacen(before, after, 1);
        let report = compile_source(
            &source("overlapping-stored-borrow.zil", &changed),
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
    }
    let overlapping = LOCAL_EXCLUSIVE_SLOT
        .replace(
            "let pointer=session.pointer",
            "let pointer=session.pointer\n        let second=session.pointer",
        )
        .replace(
            "*pointer=*pointer+1",
            "*pointer=*pointer+1\n        *second=*second+1",
        );
    let report = compile_source(
        &source("overlapping-local-package-slot.zil", &overlapping),
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
    let aliased = MULTI_LEAF_PACKAGE.replace("make(&mut a,&mut b)", "make(&mut a,&mut a)");
    let report = compile_source(
        &source("aliased-multi-leaf-package.zil", &aliased),
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
    for (before, after) in [
        (
            "relay(&mut session,&mut next)",
            "let old_child=session.pointer\n        relay(&mut session,&mut next)\n        *old_child=*old_child+1",
        ),
        (
            "relay(&mut session,&mut next)",
            "relay(&mut session,&mut old)",
        ),
    ] {
        let changed = SLOT_REPLACEMENT.replace(before, after);
        let report = compile_source(
            &source("conflicting-package-slot-replacement.zil", &changed),
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
    }
    let live_child = LOCAL_SLOT_REPLACEMENT.replace(
        "session.pointer=&mut next",
        "let child=session.pointer\n        session.pointer=&mut next\n        *child=*child+1",
    );
    let report = compile_source(
        &source("local-replacement-with-live-child.zil", &live_child),
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
    let report = compile_source(
        &source(
            "conditional-package-slot-replacement.zil",
            CONDITIONAL_SLOT_REPLACEMENT,
        ),
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
}
