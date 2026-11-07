//! A real mixed Package keeps resource responsibility and an external loan.
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

const PROGRAM: &str = include_str!("mixed-owned-borrowed-package.zil");
const TWO_LEAF: &str = include_str!("two-leaf-stored-borrow.zil");

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
    for (name, program) in [
        ("mixed-owned-borrowed-package.zil", PROGRAM),
        ("two-leaf-stored-borrow.zil", TWO_LEAF),
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
            assert_eq!(output_result.stdout, b"42\n");
            std::fs::remove_file(output).unwrap();
        }
    }
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
}
