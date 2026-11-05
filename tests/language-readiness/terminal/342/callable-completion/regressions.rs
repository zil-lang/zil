use super::*;
const CASES: [(&str, &str); 3] = [
    ("before", include_str!("loop-before.zil")),
    ("after", include_str!("loop-after.zil")),
    ("nested-transfer", include_str!("loop-transfer.zil")),
];
#[test]
fn g339_source_loop_origins_raw_wire() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (name, text) in CASES {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        eprintln!("G339 {name}: actual Source/raw/Wire success");
    }
    let report = compile_source(
        &source(include_str!("loop-double.zil")),
        &CompileOptions {
            stop_after: zil_driver::Stage::Verify,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("generic loop must preserve"))
    );
}

#[test]
fn g339_multi_role_user_drop_and_managed_give_source_raw_wire() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (name, text) in [
        ("single-user-drop", include_str!("single-user-drop.zil")),
        ("two-role-no-drop", include_str!("two-role-no-drop.zil")),
        ("two-role-user-drop", include_str!("two-role-user-drop.zil")),
        ("managed-companion", include_str!("managed-companion.zil")),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let raw = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        let plan = zil_analyze::analyze(&raw, &limits).unwrap();
        zil_verify::verify_module(raw.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&raw, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        eprintln!("G339 {name}: actual Source/raw/Wire success");
    }
}

#[test]
fn g339_qualified_environment_lending_actual_source_raw_wire() {
    let limits = Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    for (name, text) in [
        ("lending", include_str!("qualified-lending.zil")),
        ("lending-mut", include_str!("qualified-lending-mut.zil")),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let typed = zil_types::check(&g334_core(text), &target, &limits).unwrap();
        assert!(
            typed
                .closures
                .iter()
                .any(|d| d.storage_life.is_some() && !d.lending_captures.is_empty())
        );
        let plan = zil_analyze::analyze(&typed, &limits).unwrap();
        zil_verify::verify_module(typed.clone(), plan.clone(), &target, &limits).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &limits).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &limits).unwrap();
        eprintln!("G339 {name}: actual Source/raw/Wire success");
    }
}

#[test]
#[cfg(target_os = "linux")]
fn g339_actual_completion_native_clang_api_o0_o2() {
    use zil_driver::{NativeBackend, Optimization, Stage};
    for (case, text, expected) in [
        ("before", include_str!("loop-before.zil"), "45\n"),
        ("after", include_str!("loop-after.zil"), "45\n"),
        ("nested", include_str!("loop-transfer.zil"), "45\n"),
        (
            "drop-order",
            include_str!("two-role-user-drop.zil"),
            "ABBA91\n0\n",
        ),
        (
            "give-companion",
            include_str!("managed-companion.zil"),
            "45\n",
        ),
        ("lending", include_str!("qualified-lending.zil"), "0\n"),
        (
            "lending-mut",
            include_str!("qualified-lending-mut.zil"),
            "0\n",
        ),
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = std::env::temp_dir().join(format!(
                    "zil-g339-{}-{case}-{backend:?}-{optimization:?}",
                    std::process::id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                let report = compile_source(
                    &source(text),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        entry: Some("main".into()),
                        output: Some(dir.join("program")),
                        emit_dir: Some(dir.join("ir")),
                        ..CompileOptions::default()
                    },
                );
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{case}: {}",
                    report.to_json()
                );
                let native = report.native_codegen.as_ref().unwrap();
                assert_eq!(native.backend, backend);
                assert!(native.fallback.is_none());
                let run = std::process::Command::new(&report.artifacts["executable"])
                    .output()
                    .unwrap();
                assert!(run.status.success(), "{run:?}");
                assert_eq!(
                    run.stdout,
                    expected.as_bytes(),
                    "{case}/{backend:?}/{optimization:?}"
                );
                assert!(run.stderr.is_empty());
                eprintln!(
                    "G339 native {case}/{backend:?}/{optimization:?}: {:?}",
                    String::from_utf8_lossy(&run.stdout)
                );
            }
        }
    }
}

#[test]
fn g339_environment_lending_nearest_live_receiver_rejections() {
    for (name, text) in [
        ("move-live", include_str!("lending-move-live.zil")),
        ("exclusive-call-live", include_str!("lending-mut-live.zil")),
    ] {
        let report = compile_source(
            &source(text),
            &CompileOptions {
                stop_after: zil_driver::Stage::Verify,
                ..CompileOptions::default()
            },
        );
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{name}: {}",
            report.to_json()
        );
        assert_ne!(
            report.status,
            ResultCategory::ResourceLimit,
            "{name}: {}",
            report.to_json()
        );
        eprintln!("G339 {name} nearest rejection: {}", report.to_json());
    }
}
