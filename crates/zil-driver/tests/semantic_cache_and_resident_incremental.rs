//! End-to-end verification of P7 semantic candidate caching (`--semantic-cache`),
//! per-function candidate `FunctionCertificate` reuse across edits, tamper/corruption
//! self-healing under mandatory fresh `zil-check` (`verification_runs == 1`), and
//! large-project resident session incremental reuse.

use std::path::PathBuf;
use zil_base::{Phase, ResultCategory};
use zil_driver::{
    CodegenOptions, CompilationSession, CompileOptions, NativeBackend, Optimization,
    QueryDisposition, Stage, compile_path,
};

fn make_temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zil_p7_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_multi_module_project(dir: &std::path::Path, n_mods: usize, salt0: i64) {
    let mut imports = String::new();
    let mut calls = String::new();
    for i in 0..n_mods {
        let s = if i == 0 { salt0 } else { 0 };
        std::fs::write(
            dir.join(format!("m{i}.zil")),
            format!(
                "fn priv_work_{i}(x: i64) -> i64 {{\n\
                 \tvar acc: i64 = x + {};\n\
                 \tvar k: i64 = 0;\n\
                 \twhile k < 4 {{\n\
                 \t\tacc = acc + k + {};\n\
                 \t\tk = k + 1;\n\
                 \t}}\n\
                 \treturn acc;\n\
                 }}\n\
                 pub fn compute(x: i64) -> i64 {{\n\
                 \treturn priv_work_{i}(x);\n\
                 }}\n",
                (i as i64) + 1 + s,
                (i % 7) + 1
            ),
        )
        .expect("write module");
        imports.push_str(&format!("import m{i};\n"));
        calls.push_str(&format!("    acc = m{i}.compute(acc % 97);\n"));
    }
    std::fs::write(
        dir.join("main.zil"),
        format!(
            "{imports}\nfn main() -> i64 {{\n    var acc: i64 = 1;\n{calls}    return acc % 100;\n}}\n"
        ),
    )
    .expect("write main");
}

