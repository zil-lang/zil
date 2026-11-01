//! Predeclared consuming-pattern cleanup at return/continue/break exits.
use std::{fs, path::PathBuf, process::Command};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
const DECLS: &str = "extern \"C\" fn putchar(c:i32)->i32;fn emit(n:i32){unsafe{putchar(n)}}resource R{n:i32}impl Drop for R{fn drop(self:Self){emit(.n)}}enum Inner{Empty,Pair(manages R,manages R)}enum Outer{Empty,Pair(manages R,Inner)}";
const CONSTRUCT: &str = "let value=Outer.Pair(R{n:65},Inner.Pair(R{n:67},R{n:68}));";
fn run(label: &str, text: String, expected: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = std::env::var_os("ZIL_TEST_REPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!(
                "review240-exits-{label}-{optimization:?}-{}",
                std::process::id()
            ));
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("source.zil"), &text).unwrap();
        fs::write(out.join("expected.stdout"), expected).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "match-exits240.zil", text.as_str()),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                backend: if cfg!(feature = "llvm-api") {
                    NativeBackend::LlvmApi
                } else {
                    NativeBackend::Clang
                },
                emit_dir: Some(out.join("phases")),
                output: Some(out.join("program")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(out.join("stdout"), &result.stdout).unwrap();
        fs::write(out.join("stderr"), &result.stderr).unwrap();
        fs::write(out.join("exit.txt"), format!("{:?}", result.status.code())).unwrap();
        assert_eq!(result.status.code(), Some(0));
        assert_eq!(result.stdout, expected.as_bytes());
        assert!(result.stderr.is_empty());
    }
}
#[test]
fn early_arm_return_runs_defer_then_recursive_pattern_leaves() {
    // Bindings are published D,C,A; then defer Q and late L register. Return
    // reverses initialization: L,Q,A,C,D. Main emits N after the function.
    let source = format!(
        "{DECLS}fn run(){{{CONSTRUCT}match value{{Outer.Pair(a,Inner.Pair(c,d))=>{{defer{{emit(81)}}let late=R{{n:76}};return}}\nOuter.Pair(a,Inner.Empty())=>{{return}}\nOuter.Empty()=>{{return}}}}}}fn main()->i32{{run();emit(78);return 0}}"
    );
    run("return", source, "LQACDN");
}
#[test]
fn continue_and_break_each_discharge_only_the_current_pattern_owners() {
    // Two iterations: continue then break, each LQACD. The final N is outside
    // the loop. No previous iteration's tag, debt or payload can be reused.
    let source = format!(
        "{DECLS}fn main()->i32{{var i=0;while i<2{{i=i+1;{CONSTRUCT}match value{{Outer.Pair(a,Inner.Pair(c,d))=>{{defer{{emit(81)}}let late=R{{n:76}};if i==1{{continue}}break}}\nOuter.Pair(a,Inner.Empty())=>{{break}}\nOuter.Empty()=>{{break}}}}}}emit(78);return 0}}"
    );
    run("loop", source, "LQACDLQACDN");
}
