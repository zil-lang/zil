//! Independent machines: a different layout, actual API objects, every exit.
#![cfg(unix)]
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Stdio},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

fn out(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = root.join(format!("review223-{label}"));
    fs::create_dir_all(&out).unwrap();
    out
}
fn compile(
    label: &str,
    source: &str,
    optimization: Optimization,
    capture: bool,
) -> (PathBuf, PathBuf) {
    let dir = out(&format!("{label}-{optimization:?}"));
    let executable = dir.join("program");
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    let driver = if capture {
        let quote =
            |p: &std::path::Path| format!("'{}'", p.to_string_lossy().replace('\'', "'\"'\"'"));
        let wrapper = dir.join("retain-object.sh");
        fs::write(&wrapper,format!("#!/bin/sh\nfor arg in \"$@\"; do\n case \"$arg\" in *.o|*.obj) cp -- \"$arg\" {} ;; esac\ndone\nexec {} \"$@\"\n",quote(&dir.join("api.o")),quote(&clang))).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        wrapper
    } else {
        clang
    };
    fs::write(dir.join("source.zil"), source).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "independent223.zil", source),
        &CompileOptions {
            backend: NativeBackend::LlvmApi,
            clang: Some(driver),
            stop_after: Stage::Build,
            optimization,
            emit_dir: Some(dir.join("phases")),
            output: Some(executable.clone()),
            ..CompileOptions::default()
        },
    );
    fs::write(dir.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let summary = report.native_codegen.as_ref().unwrap();
    assert_eq!(summary.backend, NativeBackend::LlvmApi);
    assert!(summary.fallback.is_none());
    assert_eq!(summary.actual_units, 1);
    assert!(
        summary
            .object_reports
            .iter()
            .all(|r| r.contains("\"input_kind\":\"builder\"")
                && r.contains("\"verify_before_after\":true"))
    );
    (dir, executable)
}
fn execute(
    executable: &std::path::Path,
    input: u8,
    fail: Option<usize>,
    dir: &std::path::Path,
    label: &str,
) -> std::process::Output {
    let mut cmd = Command::new(executable);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(fail) = fail {
        cmd.env("ZIL_TEST_FAIL_AT", fail.to_string());
    }
    let mut child = cmd.spawn().unwrap();
    child.stdin.take().unwrap().write_all(&[input]).unwrap();
    let output = child.wait_with_output().unwrap();
    fs::write(dir.join(format!("{label}.stdout")), &output.stdout).unwrap();
    fs::write(dir.join(format!("{label}.stderr")), &output.stderr).unwrap();
    fs::write(
        dir.join(format!("{label}.json")),
        format!(
            "{{\"input\":{input},\"exit\":{},\"fail_at\":{}}}\n",
            output.status.code().unwrap(),
            fail.map_or("null".into(), |n| n.to_string())
        ),
    )
    .unwrap();
    output
}

#[test]
fn failed_push_keeps_the_complete_product_through_two_return_edges() {
    let source = include_str!("review223_fixtures/entry_error.zil");
    for optimization in [Optimization::O0, Optimization::O2] {
        let (dir, program) = compile("entry-error", source, optimization, false);
        let result = execute(&program, b'0', None, &dir, "run");
        assert_eq!(result.status.code(), Some(1));
        assert_eq!(result.stdout, b"ABCDEF");
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn mixed_layout_and_every_allocator_exit_preserve_exact_leaf_debts() {
    let source = include_str!("review223_fixtures/heterogeneous.zil");
    let clang = std::env::var_os("ZIL_CLANG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("clang"));
    let shim = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/language/173/allocator_probe.c");
    for optimization in [Optimization::O0, Optimization::O2] {
        let (dir, program) = compile("heterogeneous", source, optimization, true);
        let probe = dir.join("allocation-probe");
        assert!(dir.join("api.o").is_file());
        let linked = Command::new(&clang)
            .arg(dir.join("api.o"))
            .arg(&shim)
            .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
            .arg(&probe)
            .output()
            .unwrap();
        fs::write(dir.join("link.stdout"), &linked.stdout).unwrap();
        fs::write(dir.join("link.stderr"), &linked.stderr).unwrap();
        assert!(
            linked.status.success(),
            "{}",
            String::from_utf8_lossy(&linked.stderr)
        );
        for (mode, normal, requests) in [
            (b'1', "AZBC", vec![(128, 0), (7, 0), (11, 0)]),
            (
                b'2',
                "DZEFAZBC",
                vec![(256, 0), (7, 0), (11, 0), (17, 0), (21, 0)],
            ),
            (
                b'3',
                "DZEFAZBC",
                vec![(128, 0), (7, 0), (11, 0), (17, 0), (21, 0)],
            ),
            (b'4', "AZBC", vec![(128, 0), (7, 0), (11, 0), (256, 128)]),
        ] {
            let result = execute(&program, mode, None, &dir, &format!("mode-{mode}-ordinary"));
            assert_eq!(result.status.code(), Some(0));
            assert_eq!(result.stdout, normal.as_bytes());
            assert!(result.stderr.is_empty());
            for fail in 0..=requests.len() + 1 {
                let mut calls = 0;
                let mut allocated = 0;
                let mut bytes = 0;
                let mut peak = 0;
                let mut failed = false;
                for (request, previous) in &requests {
                    calls += 1;
                    if fail == calls {
                        failed = true;
                        break;
                    }
                    if *previous == 0 {
                        allocated += 1;
                    }
                    bytes += request - previous;
                    peak = peak.max(bytes);
                }
                let expected = if failed {
                    match fail {
                        1 => "",
                        2 => "A",
                        3 => "BA",
                        4 if mode == b'4' => "AZBC",
                        4 => "DAZBC",
                        5 => "EDAZBC",
                        _ => unreachable!(),
                    }
                } else {
                    normal
                };
                let result = execute(
                    &probe,
                    mode,
                    Some(fail),
                    &dir,
                    &format!("mode-{mode}-failure-{fail}"),
                );
                assert_eq!(
                    result.status.code(),
                    Some(i32::from(failed)),
                    "mode={mode} fail={fail}"
                );
                assert_eq!(
                    result.stdout,
                    expected.as_bytes(),
                    "mode={mode} fail={fail}"
                );
                let allocation = String::from_utf8(result.stderr).unwrap();
                let oracle = format!(
                    "{{\"alloc_calls\":{calls},\"allocated\":{allocated},\"frees\":{allocated},\"failures\":{},\"live\":0,\"peak_live\":{allocated},\"peak_bytes\":{peak}}}",
                    usize::from(failed)
                );
                assert_eq!(allocation.trim(), oracle, "mode={mode} fail={fail}");
            }
        }
    }
}
