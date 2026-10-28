//! Production CLI/driver path: same current-language frontend and independent Verify.
#![cfg(feature = "llvm-api")]
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompileOptions, NativeBackend, Optimization, Stage, compile_path,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil-api208-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn write(&self, name: &str, text: &str) {
        fs::write(self.0.join(name), text).unwrap();
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn options(dir: &Dir, units: usize, opt: Optimization) -> CompileOptions {
    CompileOptions {
        backend: NativeBackend::LlvmApi,
        stop_after: Stage::Build,
        output: Some(dir.0.join("program")),
        optimization: opt,
        codegen: CodegenOptions { units, jobs: 4 },
        ..CompileOptions::default()
    }
}
fn run(path: &Path, input: &[u8]) -> std::process::Output {
    let mut child = Command::new(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
fn source(dir: &Dir) {
    dir.write(
        "main.zil",
        "import a\nimport b\nfn main()->i32 { return a.get()+b.get(); }",
    );
    dir.write("a.zil", "pub fn get()->i32 { return 19; }");
    dir.write("b.zil", "pub fn get()->i32 { return 23; }");
}
fn success(report: &zil_driver::CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let s = report.native_codegen.as_ref().unwrap();
    assert_eq!(s.backend, NativeBackend::LlvmApi);
    assert_eq!(s.object_reports.len(), s.actual_units);
    assert!(
        s.object_reports
            .iter()
            .all(|s| s.contains("\"verify_before_after\":true"))
    );
    assert_eq!(report.commands.len(), 1, "only the linker is a subprocess");
    assert!(!report.commands[0].iter().any(|v| v == "ir" || v == "-c"));
}
#[test]
fn same_ir_with_clang_and_api_single_and_parallel() {
    let d = Dir::new();
    source(&d);
    for opt in [Optimization::O0, Optimization::O2] {
        let mut baseline = options(&d, 1, opt);
        baseline.backend = NativeBackend::Clang;
        let reference = compile_path(&d.0.join("main.zil"), &baseline);
        assert_eq!(
            reference.status,
            ResultCategory::Success,
            "{}",
            reference.to_json()
        );
        let ir = fs::read(&reference.artifacts["llvm"]).unwrap();
        for units in [1, 4] {
            let r = compile_path(&d.0.join("main.zil"), &options(&d, units, opt));
            success(&r);
            assert_eq!(fs::read(&r.artifacts["llvm"]).unwrap(), ir);
            assert_eq!(run(&r.artifacts["executable"], b"").status.code(), Some(42));
            if units > 1 {
                assert!(r.native_codegen.unwrap().actual_units > 1);
            }
        }
    }
}
#[test]
fn return_value_entry_wrapper_works_with_private_format_global() {
    let d = Dir::new();
    source(&d);
    let mut o = options(&d, 8, Optimization::O2);
    o.entry = Some("main".into());
    let r = compile_path(&d.0.join("main.zil"), &o);
    success(&r);
    let out = run(&r.artifacts["executable"], b"");
    assert!(out.status.success());
    assert_eq!(out.stdout, b"42\n");
}
#[test]
fn checked_overflow_traps_in_o0_o2_and_parallel() {
    let d = Dir::new();
    d.write(
        "main.zil",
        "import a\nfn main()->i32 { return a.add(2147483647); }",
    );
    d.write("a.zil", "pub fn add(x:i32)->i32 { return x+1; }");
    for opt in [Optimization::O0, Optimization::O2] {
        for units in [1, 8] {
            let r = compile_path(&d.0.join("main.zil"), &options(&d, units, opt));
            success(&r);
            let out = run(&r.artifacts["executable"], b"");
            assert!(!out.status.success());
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(out.status.signal(), Some(4));
            }
        }
    }
}
#[test]
fn function_value_target_and_caller_in_different_units() {
    let d = Dir::new();
    d.write(
        "main.zil",
        "import a\nimport b\nfn main()->i32 { return b.call(a.get(), 20); }",
    );
    d.write(
        "a.zil",
        "fn twice(x:i32)->i32 { return x*2; } pub fn get()->fn(i32)->i32 { return twice; }",
    );
    d.write(
        "b.zil",
        "pub fn call(f:fn(i32)->i32,x:i32)->i32 { return f(x)+2; }",
    );
    for opt in [Optimization::O0, Optimization::O2] {
        let r = compile_path(&d.0.join("main.zil"), &options(&d, 8, opt));
        success(&r);
        assert_eq!(run(&r.artifacts["executable"], b"").status.code(), Some(42));
    }
}
#[test]
fn current_177_state_store_callback_and_field_exit() {
    let d = Dir::new();
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/zui_language177");
    for entry in ["main", "field_exit"] {
        for opt in [Optimization::O0, Optimization::O2] {
            for units in [1, 8] {
                let r = compile_path(&root.join(format!("{entry}.zil")), &options(&d, units, opt));
                success(&r);
                for (input, main, field) in [
                    (b"0", "HZMK", "HZ"),
                    (b"1", "DHZMNK", "DHZ"),
                    (b"2", "DEHZMNOK", "DEHZ"),
                    (b"3", "DEFHZMNOPK", "DEFHZ"),
                ] {
                    let out = run(&r.artifacts["executable"], input);
                    assert_eq!(out.status.code(), Some(if entry == "main" { 0 } else { 1 }));
                    assert_eq!(
                        out.stdout,
                        if entry == "main" {
                            main.as_bytes()
                        } else {
                            field.as_bytes()
                        }
                    );
                }
                assert_eq!(run(&r.artifacts["executable"], b"9").status.code(), Some(9));
            }
        }
    }
}
#[test]
fn invalid_borrow_has_same_rejection_before_native_calls() {
    let d = Dir::new();
    d.write(
        "main.zil",
        "fn main()->i32 { var x:i32=1; let r=&x; x=2; return *r; }",
    );
    d.write("program", "previous executable");
    d.write("program.ll", "previous ir");
    let r = compile_path(&d.0.join("main.zil"), &options(&d, 8, Optimization::O2));
    assert_eq!(r.status, ResultCategory::SafeReject);
    assert_eq!(r.diagnostics[0].code, "E-BORROW-CONFLICT");
    assert!(r.commands.is_empty());
    assert!(r.native_codegen.is_none());
    assert_eq!(
        fs::read_to_string(d.0.join("program")).unwrap(),
        "previous executable"
    );
    assert_eq!(
        fs::read_to_string(d.0.join("program.ll")).unwrap(),
        "previous ir"
    );
}
#[test]
fn missing_external_symbol_is_still_link_failure_in_both_modes() {
    let d = Dir::new();
    d.write("main.zil", "extern \"C\" fn zil208_absent()->i32; fn main()->i32 { unsafe { return zil208_absent(); } }");
    for opt in [Optimization::O0, Optimization::O2] {
        let r = compile_path(&d.0.join("main.zil"), &options(&d, 1, opt));
        assert_eq!(r.status, ResultCategory::InfraError, "{}", r.to_json());
        assert_eq!(r.diagnostics[0].code, "E-LLVM-COMPILE");
        assert_eq!(r.verification_runs, 1);
        assert!(!d.0.join("program").exists());
    }
}
#[test]
fn cancelled_request_preserves_previous_outputs() {
    let d = Dir::new();
    source(&d);
    d.write("program", "old exe");
    d.write("program.ll", "old ir");
    let o = options(&d, 8, Optimization::O2);
    o.cancellation.cancel();
    let r = compile_path(&d.0.join("main.zil"), &o);
    assert_eq!(r.status, ResultCategory::Cancelled);
    assert!(r.artifacts.is_empty());
    assert_eq!(fs::read(d.0.join("program")).unwrap(), b"old exe");
    assert_eq!(fs::read(d.0.join("program.ll")).unwrap(), b"old ir");
}
#[cfg(unix)]
#[test]
fn failing_or_lying_linker_cannot_publish_or_leak_staging() {
    let d = Dir::new();
    source(&d);
    d.write("program", "old exe");
    d.write("program.ll", "old ir");
    for linker in ["/bin/false", "/bin/true"] {
        let mut o = options(&d, 8, Optimization::O0);
        o.clang = Some(linker.into());
        let r = compile_path(&d.0.join("main.zil"), &o);
        assert_eq!(r.status, ResultCategory::InfraError, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        assert_eq!(fs::read(d.0.join("program")).unwrap(), b"old exe");
        assert_eq!(fs::read(d.0.join("program.ll")).unwrap(), b"old ir");
        assert!(!fs::read_dir(&d.0).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".zil-")
        }));
    }
}

