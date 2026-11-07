//! Joint source generation, owning Store and shared Product integration.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, NativeBackend, Optimization, Stage, compile_source};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const PREFIX: &str = r#"
extern "C" fn putchar(x:i32)->i32;
resource R{code:i32;n:i32}
impl Drop for R{fn drop(self:Self){unsafe{putchar(.code)}}}
package Leaf{manages owner:R;value:i32}
package Root{child:Leaf;tag:i32}
fn make(code:i32,n:i32)->Root{return Root{child:Leaf{owner:R{code:code,n:n+1},value:n},tag:3}}
fn leaf(p:&Leaf)->i32{return p.value+p.owner.n}
fn read(p:&Root)->i32{return leaf(&p.child)+p.tag}
fn consume(give s:Store<Root>){}
"#;
fn dir(label: &str) -> PathBuf {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let p = base.join(format!(
        "composition222-224-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&p).unwrap();
    p
}
fn run(label: &str, body: &str, output: &[u8], exit: i32) {
    let source = format!("{PREFIX}{body}");
    for optimization in [Optimization::O0, Optimization::O2] {
        let p = dir(label);
        let executable = p.join("program");
        fs::write(p.join("source.zil"), &source).unwrap();
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "composition.zil", &source),
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                backend,
                output: Some(executable.clone()),
                emit_dir: Some(p.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(p.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.native_codegen.as_ref().unwrap().backend, backend);
        assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
        let actual = Command::new(executable).output().unwrap();
        fs::write(p.join("stdout"), &actual.stdout).unwrap();
        fs::write(p.join("stderr"), &actual.stderr).unwrap();
        fs::write(p.join("exit"), format!("{:?}", actual.status.code())).unwrap();
        assert_eq!(actual.status.code(), Some(exit), "{label}: {actual:?}");
        assert_eq!(actual.stdout, output, "{label}: {actual:?}");
        assert!(actual.stderr.is_empty());
    }
}
#[test]
fn generated_product_elements_keep_shared_child_lifetimes_and_recursive_cleanup() {
    run(
        "generated-prefix",
        r#"fn main()->i32{
 let s:Store<Root>=store_new();if !store_reserve(s,2){return 71}
 #for(code in [65,66]){store_push(s,make(code,code-60))}
 let view=store_at(s,0);let child=&view.child;let v=read(view)+leaf(child);
 if !store_reserve(s,4){return 72}
 let last=store_at(s,1);let total=v+read(last);store_pop(s);return total-41
 }"#,
        b"BA",
        0,
    );
    run(
        "ordinary-child",
        r#"fn main()->i32{let root=make(65,5);let child=&root.child;let n=leaf(child);let moved=root;return n+read(&moved)-25}"#,
        b"A",
        0,
    );
}
#[test]
fn shared_store_child_keeps_the_actual_container_loan_until_last_use() {
    for (label, change) in [
        ("reserve", "store_reserve(s,4);"),
        ("push", "store_push(s,make(66,6));"),
        ("pop", "store_pop(s);"),
        ("consume", "consume(give s);"),
    ] {
        let source = format!(
            "{PREFIX}fn main()->i32{{let s:Store<Root>=store_new();store_reserve(s,2);store_push(s,make(65,5));let view=store_at(s,0);let child=&view.child;{change}return leaf(child)}}"
        );
        let p = dir(label);
        fs::write(p.join("source.zil"), &source).unwrap();
        let report = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "composition.zil", &source),
            &CompileOptions {
                emit_dir: Some(p.join("phases")),
                limits: Limits::default(),
                ..CompileOptions::default()
            },
        );
        fs::write(p.join("compile.json"), report.to_json()).unwrap();
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
                .any(|d| d.phase == Phase::Verify && d.code == "E-BORROW-CONFLICT"),
            "{label}: {}",
            report.to_json()
        );
    }
}
