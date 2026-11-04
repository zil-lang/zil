//! Actual ordinary-source standard library consumers and import trust boundaries.
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, NativeBackend, Optimization, ProjectSources, Stage,
};
const ENTRY: &str = include_str!("utf8.zil");
const UTF8: &str = include_str!("../../../../lib/std/utf8.zil");
const EDIT: &str = include_str!("../../../../lib/std/text_edit.zil");
fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!("terminal345-{label}-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}
fn project() -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(FileId(0), ModuleId(0), "main.zil", ENTRY),
            SourceUnit::new(FileId(1), ModuleId(1), "<std.utf8>", UTF8),
            SourceUnit::new(FileId(2), ModuleId(2), "<std.text_edit>", EDIT),
        ],
        paths: BTreeMap::from([
            (vec!["std".into(), "utf8".into()], ModuleId(1)),
            (vec!["std".into(), "text_edit".into()], ModuleId(2)),
        ]),
    }
}
#[test]
fn ordinary_library_native_and_filesystem_hijack_control() {
    let mut backends = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        backends.push(NativeBackend::LlvmApi);
    }
    for backend in backends {
        for optimization in [Optimization::O0, Optimization::O2] {
            let path = directory(&format!("native-{backend:?}-{optimization:?}"));
            fs::write(path.join("main.zil"), ENTRY).unwrap();
            fs::create_dir_all(path.join("std")).unwrap();
            fs::write(path.join("std/utf8.zil"), "not valid ZIL; must never load").unwrap();
            let report = CompilationSession::default().compile_path(
                &path.join("main.zil"),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(path.join("program")),
                    ..CompileOptions::default()
                },
            );
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.source_files.len(), 3);
            assert_eq!(report.verification_runs, 1);
            assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
            let out = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert_eq!(
                out.status.code(),
                Some(0),
                "{backend:?}/{optimization:?}: {out:?}"
            );
            assert!(out.stdout.is_empty() && out.stderr.is_empty());
        }
    }
}
#[test]
fn ordinary_library_source_identity_and_private_helpers() {
    let options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let mut p = project();
    let mut session = CompilationSession::default();
    let positive = session.compile_project(&p, &options);
    assert_eq!(
        positive.status,
        ResultCategory::Success,
        "{}",
        positive.to_json()
    );
    p.modules[1]
        .text
        .push_str("\n// even an innocuous changed source is a different reserved library\n");
    let forged = session.compile_project(&p, &options);
    assert_eq!(
        forged.status,
        ResultCategory::SafeReject,
        "{}",
        forged.to_json()
    );
    assert!(
        forged
            .diagnostics
            .iter()
            .any(|d| d.code == "E-STD-RESERVED")
    );
    let mut private = project();
    private.modules[0].text =
        "import std.utf8\nfn main()->i32{let b=[65u8];return i32(utf8.next(&b[..],0))}".into();
    let denied = session.compile_project(&private, &options);
    assert_eq!(
        denied.status,
        ResultCategory::SafeReject,
        "{}",
        denied.to_json()
    );
    assert!(!denied.artifacts.contains_key("executable"));
}
#[test]
fn ordinary_library_source_budget_fresh_and_warm() {
    let path = directory("budget");
    fs::write(path.join("main.zil"), ENTRY).unwrap();
    let mut options = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };
    let n = ENTRY.len() + UTF8.len() + EDIT.len();
    options.limits.max_source_bytes = n;
    let mut session = CompilationSession::default();
    for _ in 0..2 {
        let r = session.compile_path(&path.join("main.zil"), &options);
        assert_eq!(r.status, ResultCategory::Success, "N={n}: {}", r.to_json());
        assert_eq!(r.verification_runs, 1);
    }
    options.limits.max_source_bytes = n - 1;
    for mut current in [session, CompilationSession::default()] {
        let r = current.compile_path(&path.join("main.zil"), &options);
        assert_eq!(
            r.status,
            ResultCategory::ResourceLimit,
            "N-1={}: {}",
            n - 1,
            r.to_json()
        );
        assert!(!r.artifacts.contains_key("executable"));
    }
}
