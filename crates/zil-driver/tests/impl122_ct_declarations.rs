//! Source-to-native regressions for pure CT alongside runtime resource cleanup.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, Stage, compile_path, compile_source,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const RESOURCE: &str = r#"extern "C" fn putchar(code:i32)->i32;
resource Token{code:i32}
impl Drop for Token{fn drop(self:Self){unsafe{putchar(.code)}}}
fn consume(give t:Token){}
"#;
const PURE: &str = "enum E{N(i64),Empty}fn make()->E{return E.N(42)}";

struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = keep
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "ct122-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, keep.is_some())
    }
    fn report(&self, r: &CompileReport) {
        fs::write(self.0.join("compile.json"), r.to_json()).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn native(text: &str, expected: &[u8]) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let c = Case::new("native");
        fs::write(c.0.join("source.zil"), text).unwrap();
        let r = compile_source(
            &SourceUnit::new(FileId(0), ModuleId(0), "ct122.zil", text),
            &CompileOptions {
                stop_after: Stage::Build,
                entry: Some("value".into()),
                output: Some(c.0.join("program")),
                emit_dir: Some(c.0.join("phases")),
                optimization,
                ..CompileOptions::default()
            },
        );
        c.report(&r);
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        let out = Command::new(&r.artifacts["executable"]).output().unwrap();
        fs::write(c.0.join("expected.stdout"), expected).unwrap();
        fs::write(c.0.join("actual.stdout"), &out.stdout).unwrap();
        fs::write(c.0.join("actual.stderr"), &out.stderr).unwrap();
        fs::write(c.0.join("actual.status"), out.status.to_string()).unwrap();
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(out.stdout, expected);
        assert!(out.stderr.is_empty());
    }
}
fn reject(text: &str, phase: Phase, code: &str) {
    let c = Case::new("reject");
    fs::write(c.0.join("source.zil"), text).unwrap();
    let r = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "ct122.zil", text),
        &CompileOptions {
            stop_after: Stage::Lower,
            emit_dir: Some(c.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    c.report(&r);
    assert_eq!(r.status, ResultCategory::SafeReject, "{}", r.to_json());
    assert_eq!(r.phase, phase, "{}", r.to_json());
    assert_eq!(r.diagnostics[0].code, code, "{}", r.to_json());
    assert!(!r.artifacts.contains_key("executable"));
}

#[test]
fn pure_enum_query_keeps_runtime_drop_on_early_return() {
    native(
        &format!(
            "{RESOURCE}{PURE}fn value()->i64{{let t=Token{{code:84}};let e=#(make());match e{{E.N(n)=>{{return n}},E.Empty()=>{{return 0}}}}}}"
        ),
        b"T42\n",
    );
}
#[test]
fn pure_record_and_repeated_queries_keep_each_runtime_owner() {
    native(
        &format!(
            "{RESOURCE}struct P{{n:i64}}fn make()->P{{return P{{n:21}}}}fn value()->i64{{let a=Token{{code:65}};let b=Token{{code:66}};let x=#(make());let y=#(make());return x.n+y.n}}"
        ),
        b"BA42\n",
    );
}
#[test]
fn owned_callback_can_combine_ct_query_with_consuming_its_token() {
    native(
        &format!(
            "{RESOURCE}{PURE}fn value()->i64{{let t=Token{{code:84}};let f=move ()=>{{let e=#(make());consume(give t);match e{{E.N(n)=>{{return n}},E.Empty()=>{{return 0}}}}}};return f()}}"
        ),
        b"T42\n",
    );
}
#[test]
fn pure_queries_across_modules_do_not_import_runtime_drop_execution() {
    for optimization in [Optimization::O0, Optimization::O2] {
        let c = Case::new("modules");
        fs::write(
            c.0.join("left.zil"),
            "pub enum E{N(i64)}pub fn make()->E{return E.N(35)}",
        )
        .unwrap();
        fs::write(
            c.0.join("right.zil"),
            "pub enum E{N(i64)}pub fn make()->E{return E.N(7)}",
        )
        .unwrap();
        fs::write(c.0.join("main.zil"),format!("import left\nimport right\n{RESOURCE}fn value()->i64{{let t=Token{{code:84}};let a=#(left.make());let b=#(right.make());match a{{left.E.N(n)=>{{match b{{right.E.N(m)=>{{return n+m}}}}}}}}}}" )).unwrap();
        let r = compile_path(
            &c.0.join("main.zil"),
            &CompileOptions {
                stop_after: Stage::Build,
                entry: Some("value".into()),
                output: Some(c.0.join("program")),
                emit_dir: Some(c.0.join("phases")),
                optimization,
                ..CompileOptions::default()
            },
        );
        c.report(&r);
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        let out = Command::new(&r.artifacts["executable"]).output().unwrap();
        fs::write(c.0.join("expected.stdout"), b"T42\n").unwrap();
        fs::write(c.0.join("actual.stdout"), &out.stdout).unwrap();
        fs::write(c.0.join("actual.stderr"), &out.stderr).unwrap();
        fs::write(c.0.join("actual.status"), out.status.to_string()).unwrap();
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(out.stdout, b"T42\n");
        assert!(out.stderr.is_empty());
    }
}
#[test]
fn query_resource_operations_and_ffi_remain_forbidden() {
    reject(
        &format!(
            "{RESOURCE}fn bad()->i64{{let t=Token{{code:84}};return 42}}fn value()->i64{{return #(bad())}}"
        ),
        Phase::Ct,
        "E-CT-FORBIDDEN",
    );
    reject(
        &format!(
            "{RESOURCE}fn bad()->i64{{unsafe{{return i64(putchar(84))}}}}fn value()->i64{{return #(bad())}}"
        ),
        Phase::Ct,
        "E-CT-FORBIDDEN",
    );
    reject(
        "fn bad()->i64{let b=bytes_new();return bytes_len(b)}fn value()->i64{return #(bad())}",
        Phase::Ct,
        "E-CT-FORBIDDEN",
    );
}
#[test]
fn runtime_drop_body_still_receives_ordinary_type_checking() {
    reject(
        &format!(
            "resource Token{{code:i32}}impl Drop for Token{{fn drop(self:Self){{let n:i32=true;}}}}{PURE}fn value()->i64{{let t=Token{{code:84}};let e=#(make());match e{{E.N(n)=>{{return n}},E.Empty()=>{{return 0}}}}}}"
        ),
        Phase::Types,
        "E-TYPE-MISMATCH",
    );
}
