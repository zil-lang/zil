//! Actual semantic edits through one warm multi-module session, not unused declarations.
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, Optimization, ProjectSources, Stage};

fn project(value: i32, reverse: bool) -> ProjectSources {
    let mut modules = vec![
        SourceUnit::new(
            FileId(0),
            ModuleId(0),
            "main.zil",
            "import lib\nfn main()->i32{return lib.value()}",
        ),
        SourceUnit::new(
            FileId(1),
            ModuleId(1),
            "lib.zil",
            format!("fn private_value()->i32=>{value}\npub fn value()->i32=>private_value()"),
        ),
    ];
    if reverse {
        modules.reverse()
    }
    ProjectSources {
        root: ModuleId(0),
        modules,
        paths: BTreeMap::from([(vec!["lib".into()], ModuleId(1))]),
    }
}
#[test]
fn warm_session_called_body_edits_reordering_and_recovery_execute_new_code() {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("warm153-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    for opt in [Optimization::O0, Optimization::O2] {
        let mut session = CompilationSession::with_capacity(1024);
        for (step, (value, reverse)) in [
            (2, false),
            (2, false),
            (7, false),
            (7, true),
            (11, true),
            (2, false),
        ]
        .into_iter()
        .enumerate()
        {
            let d = root.join(format!("{opt:?}-{step}"));
            fs::create_dir_all(&d).unwrap();
            let sources = project(value, reverse);
            for source in &sources.modules {
                fs::write(d.join(&source.path), &source.text).unwrap()
            }
            let executable = d.join("program");
            let options = CompileOptions {
                stop_after: Stage::Build,
                optimization: opt,
                output: Some(executable.clone()),
                emit_dir: Some(d.join("phases")),
                ..CompileOptions::default()
            };
            let r = session.compile_project(&sources, &options);
            fs::write(d.join("compile.json"), r.to_json()).unwrap();
            assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
            assert_eq!(r.verification_runs, 1);
            assert_eq!(session.active_query_count(), 0);
            if step == 1 {
                assert!(r.cache_hit)
            }
            let actual = Command::new(executable).output().unwrap();
            fs::write(d.join("stdout"), &actual.stdout).unwrap();
            fs::write(d.join("stderr"), &actual.stderr).unwrap();
            assert_eq!(actual.status.code(), Some(value));
            if step == 3 {
                let mut invalid = project(7, false);
                invalid.modules[1] = SourceUnit::new(
                    FileId(1),
                    ModuleId(1),
                    "lib.zil",
                    "pub fn value()->bool=>true",
                );
                let rejected = session.compile_project(&invalid, &CompileOptions::default());
                fs::write(d.join("rejected-signature.json"), rejected.to_json()).unwrap();
                assert_eq!(
                    rejected.status,
                    ResultCategory::SafeReject,
                    "{}",
                    rejected.to_json()
                );
                assert_eq!(session.active_query_count(), 0);
            }
        }
    }
}