#[test]
fn cancellation_after_actual_api_codegen_and_link_cannot_publish() {
    use zil_base::{FileId, ModuleId, Phase, SourceUnit};
    use zil_driver::CompilationSession;
    let d = Dir::new();
    d.write("program", "old exe");
    d.write("program.ll", "old ir");
    let source = SourceUnit::new(
        FileId(0),
        ModuleId(0),
        "main.zil",
        "fn main()->i32 { return 42; }",
    );
    let o = options(&d, 1, Optimization::O0);
    let token = o.cancellation.clone();
    let mut linked = false;
    let r = CompilationSession::default().compile_source_with_observer(&source, &o, &mut |event| {
        if event.phase == Phase::Link {
            linked = true;
            token.cancel();
        }
    });
    assert!(linked, "the test must reach completed native linking");
    assert_eq!(r.status, ResultCategory::Cancelled, "{}", r.to_json());
    assert_eq!(r.verification_runs, 1);
    assert_eq!(fs::read(d.0.join("program")).unwrap(), b"old exe");
    assert_eq!(fs::read(d.0.join("program.ll")).unwrap(), b"old ir");
    assert!(!r.artifacts.contains_key("executable"));
}

#[test]
fn warm_source_cache_does_not_replace_independent_verify() {
    use zil_driver::CompilationSession;
    let d = Dir::new();
    source(&d);
    let o = options(&d, 4, Optimization::O0);
    let mut session = CompilationSession::default();
    for _ in 0..2 {
        let r = session.compile_path(&d.0.join("main.zil"), &o);
        success(&r);
    }
    d.write(
        "main.zil",
        "fn main()->i32 { var x:i32=1; let r=&x; x=2; return *r; }",
    );
    let r = session.compile_path(&d.0.join("main.zil"), &o);
    assert_eq!(r.status, ResultCategory::SafeReject);
    assert_eq!(r.diagnostics[0].code, "E-BORROW-CONFLICT");
    assert!(!r.artifacts.contains_key("executable"));
    assert_eq!(run(&d.0.join("program"), b"").status.code(), Some(42));
}
