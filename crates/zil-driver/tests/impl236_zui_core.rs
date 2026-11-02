//! Real five-module consumer of 227+235. Author checks, not independent review.
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, NativeBackend, Optimization, ProjectSources, Stage,
};

const FILES: [(&str, &str); 5] = [
    (
        "main",
        include_str!("../../../examples/mainline/zui_core236/main.zil"),
    ),
    (
        "portable",
        include_str!("../../../examples/mainline/zui_core236/portable.zil"),
    ),
    (
        "model",
        include_str!("../../../examples/mainline/zui_core236/model.zil"),
    ),
    (
        "collections",
        include_str!("../../../examples/mainline/zui_core236/collections.zil"),
    ),
    (
        "session",
        include_str!("../../../examples/mainline/zui_core236/session.zil"),
    ),
];

fn project() -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: FILES
            .iter()
            .enumerate()
            .map(|(index, (name, text))| {
                SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: FILES
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, (name, _))| (vec![(*name).to_owned()], ModuleId(index as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn directory(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!("consumer236-{label}-{}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    for (name, text) in FILES {
        fs::write(path.join(format!("{name}.zil")), text).unwrap();
    }
    path
}

#[test]
fn complete_session_reaches_independent_verify() {
    let path = directory("verify");
    let report = CompilationSession::default().compile_project(
        &project(),
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    assert_eq!(report.source_files.len(), 5);
}

#[test]
fn runtime_counts_exit_modes_and_declared_shutdown_order() {
    for optimization in [Optimization::O0, Optimization::O2] {
        let path = directory(&format!("native-{optimization:?}"));
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = CompilationSession::default().compile_project(
            &project(),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                backend,
                output: Some(path.join("program")),
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("report.json"), report.to_json()).unwrap();
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
        for count in 0..4 {
            let nodes: String = (0..count).map(|i| char::from(b'A' + i)).collect();
            let old: String = (0..count).map(|i| char::from(b'a' + i)).collect();
            let replacement: String = (0..count).map(|i| char::from(b'm' + i)).collect();
            for (mode, middle, final_subscriptions, exit) in [
                (0, "N".to_owned(), old.clone(), 0),
                (1, "E".to_owned(), old.clone(), 0),
                (2, "F".to_owned(), old.clone(), 1),
                (3, "ixixixN".to_owned(), old.clone(), 0),
                (4, format!("{old}RN"), replacement, 0),
            ] {
                let input = format!("{count}{mode}");
                let expected = format!("S{middle}LQ{final_subscriptions}{nodes}");
                let mut child = Command::new(&report.artifacts["executable"])
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(input.as_bytes())
                    .unwrap();
                let output = child.wait_with_output().unwrap();
                fs::write(path.join(format!("{input}.stdout")), &output.stdout).unwrap();
                fs::write(path.join(format!("{input}.stderr")), &output.stderr).unwrap();
                assert_eq!(output.status.code(), Some(exit), "{input}: {output:?}");
                assert_eq!(output.stdout, expected.as_bytes(), "{input}: {output:?}");
                assert!(output.stderr.is_empty(), "{input}: {output:?}");
            }
        }
    }
}

#[test]
fn nearest_missing_refill_and_live_window_counterexamples() {
    // Keep this ownership boundary independent of unrelated defer lowering.
    // The valid five-module control must pass before either rejection counts.
    const ENTRY: &str = "import portable\nimport model\nimport collections\nimport session\nfn main()->i32{return 0}\n";
    let path = directory("counterexample-control");
    let mut control = project();
    control.modules[0].text = ENTRY.to_owned();
    fs::write(path.join("main.zil"), ENTRY).unwrap();
    let report = CompilationSession::default().compile_project(
        &control,
        &CompileOptions {
            stop_after: Stage::Verify,
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for (label, session_source, message) in [
        (
            "missing-refill",
            FILES[4].1.replacen("    current.nodes = node_set\n", "", 1),
            "resource closure: whole aggregate move requires every owned element or field live",
        ),
        (
            "live-window",
            format!(
                "{}\nfn held_then_move(owned: Session) -> Session {{\nvar current = owned\nlet child = current.nodes\nlet held = store_at(child.items, 0)\ncurrent.nodes = child\nportable.emit(held.id)\nreturn current\n}}\n",
                FILES[4].1
            ),
            "captured Store transition invalidates a live element window",
        ),
    ] {
        let path = directory(label);
        let mut input = project();
        input.modules[0].text = ENTRY.to_owned();
        input.modules[4].text = session_source;
        fs::write(path.join("main.zil"), ENTRY).unwrap();
        fs::write(path.join("session.zil"), &input.modules[4].text).unwrap();
        let report = CompilationSession::default().compile_project(
            &input,
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Analyze, "{}", report.to_json());
        assert!(
            report.diagnostics.iter().any(
                |diagnostic| diagnostic.code == "E-ANALYZE-IR" && diagnostic.message == message
            ),
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
