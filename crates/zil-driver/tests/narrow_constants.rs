//! Source-level control for noncanonical narrow LLVM constants and cached O2.
#![cfg(all(feature = "llvm-api", target_os = "linux"))]
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
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-narrow-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn options(&self, backend: NativeBackend, optimization: Optimization) -> CompileOptions {
        CompileOptions {
            backend,
            optimization,
            stop_after: Stage::Build,
            output: Some(self.0.join("program")),
            object_cache: (backend == NativeBackend::LlvmApi).then(|| self.0.join("objects")),
            ..CompileOptions::default()
        }
    }
    fn run(&self, input: &[u8]) -> std::process::Output {
        let mut child = Command::new(self.0.join("program"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn negative_return_paths_survive_o2_and_warm_machine_cache() {
    for body in [
        "if x<0||x>64{return -2;}var remaining=x;while remaining>0{remaining=remaining-1;}return 0;",
        "if x<0||x>64{return -2;}if x==0{return 0;}return -1;",
    ] {
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            for optimization in [Optimization::O0, Optimization::O2] {
                let dir = Dir::new();
                let options = dir.options(backend, optimization);
                let source = SourceUnit::new(
                    FileId(0),
                    ModuleId(0),
                    dir.0.join("main.zil").to_string_lossy(),
                    format!(
                        "extern \"C\" fn getchar()->i32;fn choose(x:i32)->i32{{{body}}}fn main()->i32{{var x=0;unsafe{{x=getchar();}}return choose(x);}}"
                    ),
                );
                let mut session = CompilationSession::default();
                for warm in [false, true] {
                    let report = session.compile_source(&source, &options);
                    assert_eq!(
                        report.status,
                        ResultCategory::Success,
                        "{}",
                        report.to_json()
                    );
                    assert_eq!(report.verification_runs, 1);
                    if warm && backend == NativeBackend::LlvmApi {
                        assert!(report.native_codegen.unwrap().object_cache.hits > 0);
                    }
                    for (input, expected) in [
                        (b"".as_slice(), 254),
                        (b"\0", 0),
                        (b"1", if body.contains("while") { 0 } else { 255 }),
                        (b"Z", 254),
                    ] {
                        let result = dir.run(input);
                        assert_eq!(
                            result.status.code(),
                            Some(expected),
                            "{backend:?} {optimization:?} {body}"
                        );
                        assert!(result.stdout.is_empty() && result.stderr.is_empty());
                    }
                }
            }
        }
    }
}
#[test]
fn byte_stream_preserves_success_exit_and_all_bytes_at_o0_o2() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/mainline/byte_io/main.zil");
    for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
        for optimization in [Optimization::O0, Optimization::O2] {
            let dir = Dir::new();
            let options = dir.options(backend, optimization);
            let mut session = CompilationSession::default();
            for warm in [false, true] {
                let report = session.compile_path(&path, &options);
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{}",
                    report.to_json()
                );
                assert_eq!(report.verification_runs, 1);
                if warm && backend == NativeBackend::LlvmApi {
                    assert!(report.native_codegen.unwrap().object_cache.hits > 0);
                }
                for input in [
                    vec![],
                    b"hello, Zil!\n\0\x80".to_vec(),
                    b"Abc\0\xff\n".repeat(100),
                ] {
                    let result = dir.run(&input);
                    assert_eq!(
                        result.status.code(),
                        Some(0),
                        "{backend:?} {optimization:?}"
                    );
                    assert_eq!(result.stdout, input.to_ascii_uppercase());
                    assert!(result.stderr.is_empty());
                }
            }
        }
    }
}
