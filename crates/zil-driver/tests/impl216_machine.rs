//! Real source -> independent Verify -> inline LLVM -> native cleanup. The
//! existing allocator observer wraps realloc/free, not language ownership.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Optimization, Stage, compile_source};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const MODEL: &str = r#"
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
package Leaf{manages first:Marker;manages second:Marker;n:i32}
package Node{manages lead:Marker;child:Leaf;manages tail:Marker;tag:i32}
package Root{child:Node;tag:i32}
fn root(base:i32)->Root{
    return Root{child:Node{lead:Marker{code:base},
        child:Leaf{first:Marker{code:base+1},second:Marker{code:base+2},n:base},
        tail:Marker{code:base+3},tag:base},tag:base}
}
"#;

fn build(label: &str, source: &str, opt: Optimization) -> PathBuf {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = root.join(format!(
        "product216-{label}-{opt:?}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("main.zil"), source).unwrap();
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "product216.zil", source),
        &CompileOptions {
            stop_after: Stage::Build,
            optimization: opt,
            output: Some(path.join("program")),
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("compile.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    path
}

fn native(label: &str, body: &str, expected: &str) {
    let source = format!("{MODEL}{body}");
    for opt in [Optimization::O0, Optimization::O2] {
        let path = build(label, &source, opt);
        let output = Command::new(path.join("program")).output().unwrap();
        fs::write(path.join("stdout.log"), &output.stdout).unwrap();
        fs::write(path.join("stderr.log"), &output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(0), "{label}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

#[test]
fn owned_fn_and_uncalled_environment_drop_all_descendants_once() {
    for call in ["if f()+f()!=130{return 9}", ""] {
        native(
            "fn-read-or-unused",
            &format!(
                r#"
fn make(input:Root)->impl Fn()->i32{{return move()=>input.tag}}
fn main()->i32{{let f=make(root(65));{call};emit(88);return 0}}
"#
            ),
            "XABCD",
        );
    }
}

#[test]
fn fnmut_refills_complete_children_on_each_call() {
    native(
        "fnmut-refill",
        r#"
fn make(input:Root)->impl FnMut()->i32{
    var p=input
    return ()=>{var node=p.child;let child=node.child;let n=child.n;
        node.child=Leaf{first:Marker{code:69},second:Marker{code:70},n:n};
        p.child=node;return n}
}
fn main()->i32{var f=make(root(65));if f()+f()!=130{return 9};emit(88);return 0}
"#,
        "BCEFXAEFD",
    );
}

#[test]
fn default_whole_capture_surrenders_environment_before_local_cleanup() {
    native(
        "whole-once",
        r#"
fn main()->i32{let p=root(65);let f=()=>{let q=p;emit(88);return q.tag};
    if f()!=65{return 9};emit(89);return 0}
"#,
        "XABCDY",
    );
}

#[test]
fn once_early_error_cleans_taken_child_then_remaining_local_fields() {
    native(
        "once-error",
        r#"
fn step(fail:bool)->i64!i32{if fail{return i64(9)};return 3}
fn main()->i32{let p=root(65);
    let f=(fail:bool)->i64!i32=>{var node=p.child;let child=node.child;let n=step(fail)?;return n};
    let result=f(true);emit(88);return 0}
"#,
        "BCADX",
    );
}

#[test]
fn fnmut_error_after_refill_preserves_the_next_invocation() {
    native(
        "fnmut-error",
        r#"
fn step(fail:bool)->i64!i32{if fail{return i64(9)};return 3}
fn main()->i32{var p=root(65);
    var f=(fail:bool)->i64!i32=>{let node=p.child;p.child=node;return step(fail)?};
    let failed=f(true);let ok=f(false);emit(88);return 0}
"#,
        "XABCD",
    );
}

#[test]
fn equal_closure_types_retain_distinct_live_flags() {
    native(
        "distinct-instances",
        r#"
fn make(input:Root)->impl FnOnce()->i32{return ()=>{let q=input;return q.tag}}
fn main()->i32{let first=make(root(65));let second=make(root(69));
    if first()!=65{return 9};emit(88);return 0}
"#,
        "ABCDXEFGH",
    );
}

struct Probe<'a> {
    fail: usize,
    stdout: &'a str,
    exit: i32,
    calls: usize,
    allocated: usize,
    failures: usize,
    peak: usize,
}

fn allocation_cases(label: &str, source: &str, cases: &[Probe<'_>]) {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for opt in [Optimization::O0, Optimization::O2] {
        let path = build(label, source, opt);
        let ordinary = Command::new(path.join("program")).output().unwrap();
        fs::write(path.join("ordinary.stdout"), &ordinary.stdout).unwrap();
        fs::write(path.join("ordinary.stderr"), &ordinary.stderr).unwrap();
        let normal = cases.iter().find(|case| case.fail == 0).unwrap();
        assert_eq!(ordinary.status.code(), Some(normal.exit), "{ordinary:?}");
        assert_eq!(ordinary.stdout, normal.stdout.as_bytes(), "{ordinary:?}");
        assert!(ordinary.stderr.is_empty(), "{ordinary:?}");
        let clang = std::env::var_os("ZIL_CLANG").unwrap_or_else(|| "clang".into());
        let probe = path.join("probe");
        let result = Command::new(clang)
            .arg(format!("-{opt:?}"))
            .arg(path.join("phases/llvm.ll"))
            .arg(repo.join("tools/language/173/allocator_probe.c"))
            .args(["-Wl,--wrap=realloc", "-Wl,--wrap=free", "-o"])
            .arg(&probe)
            .output()
            .unwrap();
        fs::write(path.join("probe-link.stdout"), &result.stdout).unwrap();
        fs::write(path.join("probe-link.stderr"), &result.stderr).unwrap();
        assert!(result.status.success(), "{label}: {result:?}");
        for case in cases {
            let output = Command::new(&probe)
                .env("ZIL_TEST_FAIL_AT", case.fail.to_string())
                .output()
                .unwrap();
            fs::write(
                path.join(format!("fail-{}.stdout", case.fail)),
                &output.stdout,
            )
            .unwrap();
            fs::write(
                path.join(format!("fail-{}.stderr", case.fail)),
                &output.stderr,
            )
            .unwrap();
            assert_eq!(
                output.status.code(),
                Some(case.exit),
                "{label}/{}: {output:?}",
                case.fail
            );
            assert_eq!(
                output.stdout,
                case.stdout.as_bytes(),
                "{label}/{}: {output:?}",
                case.fail
            );
            let stderr = String::from_utf8(output.stderr).unwrap();
            let expected = format!(
                "{{\"alloc_calls\":{},\"allocated\":{},\"frees\":{},\"failures\":{},\"live\":0,\"peak_live\":{},\"peak_bytes\":{}}}",
                case.calls,
                case.allocated,
                case.allocated,
                case.failures,
                usize::from(case.allocated > 0),
                case.peak
            );
            assert_eq!(stderr.trim(), expected, "{label}/{}", case.fail);
        }
    }
}

#[test]
fn callback_store_uses_actual_product_environment_stride_and_cleans_failures() {
    let source = include_str!("../../../examples/mainline/product_capture216/main.zil");
    let mutable = source
        .replace("impl Fn()", "impl FnMut()")
        .replace("<F:Fn()", "<F:FnMut()")
        .replace(
            "return move()=>input.tag",
            "var p=input;return ()=>{let child=p.child;let n=child.tag;p.child=child;return n}",
        )
        .replace("store_at(items", "store_at_mut(items");
    assert!(mutable.contains("impl FnMut()") && mutable.contains("p.child=child"));
    for (label, source) in [
        ("callback-store", source),
        ("callback-store-mut", mutable.as_str()),
    ] {
        allocation_cases(
            label,
            source,
            &[
                Probe {
                    fail: 0,
                    stdout: "ABCDEFGH",
                    exit: 0,
                    calls: 2,
                    allocated: 1,
                    failures: 0,
                    peak: 224,
                },
                Probe {
                    fail: 1,
                    stdout: "EFGHABCD",
                    exit: 1,
                    calls: 1,
                    allocated: 0,
                    failures: 1,
                    peak: 0,
                },
                Probe {
                    fail: 2,
                    stdout: "ABCDEFGH",
                    exit: 2,
                    calls: 2,
                    allocated: 1,
                    failures: 1,
                    peak: 112,
                },
                Probe {
                    fail: 3,
                    stdout: "ABCDEFGH",
                    exit: 0,
                    calls: 2,
                    allocated: 1,
                    failures: 0,
                    peak: 224,
                },
            ],
        );
    }
}

#[test]
fn taken_product_child_keeps_real_buffer_ownership_during_allocation_failure() {
    let source = r#"
extern "C" fn putchar(code:i32)->i32;
fn emit(code:i32){unsafe{putchar(code)}}
resource Marker{code:i32}
impl Drop for Marker{fn drop(self:Self){emit(.code)}}
package Leaf{manages bytes:ByteBuffer;manages tag:Marker}
package Root{child:Leaf;manages side:Marker}
fn main()->i32{
    let bytes=bytes_new();if !bytes_reserve(bytes,4){return 1};
    let p=Root{child:Leaf{bytes:give bytes,tag:Marker{code:65}},side:Marker{code:66}};
    let f=()=>{let child=p.child;if !bytes_reserve(child.bytes,16){return 2};emit(89);return 0};
    return f()
}
"#;
    allocation_cases(
        "child-buffer",
        source,
        &[
            Probe {
                fail: 0,
                stdout: "YAB",
                exit: 0,
                calls: 2,
                allocated: 1,
                failures: 0,
                peak: 16,
            },
            Probe {
                fail: 1,
                stdout: "",
                exit: 1,
                calls: 1,
                allocated: 0,
                failures: 1,
                peak: 0,
            },
            Probe {
                fail: 2,
                stdout: "AB",
                exit: 2,
                calls: 2,
                allocated: 1,
                failures: 1,
                peak: 4,
            },
            Probe {
                fail: 3,
                stdout: "YAB",
                exit: 0,
                calls: 2,
                allocated: 1,
                failures: 0,
                peak: 16,
            },
        ],
    );
}
