//! Real ZIL source traverses types, analyze, independent verify, lower and LLVM.
//! This is the C1 Copy scalar closure slice, not general closure/escape support.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, Stage, compile_source};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-closure-native-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn native(text: &str, expected: i64) {
    let directory = Directory::new();
    let source = SourceUnit::new(FileId(0), ModuleId(0), "closures_native.zil", text);
    let options = CompileOptions {
        stop_after: Stage::Build,
        entry: Some("result".into()),
        output: Some(directory.0.join("program")),
        ..CompileOptions::default()
    };
    let report = compile_source(&source, &options);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Link);
    assert_eq!(report.verification_runs, 1);
    let llvm = fs::read_to_string(&report.artifacts["llvm"]).unwrap();
    assert!(llvm.contains("%zil_env_0 = type"));
    let mut child = Command::new(&report.artifacts["executable"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native closure execution exceeded five seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("{expected}\n")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn runtime_parameters_create_distinct_environments_at_the_same_source_site() {
    native(
        r#"
        fn calculate(base: i64) -> i64 {
            let f = move (delta) => base + delta;
            return f(1) + f(2);
        }
        fn result() -> i64 { return calculate(40) + calculate(100); }
        "#,
        286,
    );
}

#[test]
fn move_snapshot_survives_outer_mutation_local_copy_and_later_shadowing() {
    native(
        r#"
        fn result() -> i64 {
            var base: i64 = 40;
            var flag = true;
            let f = move (x) => {
                if flag { return base + x; } else { return 0; }
            };
            base = 1000;
            flag = false;
            var g = f;
            g = f;
            if true {
                let base: i64 = 99;
                return g(1) + g(2) + base;
            } else { return 0; }
        }
        "#,
        182,
    );
}

#[test]
fn unit_capture_and_empty_unit_result_work_through_the_source_pipeline() {
    native(
        r#"
        fn unit() {}
        fn result() -> i64 {
            let blank = unit();
            let f = move (x) => { blank; return x; };
            let run = () => {};
            run();
            return f(42);
        }
        "#,
        42,
    );
}
