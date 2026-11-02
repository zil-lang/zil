//! Author regression for the actual 244 finite five-module type graph.
//! This only builds; 244 owns its independent execution and allocation oracle.
use std::{fs, path::PathBuf};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
};

#[test]
fn impl243_default_budget_builds_frozen_five_module_sum_cleanup_graph() {
    let names = ["main", "portable", "model", "storage", "state"];
    let text = [
        include_str!("../../../tests/impl244/project/main.zil"),
        include_str!("../../../tests/impl244/project/portable.zil"),
        include_str!("../../../tests/impl244/project/model.zil"),
        include_str!("../../../tests/impl244/project/storage.zil"),
        include_str!("../../../tests/impl244/project/state.zil"),
    ];
    let project = ProjectSources {
        root: ModuleId(0),
        modules: names
            .iter()
            .zip(text)
            .enumerate()
            .map(|(i, (name, source))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    source,
                )
            })
            .collect(),
        paths: names
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, name)| (vec![(*name).into()], ModuleId(i as u32)))
            .collect(),
    };
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        let dir = base.join(format!(
            "impl243-default244-{backend:?}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        for (name, source) in names.iter().zip(text) {
            fs::write(dir.join(format!("{name}.zil")), source).unwrap();
        }
        let report = compile_project(
            &project,
            &CompileOptions {
                backend,
                optimization: Optimization::O0,
                stop_after: Stage::Build,
                emit_dir: Some(dir.join("phases")),
                output: Some(dir.join("program")),
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
        let native = report.native_codegen.as_ref().unwrap();
        assert_eq!(native.backend, backend);
        assert!(native.fallback.is_none());
        if backend == NativeBackend::LlvmApi {
            assert_eq!(native.object_reports.len(), 1);
        }
    }
}
