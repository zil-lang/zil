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

    // 1. Cold build: populates object cache, skips redundant .ll/.u*.o dumps, and runs fresh verification.
    let cold = compile_path(&src_path, &make_opts());
    assert_eq!(
        cold.status,
        ResultCategory::Success,
        "cold build failed: {:?}",
        cold.diagnostics
    );
    assert_eq!(cold.verification_runs, 1);
    assert_eq!(cold.disk_cache, DiskCacheStatus::Rebuilt);
    assert!(
        !cold.artifacts.contains_key("llvm"),
        "cached build without --emit-dir must not write monolithic .ll artifact"
    );
    assert!(
        !std::path::PathBuf::from(format!("{}.ll", exe_path.display())).exists(),
        "cached build without --emit-dir must not leave <output>.ll on disk"
    );
    assert!(
        !std::path::PathBuf::from(format!("{}.u0.o", exe_path.display())).exists(),
        "cached build must link directly from object cache without duplicating <output>.u0.o"
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

#[cfg(feature = "llvm-api")]
#[test]
fn multi_module_cgu_stability_and_bounded_lru_eviction() {
    use std::process::Command;
    use zil_base::ResultCategory;
    use zil_driver::{
        CodegenOptions, CompileOptions, NativeBackend, Optimization, Stage, compile_path,
    };

    let dir = std::env::temp_dir().join(format!("zil_p4_cgu_lru_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let cache_dir = dir.join("objcache");
    let exe_path = dir.join("app");

    let write_mod = |idx: usize, salt: i32, extra_helper: bool, extra_param: bool| {
        let extra = if extra_helper {
            format!("fn priv_helper_{idx}(v: i32) -> i32 {{ return v + {salt}; }}\n")
        } else {
            String::new()
        };
        let extra_call = if extra_helper {
            format!(" + priv_helper_{idx}(x)")
        } else {
            String::new()
        };
        let (sig, bias) = if extra_param {
            ("pub fn calc(x: i32, b: i32) -> i32", " + b")
        } else {
            ("pub fn calc(x: i32) -> i32", "")
        };
        let src = format!(
            "{extra}{sig} {{\n  return x + {base}{extra_call}{bias};\n}}\n",
            base = (idx as i32) + salt
        );
        std::fs::write(dir.join(format!("m{idx}.zil")), src).expect("write module");
    };

    let write_main = |m5_extra: bool| {
        let mut s = String::new();
        for idx in 0..8 {
            s.push_str(&format!("import m{idx};\n"));
        }
        s.push_str("pub fn main() -> i32 {\n  var sum: i32 = 0;\n");
        for idx in 0..8 {
            if m5_extra && idx == 5 {
                s.push_str("  sum = sum + m5.calc(1, 2);\n");
            } else {
                s.push_str(&format!("  sum = sum + m{idx}.calc(1);\n"));
            }
        }
        s.push_str("  return sum;\n}\n");
        std::fs::write(dir.join("main.zil"), s).expect("write main");
    };

    for idx in 0..8 {
        write_mod(idx, 1, false, false);
    }
    write_main(false);

    let make_opts = |max_bytes: Option<u64>| CompileOptions {
        backend: NativeBackend::LlvmApi,
        stop_after: Stage::Build,
        output: Some(exe_path.clone()),
        optimization: Optimization::O0,
        object_cache: Some(cache_dir.clone()),
        object_cache_max_bytes: max_bytes,
        codegen: CodegenOptions { units: 4, jobs: 2 },
        ..CompileOptions::default()
    };

    let count_hits = |rep: &zil_driver::CompileReport| -> usize {
        rep.native_codegen
            .as_ref()
            .expect("native_codegen")
            .object_reports
            .iter()
            .filter(|r| r.contains("\"object-cache-hit\""))
            .count()
    };

    // 1. Cold build across 4 CGUs.
    let cold = compile_path(&dir.join("main.zil"), &make_opts(None));
    assert_eq!(
        cold.status,
        ResultCategory::Success,
        "{:?}",
        cold.diagnostics
    );
    assert_eq!(cold.verification_runs, 1);
    assert_eq!(count_hits(&cold), 0);
    let out = Command::new(&exe_path).output().expect("run v1");
    assert_eq!(out.status.code(), Some(44)); // sum_{i=0..7}(1 + i + 1) = 44

    // 2. Adding a brand-new private helper function in m0 must NOT shift symbol IDs
    //    or invalidate untouched CGUs (at least 3/4 CGUs must hit cache!).
    write_mod(0, 2, true, false);
    let add_fn = compile_path(&dir.join("main.zil"), &make_opts(None));
    assert_eq!(
        add_fn.status,
        ResultCategory::Success,
        "{:?}",
        add_fn.diagnostics
    );
    assert_eq!(add_fn.verification_runs, 1);
    let add_fn_hits = count_hits(&add_fn);
    assert!(
        add_fn_hits >= 3,
        "adding a private fn in m0 must preserve >= 3/4 CGU cache hits, got {add_fn_hits}/4: {:?}",
        add_fn.native_codegen
    );
    let out = Command::new(&exe_path).output().expect("run v2");
    assert_eq!(out.status.code(), Some(48)); // +1 salt + (1+2) helper = +4

    // 3. Changing public interface of m5 + updating its caller main must invalidate ONLY
    //    the CGUs owning m5 and main — untouched CGUs (without m5 or main) must stay cached!
    write_mod(5, 1, false, true);
    write_main(true);
    let iface = compile_path(&dir.join("main.zil"), &make_opts(None));
    assert_eq!(
        iface.status,
        ResultCategory::Success,
        "{:?}",
        iface.diagnostics
    );
    assert_eq!(iface.verification_runs, 1);
    let iface_hits = count_hits(&iface);
    assert!(
        iface_hits >= 2,
        "editing public signature of m5 + main must keep >= 2/4 unrelated CGUs cached, got {iface_hits}/4: {:?}",
        iface.native_codegen
    );
    let out = Command::new(&exe_path).output().expect("run v3");
    assert_eq!(out.status.code(), Some(50)); // +2 bias

    // 4. Tight cache budget triggers bounded LRU eviction over repeated edit rounds
    //    while keeping all active objects intact and subsequent warm rebuilds 4/4 cached.
    let cache_dir_bytes = || -> (u64, usize) {
        let mut bytes = 0u64;
        let mut files = 0usize;
        for entry in std::fs::read_dir(&cache_dir).expect("read cache dir") {
            let entry = entry.expect("dir entry");
            if entry.path().extension().and_then(|e| e.to_str()) == Some("o") {
                bytes += entry.metadata().expect("meta").len();
                files += 1;
            }
        }
        (bytes, files)
    };
    let (before_bytes, _) = cache_dir_bytes();
    // Pick a tight budget slightly above the active 4-unit working set so stale entries get evicted.
    let active_budget = (before_bytes / 2).max(4 * 1024);
    for round in 10..16 {
        write_mod(0, round, false, false);
        let r = compile_path(&dir.join("main.zil"), &make_opts(Some(active_budget)));
        assert_eq!(
            r.status,
            ResultCategory::Success,
            "round {round} failed: {:?}",
            r.diagnostics
        );
        assert_eq!(r.verification_runs, 1);
    }
    let (after_bytes, after_files) = cache_dir_bytes();
    assert!(
        after_files <= 6,
        "LRU eviction must keep cache file count bounded near active working set (4), got {after_files} files ({after_bytes} bytes, budget={active_budget})"
    );
    // Immediate warm rebuild after eviction must still hit 4/4 active CGUs!
    let post_warm = compile_path(&dir.join("main.zil"), &make_opts(Some(active_budget)));
    assert_eq!(post_warm.status, ResultCategory::Success);
    assert_eq!(post_warm.verification_runs, 1);
    assert_eq!(
        count_hits(&post_warm),
        4,
        "eviction must never delete active CGU objects"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
