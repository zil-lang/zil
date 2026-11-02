//! Actual-source controls for checked division during explicit incremental O2.
#![cfg(all(feature = "llvm-api", target_os = "linux"))]

use std::{
    fs,
    io::Write,
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, NativeBackend, Optimization, Stage};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-divisor-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn compile(&self, text: &str, backend: NativeBackend, incremental_o2: bool) -> String {
        let options = CompileOptions {
            backend,
            incremental_o2,
            optimization: Optimization::O2,
            stop_after: Stage::Build,
            entry: Some("result".into()),
            output: Some(self.0.join("program")),
            ..CompileOptions::default()
        };
        let source = SourceUnit::new(
            FileId(0),
            ModuleId(0),
            self.0.join("main.zil").to_string_lossy(),
            text,
        );
        let report = CompilationSession::default().compile_source(&source, &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{}", report.to_json());
        fs::read_to_string(&report.artifacts["llvm"]).unwrap()
    }
    fn run(&self, seed: u8) -> Output {
        let mut child = Command::new(self.0.join("program"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&[seed]).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("divisor control timed out");
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        child.wait_with_output().unwrap()
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn helper(llvm: &str) -> &str {
    llvm.split("define ")
        .find(|part| {
            part.lines()
                .next()
                .is_some_and(|line| line.contains("@zil.fn.0("))
        })
        .and_then(|part| part.split_once("\n}"))
        .map(|(body, _)| body)
        .expect("first source helper")
}
fn source(body: &str, setup: &str, call: &str) -> String {
    format!(
        "{body} extern \"C\" fn getchar()->i32; fn result()->i64{{var seed:i32=0;unsafe{{seed=getchar();}}{setup}return i64({call});}}"
    )
}
fn returned(output: Output, expected: i64) {
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, format!("{expected}\n").as_bytes());
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn direct_safe_divisors_reduce_real_cfg_without_changing_native_results() {
    type SafeCase = (&'static str, &'static str, fn(i64) -> i64);
    let cases: [SafeCase; 5] = [
        (
            "fn f(x:i32)->i32{return (x-127)/3+(x-127)%3;}",
            "f(seed)",
            |x| (x - 127) / 3 + (x - 127) % 3,
        ),
        (
            "fn f(x:i64)->i64{return (x-127)/7+(x-127)%7;}",
            "f(i64(seed))",
            |x| (x - 127) / 7 + (x - 127) % 7,
        ),
        ("fn f(x:u8)->u8{return x/3+x%3;}", "f(u8(seed))", |x| {
            x / 3 + x % 3
        }),
        (
            "fn f(x:u16)->u16{return x/257+x%257;}",
            "f(u16(seed)*255)",
            |x| (x * 255) / 257 + (x * 255) % 257,
        ),
        (
            "fn f(x:i32)->i32{return x/-2147483648+x% -2147483648;}",
            "f(seed-127)",
            |x| x - 127,
        ),
    ];
    for backend in [NativeBackend::LlvmApi, NativeBackend::Clang] {
        for (body, call, expected) in cases {
            let dir = Dir::new();
            let text = source(body, "", call);
            let original = dir.compile(&text, backend, false);
            for seed in [0, 1, 127, 255] {
                returned(dir.run(seed), expected(i64::from(seed)));
            }
            let optimized = dir.compile(&text, backend, true);
            for seed in [0, 1, 127, 255] {
                returned(dir.run(seed), expected(i64::from(seed)));
            }
            // Inspect the actual emitted helper: the same div/rem survives while
            // its provably false checked edges are removed before LLVM O2.
            let (old, new) = (helper(&original), helper(&optimized));
            assert!(
                new.matches("br i1 ").count() < old.matches("br i1 ").count(),
                "{old}\n{new}"
            );
            for operation in ["sdiv ", "srem ", "udiv ", "urem "] {
                assert_eq!(
                    old.matches(operation).count(),
                    new.matches(operation).count(),
                    "{old}\n{new}"
                );
            }
        }
    }
}

#[test]
fn zero_minus_one_and_runtime_divisors_keep_their_checked_behavior() {
    let cases = [
        (
            "fn f(x:i32)->i32{return x/-1;}",
            "var n:i32=seed;if seed==0{n=-2147483648;}",
            "f(n)",
            Some(-1),
            Some(-2),
        ),
        (
            "fn f(x:i64)->i64{return x% -1;}",
            "var n:i64=i64(seed);if seed==0{n=-9223372036854775808;}",
            "f(n)",
            Some(0),
            Some(0),
        ),
        ("fn f(x:u8)->u8{return x/0;}", "", "f(u8(seed))", None, None),
        ("fn f(x:i32)->i32{return x%0;}", "", "f(seed)", None, None),
        (
            "fn f(x:i32,y:i32)->i32{return x/y;}",
            "var n:i32=seed;if seed==0{n=-2147483648;}",
            "f(n,seed-1)",
            None,
            Some(2),
        ),
    ];
    for backend in [NativeBackend::LlvmApi, NativeBackend::Clang] {
        for (body, setup, call, expected_one, expected_two) in cases {
            let dir = Dir::new();
            let text = source(body, setup, call);
            let mut original: Option<String> = None;
            for incremental_o2 in [false, true] {
                let llvm = dir.compile(&text, backend, incremental_o2);
                if let Some(old) = &original {
                    assert_eq!(helper(old), helper(&llvm));
                }
                for (seed, expected) in [(0, None), (1, expected_one), (2, expected_two)] {
                    let output = dir.run(seed);
                    if let Some(expected) = expected {
                        returned(output, expected);
                    } else {
                        assert_eq!(output.status.signal(), Some(4), "{output:?}");
                        assert!(
                            output.stdout.is_empty() && output.stderr.is_empty(),
                            "{output:?}"
                        );
                    }
                }
                original = Some(llvm);
            }
        }
    }
}
