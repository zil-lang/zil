//! Independent post-P compatibility checks; no production implementation edits.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, NativeBackend, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn dir(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let out = base.join(format!(
        "review238-compat-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&out).unwrap();
    out
}
const LIB: &str = r#"
resource R{n:i32}
pub package P{manages owned:R;n:i32}
pub fn make()->P{return P{owned:R{n:7},n:40}}
pub fn step(p:P,delta:i32)->P{let owned=give p.owned;return P{owned:give owned,n:p.n+delta}}
pub fn finish(p:P)->i32{return p.n}
"#;
fn project(body: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                format!("import library;fn main()->i32{{{body}}}"),
            ),
            SourceUnit::new(FileId(1), ModuleId(1), "library.zil", LIB),
        ],
        paths: [(vec!["library".into()], ModuleId(1))].into(),
    }
}
fn save_sources(out: &std::path::Path, p: &ProjectSources) {
    for s in &p.modules {
        fs::write(out.join(format!("module{}.zil", s.module_id.0)), &s.text).unwrap();
    }
}
#[test]
fn imported_private_product_self_argument_is_an_opaque_complete_move() {
    let p = project("var p=library.make();p=library.step(p,2);return library.finish(p)-42");
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir("private-whole-positive");
        save_sources(&out, &p);
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_project(
            &p,
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
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
        assert_eq!(result.status.code(), Some(0));
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }
}
#[test]
fn internal_whole_pack_does_not_grant_source_field_access_to_other_arguments() {
    // The forbidden read is on a distinct binding, so the transaction's target
    // occurrence rule cannot mask a temporary module/visibility bypass.
    for (label, body) in [
        (
            "private-extra-argument",
            "var p=library.make();let other=library.make();p=library.step(p,other.n);return library.finish(p)",
        ),
        (
            "private-read-after-transaction",
            "var p=library.make();p=library.step(p,2);return p.n",
        ),
    ] {
        let p = project(body);
        let out = dir(label);
        save_sources(&out, &p);
        let report = compile_project(
            &p,
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.phase == Phase::Types && d.code == "E-FIELD-PRIVATE"),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn capture_view_compatibility_does_not_reopen_an_ordinary_dead_product_local() {
    let model =
        "resource R{n:i32}package P{manages r:R;n:i32}fn make()->P{return P{r:R{n:7},n:40}}";
    for (label, body) in [
        (
            "dead-local-refill",
            "var p=make();let moved=p;p=make();return 0",
        ),
        (
            "dead-local-copy-read",
            "var p=make();let moved=p;return p.n",
        ),
        (
            "maybe-dead-local-refill",
            "var p=make();if true{let moved=p}p=make();return 0",
        ),
    ] {
        let text = format!("{model}fn main()->i32{{{body}}}");
        let out = dir(label);
        fs::write(out.join("source.zil"), &text).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "dead_product.zil", text),
            &CompileOptions {
                stop_after: Stage::Verify,
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| matches!(d.phase, Phase::Analyze | Phase::Verify)),
            "{label}: {}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn internal_self_pack_preserves_existing_owned_union_argument_injection() {
    let mut p =
        project("var p=library.make();p=library.relay_union(p);return library.finish(p)-40");
    p.modules[1] = SourceUnit::new(
        FileId(1),
        ModuleId(1),
        "library.zil",
        r#"
extern "C" fn putchar(code:i32)->i32;
resource R{code:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
pub package P{manages owned:R;n:i32}
pub fn make()->P{return P{owned:R{code:65},n:40}}
pub fn relay_union(value:void!P)->P{return make()}
pub fn finish(p:P)->i32{return p.n}
"#,
    );
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = dir("private-owned-union-injection");
        save_sources(&out, &p);
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_project(
            &p,
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                output: Some(out.join("program")),
                emit_dir: Some(out.join("phases")),
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
        assert_eq!(result.status.code(), Some(0));
        assert_eq!(result.stdout, b"AA");
        assert!(result.stderr.is_empty());
    }
}
