//! Independent234: compare every returned bit with a Rust byte oracle.
//! Process exit only reports success, never the value under test.
#![cfg(target_os = "linux")]
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, NativeBackend, Optimization, Stage};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const SOURCE: &str = r#"
extern "C" fn getchar() -> i32;
extern "C" fn putchar(n: i32) -> i32;
fn choose32(x: i32) -> i32 {
    if x < 0 { return -2147483648 }
    if x == 0 { return -257 }
    if x == 1 { return -256 }
    if x == 2 { return -1 }
    if x == 3 { return 2147483647 }
    if x == 4 { return -2147483392 }
    var n = x
    while n > 4 { n = n - 1 }
    return -2
}
fn choose64(x: i32) -> i64 {
    if x < 0 { return -9223372036854775808 }
    if x == 0 { return -4294967297 }
    if x == 1 { return -4294967296 }
    if x == 2 { return -1 }
    if x == 3 { return 9223372036854775807 }
    if x == 4 { return -9223372036854775552 }
    return -2
}
fn emit32(n: i32) {
    unsafe {
        putchar(n & 255)
        putchar((n >> 8) & 255)
        putchar((n >> 16) & 255)
        putchar((n >> 24) & 255)
    }
}
fn emit64(n: i64) {
    unsafe {
        putchar(i32(n & 255))
        putchar(i32((n >> 8) & 255))
        putchar(i32((n >> 16) & 255))
        putchar(i32((n >> 24) & 255))
        putchar(i32((n >> 32) & 255))
        putchar(i32((n >> 40) & 255))
        putchar(i32((n >> 48) & 255))
        putchar(i32((n >> 56) & 255))
    }
}
fn main() -> i32 {
    var x = 0
    unsafe { x = getchar() }
    emit32(choose32(x))
    emit64(choose64(x))
    return 0
}
"#;

fn backends() -> Vec<NativeBackend> {
    let mut result = vec![NativeBackend::Clang];
    if cfg!(feature = "llvm-api") {
        result.push(NativeBackend::LlvmApi);
    }
    result
}
struct Dir(PathBuf, bool);
impl Dir {
    fn new(backend: NativeBackend, optimization: Optimization) -> Self {
        let root = std::env::var_os("ZIL_TEST_REPORT_DIR").map(PathBuf::from);
        let keep = root.is_some();
        let path = root.unwrap_or_else(std::env::temp_dir).join(format!(
            "review234-{backend:?}-{optimization:?}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, keep)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
#[test]
fn all_i32_and_i64_return_bits_match_independent_oracle_cold_and_warm() {
    // Several pairs deliberately have the same low exit byte: truncated
    // process codes could not distinguish them, but the 12-byte oracle can.
    let cases = [
        (vec![], i32::MIN, i64::MIN),
        (vec![0], -257, -4_294_967_297),
        (vec![1], -256, -4_294_967_296),
        (vec![2], -1, -1),
        (vec![3], i32::MAX, i64::MAX),
        (vec![4], i32::MIN + 256, i64::MIN + 256),
        (vec![255], -2, -2),
    ];
    for backend in backends() {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = Dir::new(backend, optimization);
            fs::write(dir.0.join("source.zil"), SOURCE).unwrap();
            let source = SourceUnit::new(FileId(0), ModuleId(0), "review234.zil", SOURCE);
            let options = CompileOptions {
                backend,
                optimization,
                stop_after: Stage::Build,
                output: Some(dir.0.join("program")),
                object_cache: (backend == NativeBackend::LlvmApi).then(|| dir.0.join("objects")),
                ..CompileOptions::default()
            };
            let mut session = CompilationSession::default();
            for warm in [false, true] {
                let report = session.compile_source(&source, &options);
                fs::write(dir.0.join(format!("compile-{warm}.json")), report.to_json()).unwrap();
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                let summary = report.native_codegen.as_ref().unwrap();
                assert_eq!(summary.backend, backend);
                assert!(summary.fallback.is_none());
                if backend == NativeBackend::LlvmApi {
                    assert!(!summary.object_reports.is_empty());
                    if warm {
                        assert!(summary.object_cache.hits > 0);
                        assert!(
                            summary
                                .object_reports
                                .iter()
                                .any(|r| r.contains("\"input_kind\":\"object-cache\""))
                        );
                    } else {
                        assert!(
                            summary
                                .object_reports
                                .iter()
                                .all(|r| r.contains("\"input_kind\":\"builder\""))
                        );
                    }
                }
                for (index, (input, expected32, expected64)) in cases.iter().enumerate() {
                    let mut expected = expected32.to_le_bytes().to_vec();
                    expected.extend_from_slice(&expected64.to_le_bytes());
                    let mut child = Command::new(dir.0.join("program"))
                        .stdin(Stdio::piped())
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped())
                        .spawn()
                        .unwrap();
                    child.stdin.take().unwrap().write_all(input).unwrap();
                    let output = child.wait_with_output().unwrap();
                    let name = format!("{warm}-{index}");
                    fs::write(dir.0.join(format!("{name}.input")), input).unwrap();
                    fs::write(dir.0.join(format!("{name}.expected")), &expected).unwrap();
                    fs::write(dir.0.join(format!("{name}.stdout")), &output.stdout).unwrap();
                    fs::write(dir.0.join(format!("{name}.stderr")), &output.stderr).unwrap();
                    fs::write(
                        dir.0.join(format!("{name}.exit")),
                        format!("{:?}", output.status.code()),
                    )
                    .unwrap();
                    assert_eq!(
                        output.status.code(),
                        Some(0),
                        "{backend:?}/{optimization:?}/{warm}/{index}"
                    );
                    assert!(output.stderr.is_empty());
                    assert_eq!(
                        output.stdout, expected,
                        "{backend:?}/{optimization:?}/{warm}/{index}: i32={expected32}, i64={expected64}"
                    );
                }
            }
        }
    }
}