#[test]
fn semantic_cache_reuses_candidates_runs_fresh_verify_and_heals_tampering() {
    let dir = make_temp_dir("sem_cache_heal");
    let sem_dir = dir.join("sem_cache");
    write_multi_module_project(&dir, 24, 0);

    let opts = CompileOptions {
        stop_after: Stage::Verify,
        semantic_cache: Some(sem_dir.clone()),
        ..CompileOptions::default()
    };

    // 1. Cold check: executes full pipeline, populates `.zsem` and `certs/`.
    let cold = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        cold.status,
        ResultCategory::Success,
        "{:?}",
        cold.diagnostics
    );
    assert_eq!(cold.verification_runs, 1);
    assert!(
        !cold
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Reused),
        "cold run must not report reused Types"
    );

    // 2. Warm check in a brand-new process/session: reuses disk `.zsem` candidate
    //    (skipping typeck/solve) while ALWAYS running fresh `zil-check` (`verification_runs == 1`).
    let warm = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        warm.status,
        ResultCategory::Success,
        "{:?}",
        warm.diagnostics
    );
    assert_eq!(warm.verification_runs, 1);
    assert!(warm.cache_hit);
    assert!(
        warm.query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Reused),
        "warm run must reuse semantic candidate Types"
    );
    assert!(
        warm.query_events
            .iter()
            .any(|e| e.phase == Phase::Analyze && e.disposition == QueryDisposition::Reused),
        "warm run must reuse semantic candidate Analyze"
    );
    assert!(
        warm.query_events
            .iter()
            .any(|e| e.phase == Phase::Verify && e.disposition == QueryDisposition::Executed),
        "warm run must execute fresh Verify"
    );

    // 3. Corrupt `.zsem` file on disk: must detect SHA-256 corruption, purge, rebuild fresh, and succeed!
    let mut zsem_files = Vec::new();
    for entry in std::fs::read_dir(&sem_dir).expect("read sem_dir").flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("zsem") {
            zsem_files.push(p);
        }
    }
    assert!(!zsem_files.is_empty(), "expected persisted .zsem snapshot");
    for p in &zsem_files {
        std::fs::write(p, b"tampered semantic cache payload").expect("corrupt zsem");
    }
    let healed = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        healed.status,
        ResultCategory::Success,
        "{:?}",
        healed.diagnostics
    );
    assert_eq!(healed.verification_runs, 1);

    // 4. Edit 1 module (`m0.zil`): full-program `.zsem` misses, per-function cert cache
    //    serves unchanged functions, and fresh `zil-check` verifies the entire module.
    write_multi_module_project(&dir, 24, 9);
    let edited = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        edited.status,
        ResultCategory::Success,
        "{:?}",
        edited.diagnostics
    );
    assert_eq!(edited.verification_runs, 1);

    // 5. Introduce a genuine safety violation (escaping local reference): MUST be rejected!
    std::fs::write(
        dir.join("m0.zil"),
        "fn bad<life a>() -> &'a i64 {\n\
         \tlet local: i64 = 42;\n\
         \treturn &local;\n\
         }\n\
         pub fn compute(x: i64) -> i64 {\n\
         \tlet r = bad();\n\
         \treturn x + *r;\n\
         }\n",
    )
    .expect("write unsafe m0");
    let rejected = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        rejected.status,
        ResultCategory::SafeReject,
        "unsafe edit after semantic cache must be strictly rejected"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn large_resident_session_reuses_all_asts_and_semantic_candidates_across_edits() {
    let dir = make_temp_dir("resident_large");
    write_multi_module_project(&dir, 40, 0);

    let mut session = CompilationSession::with_capacity(256);
    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..CompileOptions::default()
    };

    // 1. Cold resident compile parses all 41 files (entry + 40 modules) and populates RAM caches.
    let r1 = session.compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(r1.status, ResultCategory::Success, "{:?}", r1.diagnostics);
    assert_eq!(r1.verification_runs, 1);
    assert_eq!(
        session.cached_query_count(),
        41,
        "all 41 module ASTs must remain cached in large session"
    );

    // 2. Warm no-op compile hits ALL 41 parse ASTs AND in-memory semantic snapshot while executing fresh Verify.
    let r2 = session.compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(r2.status, ResultCategory::Success);
    assert_eq!(r2.verification_runs, 1);
    let reused_parses = r2
        .query_events
        .iter()
        .filter(|e| e.phase == Phase::Parse && e.disposition == QueryDisposition::Reused)
        .count();
    assert_eq!(reused_parses, 41, "all 41 module ASTs must be reused");
    assert!(
        r2.query_events
            .iter()
            .any(|e| e.phase == Phase::Types && e.disposition == QueryDisposition::Reused)
    );

    // 3. Edit 1 module (`m0.zil`): 40/41 parse ASTs reused, m0 re-parsed, fresh Verify passes.
    write_multi_module_project(&dir, 40, 17);
    let r3 = session.compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(r3.status, ResultCategory::Success, "{:?}", r3.diagnostics);
    assert_eq!(r3.verification_runs, 1);
    let reused_parses_3 = r3
        .query_events
        .iter()
        .filter(|e| e.phase == Phase::Parse && e.disposition == QueryDisposition::Reused)
        .count();
    assert_eq!(
        reused_parses_3, 40,
        "40 unchanged module ASTs must be reused on 1-module edit"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "llvm-api")]
#[test]
fn semantic_cache_and_object_cache_end_to_end_native_execution() {
    use std::process::Command;

    let dir = make_temp_dir("sem_and_obj_e2e");
    let sem_dir = dir.join("sem_cache");
    let obj_dir = dir.join("obj_cache");
    let exe_path = dir.join(format!("app{}", std::env::consts::EXE_SUFFIX));
    write_multi_module_project(&dir, 24, 0);

    let opts = CompileOptions {
        stop_after: Stage::Build,
        optimization: Optimization::O0,
        backend: NativeBackend::LlvmApi,
        output: Some(exe_path.clone()),
        codegen: CodegenOptions { units: 8, jobs: 4 },
        semantic_cache: Some(sem_dir),
        object_cache: Some(obj_dir),
        ..CompileOptions::default()
    };

    let cold = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        cold.status,
        ResultCategory::Success,
        "{:?}",
        cold.diagnostics
    );
    assert_eq!(cold.verification_runs, 1);
    let expected_code = Command::new(&exe_path).output().unwrap().status.code();
    assert!(expected_code.is_some());

    // Warm build hits semantic cache + 8/8 object cache + skips redundant link, while running fresh Verify!
    let warm = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        warm.status,
        ResultCategory::Success,
        "{:?}",
        warm.diagnostics
    );
    assert_eq!(warm.verification_runs, 1);
    let wcg = warm.native_codegen.as_ref().unwrap();
    assert_eq!(wcg.cache_hits, 8);
    assert!(wcg.link_skipped);
    assert_eq!(
        Command::new(&exe_path).output().unwrap().status.code(),
        expected_code
    );

    let _ = std::fs::remove_dir_all(&dir);
}
