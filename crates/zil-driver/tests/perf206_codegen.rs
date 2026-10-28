//! Real-source regression tests for opt-in native codegen partitioning.
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::ResultCategory;
use zil_driver::{CodegenOptions, CompileOptions, Optimization, Stage, compile_path};

static SEQ: AtomicU64 = AtomicU64::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zil206-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn write(&self, file: &str, text: &str) {
        fs::write(self.0.join(file), text).unwrap();
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn options(dir: &Path, units: usize, jobs: usize, opt: Optimization) -> CompileOptions {
    CompileOptions {
        stop_after: Stage::Build,
        output: Some(dir.join("program")),
        clang: std::env::var_os("ZIL_CLANG").map(PathBuf::from),
        codegen: CodegenOptions { units, jobs },
        optimization: opt,
        ..CompileOptions::default()
    }
}
fn sample(dir: &Dir) {
    dir.write(
        "main.zil",
        "import a\nimport b\nfn main()->i32 { return i32(a.twice(11)+b.twice(10)); }\n",
    );
    dir.write("a.zil", "fn private_add(x:i64)->i64 { return x+1; }\npub fn twice(x:i64)->i64 { return private_add(x)*2; }\n");
    dir.write("b.zil", "pub fn twice(x:i64)->i64 { var y=x; var i:i64=0; while i<2 { y+=1; i+=1; } return y*2; }\n");
}

#[test]
fn same_verified_ir_and_observable_result_for_serial_and_parallel_units() {
    let d = Dir::new();
    sample(&d);
    for opt in [Optimization::O0, Optimization::O2] {
        let mut original = None;
        for (units, jobs) in [(1, 1), (4, 1), (4, 2), (4, 4)] {
            let report = compile_path(&d.0.join("main.zil"), &options(&d.0, units, jobs, opt));
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{:?}",
                report.diagnostics
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            if units > 1 {
                assert!(native.actual_units > 1, "{:?}", native);
                assert!(native.jobs <= jobs && native.jobs <= native.actual_units);
                assert_eq!(report.commands.len(), native.actual_units + 1);
                assert!(
                    report.commands[..native.actual_units]
                        .iter()
                        .all(|c| c.iter().any(|arg| arg == "-c"))
                );
                assert!(
                    !report
                        .commands
                        .last()
                        .unwrap()
                        .iter()
                        .any(|arg| arg == "ir")
                );
            }
            let llvm = fs::read(&report.artifacts["llvm"]).unwrap();
            if let Some(ref expected) = original {
                assert_eq!(&llvm, expected);
            } else {
                original = Some(llvm);
            }
            assert_eq!(
                Command::new(&report.artifacts["executable"])
                    .status()
                    .unwrap()
                    .code(),
                Some(48)
            );
        }
    }
}

#[test]
fn return_value_wrapper_private_format_and_cross_module_call() {
    let d = Dir::new();
    sample(&d);
    for opt in [Optimization::O0, Optimization::O2] {
        let mut opts = options(&d.0, 4, 2, opt);
        opts.entry = Some("main".into());
        let report = compile_path(&d.0.join("main.zil"), &opts);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{:?}",
            report.diagnostics
        );
        assert!(report.native_codegen.as_ref().unwrap().actual_units > 1);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"48\n");
    }
}

#[test]
fn single_source_group_keeps_original_native_path() {
    let d = Dir::new();
    d.write("main.zil", "fn main()->i32 { return 7; }");
    let r = compile_path(
        &d.0.join("main.zil"),
        &options(&d.0, 8, 4, Optimization::O0),
    );
    assert_eq!(r.status, ResultCategory::Success, "{:?}", r.diagnostics);
    assert_eq!(r.native_codegen.as_ref().unwrap().actual_units, 1);
    assert!(r.native_codegen.as_ref().unwrap().fallback.is_some());
    assert_eq!(r.commands.len(), 1);
    assert_eq!(
        Command::new(&r.artifacts["executable"])
            .status()
            .unwrap()
            .code(),
        Some(7)
    );
}

#[test]
fn unsafe_source_is_rejected_before_any_native_worker() {
    let d = Dir::new();
    d.write(
        "main.zil",
        "fn main()->i32 { var x:i32=1; let r=&x; x=2; return *r; }",
    );
    let a = compile_path(
        &d.0.join("main.zil"),
        &options(&d.0, 1, 1, Optimization::O0),
    );
    let b = compile_path(
        &d.0.join("main.zil"),
        &options(&d.0, 4, 4, Optimization::O0),
    );
    assert_eq!(a.status, ResultCategory::SafeReject, "{:?}", a.diagnostics);
    assert_eq!(a.diagnostics[0].code, "E-BORROW-CONFLICT");
    assert_eq!(a.status, b.status);
    assert_eq!(a.diagnostics, b.diagnostics);
    assert!(a.commands.is_empty() && b.commands.is_empty());
    assert!(!d.0.join("program").exists());
}

