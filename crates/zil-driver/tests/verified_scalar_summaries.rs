//! Actual-source controls for bounded summaries over currently verified scalar CFGs.
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
            "zil-summary-{}-{}",
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
        self.compile_options(text, options)
    }
    fn compile_main(&self, text: &str, closed: bool) -> String {
        let mut options = CompileOptions {
            backend: NativeBackend::LlvmApi,
            incremental_o2: true,
            closed_executable: closed,
            optimization: Optimization::O2,
            stop_after: Stage::Build,
            output: Some(self.0.join("program")),
            ..CompileOptions::default()
        };
        options.codegen.units = 8;
        options.codegen.jobs = 2;
        self.compile_options(text, options)
    }
    fn compile_options(&self, text: &str, options: CompileOptions) -> String {
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
        if let Err(error) = child.stdin.take().unwrap().write_all(&[seed]) {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("summary control timed out");
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

fn first_function(llvm: &str) -> &str {
    llvm.split("define ")
        .find(|part| {
            part.lines()
                .next()
                .is_some_and(|line| line.contains("@zil.fn.0("))
        })
        .and_then(|part| part.split_once("\n}"))
        .map(|(body, _)| body)
        .unwrap()
}

#[test]
fn summaries_preserve_checked_scalar_results_and_avoid_machine_expansion() {
    let cases = [
        (
            "fn result()->i32{unit(());return 7;} fn unit(x:())->(){}",
            "7\n",
        ),
        (
            "fn result()->i64{var sum:i64=0;var i:i64=0;while i<6{sum=sum+step(i);i=i+1;}return sum;} fn step(x:i64)->i64{if x<3{return x*2;}else{return x+4;}}",
            "30\n",
        ),
        (
            "fn result()->u8{var x:u8=250;let y:u8=5;x=x+y;return x;}",
            "255\n",
        ),
        ("fn result()->u16{return 65535-1;}", "65534\n"),
        (
            "fn result()->i64{return -9223372036854775807-1;}",
            "-9223372036854775808\n",
        ),
        ("fn result()->bool{return !(3<2)&&(4==4);}", "1\n"),
    ];
    for (index, (source, expected)) in cases.iter().enumerate() {
        let dir = Dir::new();
        let backends: &[NativeBackend] = if index == 0 {
            &[NativeBackend::LlvmApi, NativeBackend::Clang]
        } else {
            &[NativeBackend::LlvmApi]
        };
        for &backend in backends {
            for enabled in [false, true] {
                let llvm = dir.compile(source, backend, enabled);
                let output = dir.run(7);
                assert!(output.status.success(), "{source}: {output:?}");
                assert_eq!(output.stdout, expected.as_bytes(), "{source}");
                assert!(output.stderr.is_empty());
                if enabled {
                    let body = first_function(&llvm);
                    assert!(!body.contains("call "), "{body}");
                    assert!(!body.contains(" br "), "{body}");
                    assert_eq!(
                        body.lines()
                            .filter(|line| line.trim_start().starts_with("ret "))
                            .count(),
                        1
                    );
                }
            }
        }
    }
}

#[test]
fn effects_opaque_input_recursion_and_exhausted_work_keep_runtime_paths() {
    let cases = [
        (
            "fn result()->i32{unsafe{putchar(88);}return 7;} extern \"C\" fn putchar(x:i32)->i32;",
            "X7\n",
        ),
        (
            "fn result()->i32{var x:i32=0;unsafe{x=getchar();}return x+1;} extern \"C\" fn getchar()->i32;",
            "8\n",
        ),
        (
            "fn result()->i32{return recursive(3);} fn recursive(x:i32)->i32{if x==0{return 1;}return recursive(x-1);}",
            "1\n",
        ),
        (
            "fn result()->i32{var i:i32=0;while i<200000{i=i+1;}return i;}",
            "200000\n",
        ),
    ];
    for (source, expected) in cases {
        let dir = Dir::new();
        let llvm = dir.compile(source, NativeBackend::LlvmApi, true);
        let output = dir.run(7);
        assert!(output.status.success(), "{source}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{source}");
        assert!(output.stderr.is_empty());
        let body = first_function(&llvm);
        assert!(
            body.contains("call ") || body.contains("br "),
            "fallback lost its original body: {body}"
        );
    }
}

#[test]
fn overflow_is_not_replaced_by_a_returning_summary() {
    for source in [
        "fn result()->i64{return plus(9223372036854775807);} fn plus(x:i64)->i64{return x+1;}",
        "fn result()->u8{return plus(255);} fn plus(x:u8)->u8{return x+1;}",
        "fn result()->i64{return neg(-9223372036854775807-1);} fn neg(x:i64)->i64{return -x;}",
    ] {
        let dir = Dir::new();
        let llvm = dir.compile(source, NativeBackend::LlvmApi, true);
        assert!(first_function(&llvm).contains("call "));
        let output = dir.run(0);
        assert_eq!(output.status.signal(), Some(4), "{source}: {output:?}");
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn effect_before_overflow_is_still_observed() {
    let source = "fn result()->i64{unsafe{putchar(88);fflush(c_null());}return plus(9223372036854775807);} fn plus(x:i64)->i64{return x+1;} extern \"C\" fn putchar(x:i32)->i32; extern \"C\" fn fflush(stream:c_handle)->i32;";
    let dir = Dir::new();
    dir.compile(source, NativeBackend::LlvmApi, true);
    let output = dir.run(0);
    assert_eq!(output.status.signal(), Some(4), "{output:?}");
    assert_eq!(output.stdout, b"X");
}

#[test]
fn constant_calls_inside_effectful_runtime_callers_keep_order_and_dynamic_calls() {
    let source = "fn result()->i32{var x:i32=0;unsafe{x=getchar();putchar(65);}let n=known();unsafe{putchar(66);}return dynamic(x)+n;}fn known()->i32{return -9;}fn dynamic(x:i32)->i32{return x;}extern \"C\" fn getchar()->i32;extern \"C\" fn putchar(x:i32)->i32;";
    for enabled in [false, true] {
        let dir = Dir::new();
        let llvm = dir.compile(source, NativeBackend::LlvmApi, enabled);
        let output = dir.run(7);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"AB-2\n");
        assert!(output.stderr.is_empty());
        let body = first_function(&llvm);
        assert!(body.contains("@getchar("));
        assert!(body.contains("@putchar("));
        assert!(
            body.contains("@zil.fn.2("),
            "runtime argument must remain: {body}"
        );
        assert_eq!(body.contains("@zil.fn.1("), !enabled);
    }
}

#[test]
fn constant_fact_does_not_hide_a_later_effectful_or_trapping_zero_arg_call() {
    let source = "fn result()->i64{let n=known();unsafe{putchar(n);fflush(c_null());}return fail();}fn known()->i32{return 88;}fn fail()->i64{return 9223372036854775807+1;}extern \"C\" fn putchar(x:i32)->i32;extern \"C\" fn fflush(stream:c_handle)->i32;";
    let dir = Dir::new();
    let llvm = dir.compile(source, NativeBackend::LlvmApi, true);
    assert!(first_function(&llvm).contains("@zil.fn.2("));
    let output = dir.run(0);
    assert_eq!(output.status.signal(), Some(4), "{output:?}");
    assert_eq!(output.stdout, b"X");

    let source = "fn result()->i32{let f:fn()->i32=known;return f();}fn known()->i32{return 12;}";
    let llvm = dir.compile(source, NativeBackend::LlvmApi, true);
    assert!(
        first_function(&llvm).contains("call i32 %"),
        "indirect call retains its path"
    );
    let output = dir.run(0);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"12\n");
}

#[test]
fn closed_collection_omits_known_definitions_but_keeps_entry_and_effectful_helpers() {
    let source = "fn unused()->i32{return 19;}fn known()->i32{return 66;}fn effect()->i32{unsafe{putchar(65);}return 0;}extern \"C\" fn putchar(x:i32)->i32;fn main()->i32{effect();unsafe{putchar(known());}return 0;}";
    for closed in [false, true] {
        let dir = Dir::new();
        let llvm = dir.compile_main(source, closed);
        let output = dir.run(0);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, b"AB");
        assert!(output.stderr.is_empty());
        assert_eq!(llvm.contains("@zil.fn.0("), !closed);
        assert_eq!(llvm.contains("@zil.fn.1("), !closed);
        assert!(llvm.contains("@zil.fn.2("), "effect helper remains: {llvm}");
        assert!(llvm.contains("@zil.fn.4("), "source entry remains: {llvm}");
        assert!(llvm.contains("define i32 @main("), "native wrapper remains");
    }
}

#[test]
fn closed_collection_falls_back_when_a_function_address_can_be_observed() {
    let source = "fn known()->i32{return 65;}extern \"C\" fn putchar(x:i32)->i32;fn main()->i32{let f:fn()->i32=known;unsafe{putchar(f());}return 0;}";
    let dir = Dir::new();
    let llvm = dir.compile_main(source, true);
    assert!(
        llvm.contains("@zil.fn.0("),
        "addressed definition remains: {llvm}"
    );
    assert!(llvm.contains("call i32 %"), "indirect path remains: {llvm}");
    let output = dir.run(0);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(output.stdout, b"A");
    assert!(output.stderr.is_empty());
}

#[test]
fn closed_collection_removes_a_known_callee_without_hiding_a_trapping_callee() {
    let source = "fn known()->i32{return 1;}fn fail()->i32{return 2147483647+1;}fn main()->i32{return known()+fail();}";
    let dir = Dir::new();
    let llvm = dir.compile_main(source, true);
    assert!(!llvm.contains("@zil.fn.0("), "known definition is omitted");
    assert!(llvm.contains("@zil.fn.1("), "trapping definition remains");
    let output = dir.run(0);
    assert_eq!(output.status.signal(), Some(4), "{output:?}");
    assert!(output.stdout.is_empty());
}
