//! Verifies multi-unit reachability DCE and SHA-256 per-unit `--object-cache`
//! reuse without ever bypassing fresh `zil-check` verification.

use zil_backend_llvm::emit_verified_body_units_for_target;
use zil_driver::verify_body_program;

#[test]
fn multi_unit_executable_dce_prunes_unreachable_functions_and_balances_units() {
    let src = r#"
fn unused_dead_helper(x: i32) -> i32 {
    return x + 999
}

fn helper_a(x: i32) -> i32 {
    return x + 10
}

fn helper_b(x: i32) -> i32 {
    return x + 30
}

pub fn main() -> i32 {
    return helper_a(1) + helper_b(2)
}
"#;
    let verified = verify_body_program(&[("multi_dce.zil", src)]).expect("program must verify");
    let units = emit_verified_body_units_for_target(&verified, true, 2, "x86_64-unknown-linux-gnu")
        .expect("multi-unit IR emission must succeed");
    assert_eq!(units.len(), 2);
    for (idx, unit) in units.iter().enumerate() {
        assert!(
            !unit.contains("unused_dead_helper"),
            "unit {idx} must prune unreachable function `unused_dead_helper`"
        );
    }
    let defs_u0 = units[0].matches("define ").count();
    let defs_u1 = units[1].matches("define ").count();
    assert!(
        defs_u0 >= 2 && defs_u1 >= 1,
        "expected balanced definitions across units: u0={defs_u0}, u1={defs_u1}"
    );
}

#[cfg(feature = "llvm-api")]
#[test]
fn object_cache_hits_unchanged_units_and_always_runs_fresh_verify() {
    use std::process::Command;
    use zil_base::ResultCategory;
    use zil_driver::{
        CodegenOptions, CompileOptions, DiskCacheStatus, NativeBackend, Optimization, Stage,
        compile_path,
    };

    let dir = std::env::temp_dir().join(format!("zil_p3_obj_cache_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let cache_dir = dir.join("objcache");
    let src_path = dir.join("main.zil");
    let exe_path = dir.join("app");

    let make_src = |val: i32| {
        format!(
            r#"
fn unused_dead_helper(x: i32) -> i32 {{
    return x + 999
}}

fn helper_a(x: i32) -> i32 {{
    return x + 10
}}

fn helper_b(x: i32) -> i32 {{
    return x + {val}
}}

pub fn main() -> i32 {{
    return helper_a(1) + helper_b(2)
}}
"#
        )
    };

    std::fs::write(&src_path, make_src(30)).expect("write v1 source");

    let make_opts = || CompileOptions {
        backend: NativeBackend::LlvmApi,
        stop_after: Stage::Build,
        output: Some(exe_path.clone()),
        optimization: Optimization::O2,
        object_cache: Some(cache_dir.clone()),
        codegen: CodegenOptions { units: 2, jobs: 2 },
        ..CompileOptions::default()
    };

    // 1. Cold build: populates object cache and runs fresh verification.
    let cold = compile_path(&src_path, &make_opts());
    assert_eq!(
        cold.status,
        ResultCategory::Success,
        "cold build failed: {:?}",
        cold.diagnostics
    );
    assert_eq!(cold.verification_runs, 1);
    assert_eq!(cold.disk_cache, DiskCacheStatus::Rebuilt);
    let ir_text = std::fs::read_to_string(cold.artifacts.get("llvm").expect("llvm artifact"))
        .expect("read emitted IR");
    assert!(
        !ir_text.contains("unused_dead_helper"),
        "multi-unit executable DCE must prune unreachable function `unused_dead_helper`"
    );
    let out1 = Command::new(&exe_path).output().expect("run v1 exe");
    assert_eq!(out1.status.code(), Some(43));

    // 2. Warm rebuild: 100% object cache hit while still running fresh verification.
    let warm = compile_path(&src_path, &make_opts());
    assert_eq!(
        warm.status,
        ResultCategory::Success,
        "warm build failed: {:?}",
        warm.diagnostics
    );
    assert_eq!(
        warm.verification_runs, 1,
        "object cache must never skip zil-check"
    );
    assert_eq!(warm.disk_cache, DiskCacheStatus::Hit);
    let nc_warm = warm
        .native_codegen
        .as_ref()
        .expect("native_codegen summary");
    assert_eq!(nc_warm.actual_units, 2);
    assert!(
        nc_warm
            .object_reports
            .iter()
            .all(|r| r.contains("\"object-cache-hit\"")),
        "expected 2/2 cache hits on warm rebuild, got {:?}",
        nc_warm.object_reports
    );

    // 3. Private edit to helper_b: unchanged unit hits cache, edited unit rebuilds.
    std::fs::write(&src_path, make_src(40)).expect("write v2 source");
    let edited = compile_path(&src_path, &make_opts());
    assert_eq!(
        edited.status,
        ResultCategory::Success,
        "edited build failed: {:?}",
        edited.diagnostics
    );
    assert_eq!(edited.verification_runs, 1);
    assert_eq!(edited.disk_cache, DiskCacheStatus::Rebuilt);
    let nc_edit = edited
        .native_codegen
        .as_ref()
        .expect("native_codegen summary");
    let hits = nc_edit
        .object_reports
        .iter()
        .filter(|r| r.contains("\"object-cache-hit\""))
        .count();
    assert_eq!(
        hits, 1,
        "expected exactly 1 cached unit and 1 rebuilt unit after editing helper_b, got {:?}",
        nc_edit.object_reports
    );
    let out2 = Command::new(&exe_path).output().expect("run v2 exe");
    assert_eq!(out2.status.code(), Some(53));

    let _ = std::fs::remove_dir_all(&dir);
}