#[test]
fn checked_overflow_still_traps_across_codegen_boundaries() {
    let d = Dir::new();
    d.write(
        "main.zil",
        "import calc\nfn main()->i32 { return calc.add(2147483647); }",
    );
    d.write("calc.zil", "pub fn add(x:i32)->i32 { return x+1; }");
    for opt in [Optimization::O0, Optimization::O2] {
        for units in [1, 4] {
            let report = compile_path(&d.0.join("main.zil"), &options(&d.0, units, 2, opt));
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{:?}",
                report.diagnostics
            );
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            assert!(!output.status.success());
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(output.status.signal(), Some(4));
            }
        }
    }
}

#[test]
fn owning_closure_and_byte_runtime_preserve_full_consumer_output() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/product/108/programs104/retained/zil/main.zil");
    let d = Dir::new();
    for opt in [Optimization::O0, Optimization::O2] {
        let mut expected = None;
        for units in [1, 8] {
            let report = compile_path(&root, &options(&d.0, units, 4, opt));
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{:?}",
                report.diagnostics
            );
            assert_eq!(report.verification_runs, 1);
            use std::io::Write;
            use std::process::Stdio;
            let mut child = Command::new(&report.artifacts["executable"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(b"abcdefghijklmnopqrstuvwxyz0123456789")
                .unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(output.status.success());
            if let Some(ref expected) = expected {
                assert_eq!(&output.stdout, expected);
            } else {
                expected = Some(output.stdout);
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn failed_worker_drains_all_started_tools_and_preserves_previous_outputs() {
    use std::os::unix::fs::PermissionsExt;
    let d = Dir::new();
    sample(&d);
    d.write("program", "old executable");
    d.write("program.ll", "old ir");
    let log = d.0.join("workers.log");
    d.write(
        "clang-fail",
        &format!(
            r#"#!/bin/sh
input=''; output=''; prev=''
for arg in "$@"; do
 case "$arg" in *.ll) input="$arg";; esac
 if [ "$prev" = '-o' ]; then output="$arg"; fi
 prev="$arg"
done
name=$(basename "$input")
echo "start $name" >> '{log}'
sleep 0.05
echo "end $name" >> '{log}'
case "$name" in unit-0.ll) echo 'forced unit failure' >&2; exit 7;; esac
printf object > "$output"
"#,
            log = log.display()
        ),
    );
    fs::set_permissions(d.0.join("clang-fail"), fs::Permissions::from_mode(0o755)).unwrap();
    let mut opts = options(&d.0, 4, 2, Optimization::O0);
    opts.clang = Some(d.0.join("clang-fail"));
    let report = compile_path(&d.0.join("main.zil"), &opts);
    assert_eq!(report.status, ResultCategory::InfraError);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-LLVM-COMPILE")
    );
    assert_eq!(
        fs::read_to_string(d.0.join("program")).unwrap(),
        "old executable"
    );
    assert_eq!(
        fs::read_to_string(d.0.join("program.ll")).unwrap(),
        "old ir"
    );
    let log = fs::read_to_string(&log).unwrap();
    assert_eq!(
        log.lines().filter(|s| s.starts_with("start ")).count(),
        log.lines().filter(|s| s.starts_with("end ")).count()
    );
    assert!(log.contains("start unit-0.ll"));
    assert!(!fs::read_dir(&d.0).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".zil-")
    }));
}

#[test]
fn ordinary_function_values_cross_unit_boundaries_without_abi_changes() {
    let d = Dir::new();
    d.write("main.zil","import factory\nimport apply\nfn main()->i32 { let f=factory.get(); return i32(apply.run(f,19)); }");
    d.write(
        "factory.zil",
        "fn twice(x:i64)->i64 { return x*2; } pub fn get()->fn(i64)->i64 { return twice; }",
    );
    d.write(
        "apply.zil",
        "pub fn run(f:fn(i64)->i64,x:i64)->i64 { return f(x)+1; }",
    );
    for opt in [Optimization::O0, Optimization::O2] {
        for units in [1, 8] {
            let report = compile_path(&d.0.join("main.zil"), &options(&d.0, units, 4, opt));
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{:?}",
                report.diagnostics
            );
            assert_eq!(
                Command::new(&report.artifacts["executable"])
                    .status()
                    .unwrap()
                    .code(),
                Some(39)
            );
        }
    }
}

#[test]
fn undefined_foreign_symbol_still_fails_and_never_publishes_partial_native_code() {
    let d = Dir::new();
    d.write(
        "main.zil",
        "import broken\nfn main()->i32 { return broken.call(); }",
    );
    d.write("broken.zil","extern \"C\" fn zil206_absent_foreign()->i32; pub fn call()->i32 { unsafe { return zil206_absent_foreign(); } }");
    for opt in [Optimization::O0, Optimization::O2] {
        for units in [1, 8] {
            let report = compile_path(&d.0.join("main.zil"), &options(&d.0, units, 4, opt));
            assert_eq!(
                report.status,
                ResultCategory::InfraError,
                "{:?}",
                report.diagnostics
            );
            assert_eq!(report.diagnostics[0].code, "E-LLVM-COMPILE");
            assert!(!d.0.join("program").exists());
            assert!(!report.artifacts.contains_key("executable"));
        }
    }
}
