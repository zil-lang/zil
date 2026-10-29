//! Real-source behavior across the O0 SSA preparation, Clang reference and O2.
#![cfg(all(feature = "llvm-api", target_os = "linux"))]

use std::{
    fs,
    io::Write,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::ResultCategory;
use zil_driver::{
    CodegenOptions, CompileOptions, NativeBackend, Optimization, Stage, compile_path,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-o0-ssa-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, source: &str) {
        fs::write(self.0.join(name), source).unwrap();
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy, Debug)]
enum Exit {
    Code(i32),
    Signal(i32),
}

fn differential(dir: &Dir, source: &Path, cases: &[(&[u8], Exit, &[u8])]) {
    let mut reference_ir = None;
    for (backend, optimization, units, jobs) in [
        (NativeBackend::Clang, Optimization::O0, 1, 1),
        (NativeBackend::LlvmApi, Optimization::O0, 1, 1),
        (NativeBackend::LlvmApi, Optimization::O0, 4, 4),
        (NativeBackend::LlvmApi, Optimization::O2, 1, 1),
    ] {
        let report = compile_path(
            source,
            &CompileOptions {
                backend,
                optimization,
                codegen: CodegenOptions { units, jobs },
                stop_after: Stage::Build,
                output: Some(dir.0.join("program")),
                object_cache: None,
                ..CompileOptions::default()
            },
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        if backend == NativeBackend::LlvmApi {
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert_eq!(native.object_reports.len(), native.actual_units);
            assert!(!native.object_cache.enabled);
            if units > 1 {
                assert!(native.actual_units > 1, "{}", report.to_json());
            }
            let pipeline = match optimization {
                Optimization::O0 => "\"pipeline\":\"default<O0>,function(mem2reg)\"",
                Optimization::O2 => "\"pipeline\":\"default<O2>\"",
            };
            for receipt in &native.object_reports {
                assert!(receipt.contains(pipeline), "{receipt}");
                assert!(
                    receipt.contains("\"verify_before_after\":true"),
                    "{receipt}"
                );
                assert!(receipt.contains("\"input_kind\":\"builder\""), "{receipt}");
                assert!(receipt.contains("\"parse_ns\":0"), "{receipt}");
            }
        }
        // This is the existing pre-pass sidecar, not a post-mem2reg IR dump.
        let ir = fs::read(&report.artifacts["llvm"]).unwrap();
        if let Some(reference) = &reference_ir {
            assert_eq!(&ir, reference);
        } else {
            reference_ir = Some(ir);
        }
        for &(input, exit, stdout) in cases {
            let mut child = Command::new(&report.artifacts["executable"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            let output = child.wait_with_output().unwrap();
            match exit {
                Exit::Code(code) => assert_eq!(output.status.code(), Some(code), "{output:?}"),
                Exit::Signal(signal) => {
                    assert_eq!(output.status.signal(), Some(signal), "{output:?}")
                }
            }
            assert_eq!(output.stdout, stdout, "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}

#[test]
fn runtime_loop_and_branch_values_survive_phi_promotion() {
    let dir = Dir::new();
    dir.write(
        "main.zil",
        r#"import calc
extern "C" fn getchar()->i32;
fn main()->i32 {
    var c:i32=0;
    unsafe { c=getchar(); }
    return calc.compute(c-48);
}"#,
    );
    dir.write(
        "calc.zil",
        r#"pub fn compute(n:i32)->i32 {
    var index:i32=0;
    var total:i32=0;
    while index<n {
        if index<3 { total+=index; } else { total+=2; }
        index+=1;
    }
    if total>10 { total-=1; } else { total+=1; }
    return total;
}"#,
    );
    differential(
        &dir,
        &dir.0.join("main.zil"),
        &[
            (b"0", Exit::Code(1), b""),
            (b"1", Exit::Code(1), b""),
            (b"3", Exit::Code(4), b""),
            (b"8", Exit::Code(12), b""),
        ],
    );
}

#[test]
fn actual_callback_addresses_and_sequential_reborrows_keep_their_storage() {
    let dir = Dir::new();
    dir.write(
        "main.zil",
        "import borrowed\nfn main()->i32 { return borrowed.check(); }",
    );
    // The same address-use shape as the current impl189_address consumer.
    dir.write(
        "borrowed.zil",
        "pub fn check()->i32{var n=20;let p=&mut n;let read=(q:&i32)->i32=>*q;let bump=(q:&mut i32)->i32=>{*q+=1;return *q};let old=read(p);let a=bump(p);let b=bump(p);return old+a+b+n-85}",
    );
    differential(&dir, &dir.0.join("main.zil"), &[(b"", Exit::Code(0), b"")]);
}

#[test]
fn field_exit_preserves_dynamic_store_and_cleanup_order() {
    let dir = Dir::new();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/mainline/zui_language177/field_exit.zil");
    differential(
        &dir,
        &source,
        &[
            (b"0", Exit::Code(1), b"HZ"),
            (b"1", Exit::Code(1), b"DHZ"),
            (b"2", Exit::Code(1), b"DEHZ"),
            (b"3", Exit::Code(1), b"DEFHZ"),
            (b"9", Exit::Code(9), b""),
        ],
    );
}

#[test]
fn checked_overflow_still_traps_across_module_boundaries() {
    let dir = Dir::new();
    dir.write(
        "main.zil",
        "import checked\nfn main()->i32 { return checked.add(2147483647); }",
    );
    dir.write("checked.zil", "pub fn add(x:i32)->i32 { return x+1; }");
    differential(
        &dir,
        &dir.0.join("main.zil"),
        &[(b"", Exit::Signal(4), b"")],
    );
}
