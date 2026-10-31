//! A native correctness control, not a timing benchmark.
use std::{fs, path::Path, process::Command};
use zil_base::ResultCategory;
use zil_driver::{CompilationSession, CompileOptions, Optimization, Stage};

fn execute(path: &Path, expected: i32, output: &Path) {
    let result = Command::new(path).output().unwrap();
    fs::write(output.join("stdout"), &result.stdout).unwrap();
    fs::write(output.join("stderr"), &result.stderr).unwrap();
    fs::write(output.join("exit.json"), format!("{{\"actual\":{:?},\"expected\":{expected}}}\n", result.status.code().unwrap())).unwrap();
    assert_eq!(result.status.code(), Some(expected));
    assert_eq!(result.stdout, vec![(64 + expected) as u8]);
    assert!(result.stderr.is_empty());
}

fn main() {
    let arg = std::env::args().nth(1).expect("dedicated new control directory");
    let dir = Path::new(&arg);
    fs::create_dir(dir).unwrap();
    let sources = dir.join("sources");
    fs::create_dir(&sources).unwrap();
    fs::write(sources.join("zil.toml"), "[package]\nname=\"cp29-native-control\"\n").unwrap();
    fs::write(sources.join("main.zil"), "import a\nimport b\nimport io\nfn main()->i32{let n=a.get()+b.get();io.put(64+n);return n}\n").unwrap();
    fs::write(sources.join("b.zil"), "pub fn get()->i32=>2\n").unwrap();
    fs::write(sources.join("io.zil"), "extern \"C\" fn putchar(n:i32)->i32;\npub fn put(n:i32){unsafe{putchar(n)}}\n").unwrap();
    let root = sources.join("main.zil");
    let leaf = sources.join("a.zil");
    // This exercises the separate memo through automatic raw-query eviction.
    let mut session = CompilationSession::with_capacity(8);
    for (step, value) in [("cold", 1), ("private", 5), ("restore", 1)] {
        fs::write(&leaf, format!("pub fn get()->i32=>value()\nfn value()->i32=>{value}\n")).unwrap();
        let request = dir.join(step);
        fs::create_dir(&request).unwrap();
        let warm_dir = request.join("warm");
        let cold_dir = request.join("fresh");
        fs::create_dir(&warm_dir).unwrap();
        fs::create_dir(&cold_dir).unwrap();
        let options = CompileOptions {
            stop_after: Stage::Build,
            optimization: Optimization::O0,
            output: Some(warm_dir.join("program")),
            emit_dir: Some(warm_dir.join("phases")),
            ..CompileOptions::default()
        };
        let warm = session.compile_path(&root, &options);
        fs::write(warm_dir.join("compile.json"), warm.to_json()).unwrap();
        assert_eq!(warm.status, ResultCategory::Success, "{}", warm.to_json());
        assert_eq!(warm.verification_runs, 1);
        assert_eq!(session.active_query_count(), 0);
        let memo = session.type_memo_stats();
        assert!(memo.eligible);
        assert_eq!(memo.retained_modules, 3);
        assert_eq!(memo.reused_functions, if step == "cold" { 0 } else { 2 });
        fs::write(warm_dir.join("memo.json"), format!("{{\"eligible\":{},\"reused_functions\":{},\"checked_functions\":{},\"retained_modules\":{},\"retained_weight\":{}}}\n", memo.eligible, memo.reused_functions, memo.checked_functions, memo.retained_modules, memo.retained_weight)).unwrap();
        execute(&warm.artifacts["executable"], value + 2, &warm_dir);
        let fresh = zil_driver::compile_path(&root, &CompileOptions {
            output: Some(cold_dir.join("program")),
            emit_dir: Some(cold_dir.join("phases")),
            ..options.clone()
        });
        fs::write(cold_dir.join("compile.json"), fresh.to_json()).unwrap();
        assert_eq!(fresh.status, ResultCategory::Success, "{}", fresh.to_json());
        assert_eq!(fresh.verification_runs, 1);
        assert_eq!(warm.diagnostics, fresh.diagnostics);
        assert_eq!(warm.scalar_interfaces, fresh.scalar_interfaces);
        assert_eq!(warm.input_fingerprint, fresh.input_fingerprint);
        assert_eq!(warm.source_files, fresh.source_files);
        execute(&fresh.artifacts["executable"], value + 2, &cold_dir);
        // Compare the emitted raw/later representations too, ignoring no fields.
        for phase in ["typed", "candidate", "lir", "llvm", "verified-interfaces"] {
            assert_eq!(fs::read(&warm.artifacts[phase]).unwrap(), fs::read(&fresh.artifacts[phase]).unwrap(), "representation {phase} at {step}");
        }
        if step == "private" {
            let retained = session.retained_type_memo_stats();
            fs::write(&leaf, "pub fn get()->bool=>true\n").unwrap();
            let reject_dir = dir.join("reject");
            fs::create_dir(&reject_dir).unwrap();
            let reject_options = CompileOptions {
                stop_after: Stage::Build,
                output: Some(reject_dir.join("must-not-publish")),
                ..CompileOptions::default()
            };
            let warm = session.compile_path(&root, &reject_options);
            let fresh = zil_driver::compile_path(&root, &reject_options);
            fs::write(reject_dir.join("warm.json"), warm.to_json()).unwrap();
            fs::write(reject_dir.join("fresh.json"), fresh.to_json()).unwrap();
            assert_eq!(warm.status, ResultCategory::SafeReject);
            assert_eq!(warm.diagnostics, fresh.diagnostics);
            assert_eq!(session.retained_type_memo_stats(), retained);
            assert_eq!(session.active_query_count(), 0);
            assert!(!reject_dir.join("must-not-publish").exists());
        }
    }
    fs::write(dir.join("RESULT.json"), "{\"pass\":true,\"optimization\":\"O0\",\"persistent_successes\":3,\"fresh_successes\":3,\"native_executions\":6,\"rejected_warm_fresh_pair\":1,\"timing_benchmark\":false}\n").unwrap();
    println!("CP29 native control: 6 native executions, warm/fresh IR and output equality, rejected edit rollback, pass");
}
