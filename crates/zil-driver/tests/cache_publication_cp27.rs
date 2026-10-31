#![cfg(target_os = "linux")]
//! A newly built ELF can occupy a predicted memo temporary path. A cache miss
//! must remain advisory when create_new finds that independently owned output.
use std::{fs, path::PathBuf, process::Command};
use zil_base::ResultCategory;
use zil_driver::{CompileOptions, CompileReport, NativeBackend, Stage, compile_path};

const CHILD: &str = "ZIL_CP27_QUERY_OUTPUT_COLLISION";

fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
    let executable = &report.artifacts["executable"];
    assert!(fs::read(executable).unwrap().starts_with(b"\x7fELF"));
    let native = Command::new(executable).output().unwrap();
    assert!(native.status.success(), "{native:?}");
    assert!(native.stdout.is_empty());
    assert!(native.stderr.is_empty());
}

#[test]
fn checkpoint_temp_collision_keeps_real_elf_and_allows_repair_and_hit() {
    let Some(root) = std::env::var_os(CHILD) else {
        let root = std::env::temp_dir().join(format!("zil-cp27-output-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("main.zil"), "fn main()->i32=>0\n").unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "checkpoint_temp_collision_keeps_real_elf_and_allows_repair_and_hit",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, &root)
            .output()
            .unwrap();
        assert!(
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "failure retained at {}: {}\n{}\n{}",
            root.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        fs::remove_dir_all(root).unwrap();
        return;
    };
    let root = PathBuf::from(root);
    let directory = root.join("cache/frontend-cp12-v1");
    fs::create_dir_all(&directory).unwrap();
    let output = directory.join(format!(".pending-{}-0", std::process::id()));
    let mut options = CompileOptions {
        stop_after: Stage::Build,
        backend: NativeBackend::Clang,
        output: Some(output.clone()),
        ..CompileOptions::default()
    };
    let source = root.join("main.zil");
    let control = compile_path(&source, &options);
    success(&control);
    assert_eq!(control.query_cache.state, "disabled");

    options.query_cache = Some(root.join("cache"));
    let collision = compile_path(&source, &options);
    success(&collision);
    assert_eq!(collision.query_cache.state, "miss");
    assert!(!collision.query_cache.stored);
    assert!(
        collision
            .query_cache
            .write_reason
            .as_deref()
            .is_some_and(|reason| {
                reason.contains("File exists") || reason.contains("os error 17")
            })
    );
    let entry = directory.join(format!("{}.qcp", collision.query_cache.key.unwrap()));
    assert!(!entry.exists());

    let repaired = compile_path(&source, &options);
    success(&repaired);
    assert_eq!(repaired.query_cache.state, "miss");
    assert!(repaired.query_cache.stored);
    assert_eq!(repaired.query_cache.key, collision.query_cache.key);
    assert!(entry.is_file());

    let warm = compile_path(&source, &options);
    success(&warm);
    assert_eq!(warm.query_cache.state, "hit");
    assert!(!warm.query_cache.stored);
    assert_eq!(warm.query_cache.key, collision.query_cache.key);
    for path in fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
    {
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(".pending-")
        {
            assert!(
                path == output || path == output.with_extension("ll"),
                "{}",
                path.display()
            );
        }
    }
}
