//! Independent supplement for ordinary defer in retained generic definitions.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = root.join(format!(
        "review237-generic-defer-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&out).unwrap();
    out
}
fn native(label: &str, text: &str, expected: &[u8]) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir(label);
        fs::write(out.join("source.zil"), text).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "generic_defer.zil", text),
            &CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let summary = report.native_codegen.as_ref().unwrap();
        assert_eq!(summary.backend, backend);
        assert!(summary.fallback.is_none());
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(out.join("stdout"), &result.stdout).unwrap();
        fs::write(out.join("stderr"), &result.stderr).unwrap();
        fs::write(
            out.join("exit.txt"),
            format!("{:?}\n", result.status.code()),
        )
        .unwrap();
        assert_eq!(result.status.code(), Some(0), "{label}: {result:?}");
        assert_eq!(result.stdout, expected);
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn original_self_feeding_generic_defer_sources_execute_alqz_without_editing_the_cleanup() {
    // These are the original 238 probes. They require the combined candidate's
    // whole-Product replacement lowering as well as generic defer admission.
    native(
        "original-implicit",
        include_str!("review237_fixtures/original238_defer_implicit.zil"),
        b"ALQZ",
    );
    native(
        "original-explicit",
        include_str!("review237_fixtures/original238_defer_explicit.zil"),
        b"ALQZ",
    );
}

#[test]
fn a_generic_cleanup_has_its_own_legal_loop_control_boundary() {
    native(
        "own-loop",
        "fn cleanup<T:Copy>(unused:T)->i32{defer{var i=0;while i<2{i+=1;if i==1{continue}break}}return 0}fn main()->i32{return cleanup(7)}",
        b"",
    );
}

#[test]
fn generic_defer_keeps_the_existing_direct_callable_helper_boundary() {
    for (label, text) in [
        (
            "helper-local-capture-F-boundary",
            "fn cleanup<F:Fn()->i32>(callback:F)->i32{defer{var i=0;while i<2{i+=1;if i==1{continue}break}callback()}return 0}fn make()->impl Fn()->i32{return move()=>7}fn main()->i32{return cleanup(make())}",
        ),
        (
            "helper-local-empty-capture-boundary",
            "fn cleanup<F:Fn()->i32>(callback:F)->i32{defer{var i=0;while i<2{i+=1;if i==1{continue}break}}return 0}fn make()->impl Fn()->i32{return move()=>7}fn main()->i32{return cleanup(make())}",
        ),
    ] {
        let out = dir(label);
        fs::write(out.join("source.zil"), text).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "generic_defer.zil", text),
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == Phase::Types && d.category == ResultCategory::Unsupported)
        );
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn unused_generic_defer_definitions_preserve_declared_capabilities_and_cleanup_control() {
    for (label, definition) in [
        (
            "twice-once",
            "fn probe<F:FnOnce()->i32>(callback:F){defer{callback();callback()}}",
        ),
        (
            "borrowed-once",
            "fn probe<F:FnOnce()->i32>(callback:&F){defer{callback()}}",
        ),
        (
            "moved-at-registration",
            "fn probe<F:FnOnce()->i32>(callback:F){defer{callback()}callback()}",
        ),
        (
            "shared-capture-then-move",
            "fn probe<F:Fn()->i32>(callback:F){defer{callback()}let moved=callback;}",
        ),
        (
            "exclusive-capture-then-call",
            "fn probe<F:FnMut()->i32>(callback:F){defer{callback()}callback()}",
        ),
        (
            "package-shared-capture-then-take",
            "resource Ticket{code:i32}package State<F>{manages marker:Ticket;callback:F}fn probe<F:Fn()->i32>(state:State<F>){defer{state.callback()}let moved=state.callback;}",
        ),
        (
            "package-once-capture-then-call",
            "resource Ticket{code:i32}package State<F>{manages marker:Ticket;callback:F}fn probe<F:FnOnce()->i32>(state:State<F>){defer{state.callback()}state.callback()}",
        ),
        ("return", "fn probe<F:Fn()->i32>(callback:F){defer{return}}"),
        (
            "nested-defer",
            "fn probe<F:Fn()->i32>(callback:F){defer{defer{callback()}}}",
        ),
        (
            "outer-break",
            "fn probe<F:Fn()->i32>(callback:F){while true{defer{break}}}",
        ),
        (
            "outer-continue",
            "fn probe<F:Fn()->i32>(callback:F){while true{defer{continue}}}",
        ),
        (
            "error-propagation",
            "fn value()->bool!i32{return 0}fn probe<F:Fn()->i32>(callback:F)->bool!i32{defer{value()?}return 0}",
        ),
        (
            "foreign-without-unsafe",
            "extern \"C\" fn putchar(code:i32)->i32;fn probe<F:Fn()->i32>(callback:F){defer{putchar(65)}}",
        ),
        (
            "foreign-outside-cleanup",
            "extern \"C\" fn putchar(code:i32)->i32;fn probe<F:Fn()->i32>(callback:F){unsafe{putchar(65)}}",
        ),
    ] {
        let text = format!("{definition}fn main(){{}}");
        let out = dir(label);
        fs::write(out.join("source.zil"), &text).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "unused_generic_defer.zil", text),
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
        let closed_state_capture = matches!(
            label,
            "package-shared-capture-then-take" | "package-once-capture-then-call"
        );
        assert!(
            report.diagnostics.iter().any(|d| (d.phase == Phase::Expand
                && matches!(
                    d.category,
                    ResultCategory::SafeReject | ResultCategory::Unsupported
                ))
                || (closed_state_capture
                    && d.phase == Phase::Types
                    && d.category == ResultCategory::Unsupported)),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}
