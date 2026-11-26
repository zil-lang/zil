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

#[cfg(feature = "llvm-api")]
#[test]
fn object_cache_rejects_corrupted_object_and_tampered_metadata_and_heals() {
    use std::process::Command;
    use zil_base::ResultCategory;
    use zil_driver::{
        CodegenOptions, CompileOptions, NativeBackend, Optimization, Stage, compile_path,
    };

    let dir = std::env::temp_dir().join(format!("zil_cache_integrity_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let cache_dir = dir.join("objcache");
    let src_path = dir.join("main.zil");
    let exe_path = dir.join("app");

    std::fs::write(
        &src_path,
        r#"
fn helper_a(x: i32) -> i32 { return x + 11; }
fn helper_b(x: i32) -> i32 { return x + 22; }
pub fn main() -> i32 { return helper_a(1) + helper_b(2); }
"#,
    )
    .expect("write source");

    let opts = CompileOptions {
        backend: NativeBackend::LlvmApi,
        stop_after: Stage::Build,
        output: Some(exe_path.clone()),
        optimization: Optimization::O0,
        object_cache: Some(cache_dir.clone()),
        codegen: CodegenOptions { units: 2, jobs: 2 },
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

    // 1. Cold build populates 2 cached objects + 2 `.meta` sidecars.
    let cold = compile_path(&src_path, &opts);
    assert_eq!(
        cold.status,
        ResultCategory::Success,
        "{:?}",
        cold.diagnostics
    );
    assert_eq!(count_hits(&cold), 0);
    let out = Command::new(&exe_path).output().expect("run cold exe");
    assert_eq!(out.status.code(), Some(36));

    // Confirm warm hit reports real LLVM version (not placeholder "cached").
    let warm = compile_path(&src_path, &opts);
    assert_eq!(warm.status, ResultCategory::Success);
    assert_eq!(count_hits(&warm), 2);
    for rep in &warm.native_codegen.as_ref().unwrap().object_reports {
        assert!(
            !rep.contains("\"cached\""),
            "hit receipt must carry real LLVM version: {rep}"
        );
        assert!(
            rep.contains("default<O0>"),
            "hit receipt must carry pipeline identity: {rep}"
        );
    }

    let obj_ext = if cfg!(windows) { "obj" } else { "o" };
    let mut cached_objs: Vec<std::path::PathBuf> = std::fs::read_dir(&cache_dir)
        .expect("read cache dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some(obj_ext))
        .collect();
    cached_objs.sort();
    assert_eq!(cached_objs.len(), 2, "expected 2 cached objects");
    let target_obj = cached_objs[0].clone();
    let target_meta = std::path::PathBuf::from(format!("{}.meta", target_obj.display()));
    assert!(target_meta.exists(), "paired .meta sidecar must exist");

    // 2. Corrupt cached object bytes (same length, flipped bytes) -> must detect, recompile & heal.
    let orig_bytes = std::fs::read(&target_obj).expect("read cached obj");
    let mut corrupt_bytes = orig_bytes.clone();
    for b in corrupt_bytes.iter_mut().take(32) {
        *b ^= 0xFF;
    }
    std::fs::write(&target_obj, &corrupt_bytes).expect("write corrupt obj");
    let healed1 = compile_path(&src_path, &opts);
    assert_eq!(
        healed1.status,
        ResultCategory::Success,
        "corrupt cache entry must trigger auto-recompile: {:?}",
        healed1.diagnostics
    );
    assert_eq!(healed1.verification_runs, 1);
    assert_eq!(count_hits(&healed1), 1, "only untouched unit may hit cache");
    assert_eq!(
        Command::new(&exe_path)
            .output()
            .expect("run healed1")
            .status
            .code(),
        Some(36)
    );
    assert_eq!(
        count_hits(&compile_path(&src_path, &opts)),
        2,
        "cache must be healed"
    );

    // 3. Tamper LLVM toolchain version in `.meta` -> must reject hit, recompile & heal.
    let meta_txt = std::fs::read_to_string(&target_meta).expect("read meta");
    let tampered_meta = meta_txt
        .lines()
        .map(|l| {
            if l.starts_with("llvm=") {
                "llvm=99.0.0"
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(&target_meta, tampered_meta).expect("write tampered meta");
    let healed2 = compile_path(&src_path, &opts);
    assert_eq!(healed2.status, ResultCategory::Success);
    assert_eq!(
        count_hits(&healed2),
        1,
        "mismatched LLVM metadata must invalidate entry"
    );
    assert_eq!(count_hits(&compile_path(&src_path, &opts)), 2);

    // 4. Delete `.meta` sidecar leaving bare `.o` -> must reject hit, recompile & recreate `.meta`.
    std::fs::remove_file(&target_meta).expect("remove meta");
    let healed3 = compile_path(&src_path, &opts);
    assert_eq!(healed3.status, ResultCategory::Success);
    assert_eq!(
        count_hits(&healed3),
        1,
        "missing .meta must reject bare object file"
    );
    assert!(
        target_meta.exists(),
        "recompilation must recreate .meta sidecar"
    );
    assert_eq!(count_hits(&compile_path(&src_path, &opts)), 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "llvm-api")]
#[test]
fn module_add_delete_rename_and_move_fn_preserve_unrelated_cgu_cache() {
    use std::process::Command;
    use zil_base::ResultCategory;
    use zil_driver::{
        CodegenOptions, CompileOptions, NativeBackend, Optimization, Stage, compile_path,
    };

    let dir = std::env::temp_dir().join(format!("zil_cgu_mod_ops_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let cache_dir = dir.join("objcache");
    let exe_path = dir.join("app");

    let write_mod = |name: &str, val: i32, extra_fn: &str| {
        let src = format!("{extra_fn}pub fn calc(x: i32) -> i32 {{ return x + {val}; }}\n");
        std::fs::write(dir.join(format!("{name}.zil")), src).expect("write module");
    };
    let write_main = |mods: &[&str], extra_call: &str| {
        let mut s = String::new();
        for m in mods {
            s.push_str(&format!("import {m};\n"));
        }
        s.push_str("pub fn main() -> i32 {\n  var sum: i32 = 0;\n");
        for m in mods {
            s.push_str(&format!("  sum = sum + {m}.calc(1);\n"));
        }
        s.push_str(extra_call);
        s.push_str("  return sum % 120;\n}\n");
        std::fs::write(dir.join("main.zil"), s).expect("write main");
    };

    let base_names: Vec<String> = (0..20).map(|i| format!("m{i}")).collect();
    for (i, name) in base_names.iter().enumerate() {
        let extra = if i == 1 {
            "pub fn moved_helper(v: i32) -> i32 { return v + 7; }\n"
        } else {
            ""
        };
        write_mod(name, (i as i32) + 1, extra);
    }
    let base_refs: Vec<&str> = base_names.iter().map(String::as_str).collect();
    write_main(&base_refs, "  sum = sum + m1.moved_helper(1);\n");

    let opts = CompileOptions {
        backend: NativeBackend::LlvmApi,
        stop_after: Stage::Build,
        output: Some(exe_path.clone()),
        optimization: Optimization::O0,
        object_cache: Some(cache_dir.clone()),
        codegen: CodegenOptions { units: 8, jobs: 2 },
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

    // 1. Cold build across 8 CGUs.
    let cold = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        cold.status,
        ResultCategory::Success,
        "{:?}",
        cold.diagnostics
    );
    assert_eq!(count_hits(&cold), 0);
    // sum = sum_{i=0..19}(1 + i + 1) + (1 + 7) = 230 + 8 = 238; 238 % 120 = 118
    assert_eq!(
        Command::new(&exe_path).output().unwrap().status.code(),
        Some(118)
    );

    // 2. Insert a brand-new module `m_added` -> only unit 0 (`main`) + bucket(`m_added`) miss (>= 6/8 hits!).
    write_mod("m_added", 200, "");
    let mut mods_added = base_refs.clone();
    mods_added.push("m_added");
    write_main(&mods_added, "  sum = sum + m1.moved_helper(1);\n");
    let r_add = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        r_add.status,
        ResultCategory::Success,
        "{:?}",
        r_add.diagnostics
    );
    assert_eq!(r_add.verification_runs, 1);
    assert!(
        count_hits(&r_add) >= 6,
        "inserting 1 new module must preserve >= 6/8 CGU hits, got {}/8",
        count_hits(&r_add)
    );
    assert_eq!(
        Command::new(&exe_path).output().unwrap().status.code(),
        Some(79)
    ); // (238 + 201) % 120 = 79

    // 3. Delete module `m3` -> only unit 0 (`main`) + bucket(`m3`) miss (>= 6/8 hits!).
    let mods_del: Vec<&str> = mods_added.into_iter().filter(|&m| m != "m3").collect();
    write_main(&mods_del, "  sum = sum + m1.moved_helper(1);\n");
    let r_del = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        r_del.status,
        ResultCategory::Success,
        "{:?}",
        r_del.diagnostics
    );
    assert!(
        count_hits(&r_del) >= 6,
        "deleting 1 module must preserve >= 6/8 CGU hits, got {}/8",
        count_hits(&r_del)
    );

    // 4. Rename module `m4` -> `m4_renamed` -> only unit 0 + old/new buckets miss (>= 5/8 hits!).
    write_mod("m4_renamed", 205, "");
    let mods_ren: Vec<&str> = mods_del
        .into_iter()
        .map(|m| if m == "m4" { "m4_renamed" } else { m })
        .collect();
    write_main(&mods_ren, "  sum = sum + m1.moved_helper(1);\n");
    let r_ren = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        r_ren.status,
        ResultCategory::Success,
        "{:?}",
        r_ren.diagnostics
    );
    assert!(
        count_hits(&r_ren) >= 5,
        "renaming 1 module must preserve >= 5/8 CGU hits, got {}/8",
        count_hits(&r_ren)
    );

    // 5. Move `moved_helper` from `m1` to `m2` -> only unit 0 + bucket(m1) + bucket(m2) miss (>= 5/8 hits!).
    write_mod("m1", 2, "");
    write_mod(
        "m2",
        3,
        "pub fn moved_helper(v: i32) -> i32 { return v + 7; }\n",
    );
    write_main(&mods_ren, "  sum = sum + m2.moved_helper(1);\n");
    let r_move = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        r_move.status,
        ResultCategory::Success,
        "{:?}",
        r_move.diagnostics
    );
    assert!(
        count_hits(&r_move) >= 5,
        "moving 1 fn between modules must preserve >= 5/8 CGU hits, got {}/8",
        count_hits(&r_move)
    );

    // 6. Batch-add 3 new modules (`m_b0..m_b2`) -> untouched buckets stay cached (>= 4/8 hits!).
    write_mod("m_b0", 101, "");
    write_mod("m_b1", 102, "");
    write_mod("m_b2", 103, "");
    let mut mods_batch = mods_ren;
    mods_batch.extend_from_slice(&["m_b0", "m_b1", "m_b2"]);
    write_main(&mods_batch, "  sum = sum + m2.moved_helper(1);\n");
    let r_batch = compile_path(&dir.join("main.zil"), &opts);
    assert_eq!(
        r_batch.status,
        ResultCategory::Success,
        "{:?}",
        r_batch.diagnostics
    );
    assert!(
        count_hits(&r_batch) >= 4,
        "batch-adding 3 modules must preserve >= 4/8 CGU hits, got {}/8",
        count_hits(&r_batch)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "llvm-api")]
#[test]
fn soft_budget_smaller_than_active_working_set_preserves_active_and_evicts_stale() {
    use std::process::Command;
    use zil_base::ResultCategory;
    use zil_driver::{
        CodegenOptions, CompileOptions, NativeBackend, Optimization, Stage, compile_path,
    };

    let dir = std::env::temp_dir().join(format!("zil_soft_budget_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let cache_dir = dir.join("objcache");
    let src_path = dir.join("main.zil");
    let exe_path = dir.join("app");

    let write_src = |v: i32| {
        std::fs::write(
            &src_path,
            format!(
                "fn a(x: i32) -> i32 {{ return x + {v}; }}\nfn b(x: i32) -> i32 {{ return x + 2; }}\npub fn main() -> i32 {{ return a(1) + b(1); }}\n"
            ),
        )
        .expect("write src");
    };
    let make_opts = |budget: Option<u64>| CompileOptions {
        backend: NativeBackend::LlvmApi,
        stop_after: Stage::Build,
        output: Some(exe_path.clone()),
        optimization: Optimization::O0,
        object_cache: Some(cache_dir.clone()),
        object_cache_max_bytes: budget,
        codegen: CodegenOptions { units: 2, jobs: 2 },
        ..CompileOptions::default()
    };

    // Populate stale entries across 4 versions (uncapped).
    for v in 1..=4 {
        write_src(v);
        assert_eq!(
            compile_path(&src_path, &make_opts(None)).status,
            ResultCategory::Success
        );
    }

    // Build with a 1-byte soft budget (strictly smaller than active 2-unit working set).
    let tiny = compile_path(&src_path, &make_opts(Some(1)));
    assert_eq!(
        tiny.status,
        ResultCategory::Success,
        "{:?}",
        tiny.diagnostics
    );
    assert_eq!(
        Command::new(&exe_path).output().unwrap().status.code(),
        Some(8)
    );

    // All stale `.o`/`.meta` must be evicted; only the 2 active objects + 2 `.meta` remain,
    // and zero `.stage-*` directories may remain.
    let obj_ext = if cfg!(windows) { "obj" } else { "o" };
    let mut obj_count = 0usize;
    let mut meta_count = 0usize;
    let mut stage_dirs = 0usize;
    for entry in std::fs::read_dir(&cache_dir).expect("read cache dir") {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".stage-") {
            stage_dirs += 1;
        } else if name.ends_with(&format!(".{obj_ext}.meta")) {
            meta_count += 1;
        } else if name.ends_with(&format!(".{obj_ext}")) {
            obj_count += 1;
        }
    }
    assert_eq!(
        stage_dirs, 0,
        "staging directory must be removed after link"
    );
    assert_eq!(
        obj_count, 2,
        "only the 2 active objects may remain under 1-byte soft budget"
    );
    assert_eq!(
        meta_count, 2,
        "paired .meta count must match active objects"
    );

    // Immediate warm rebuild still hits 2/2 active objects!
    let warm = compile_path(&src_path, &make_opts(Some(1)));
    assert_eq!(warm.status, ResultCategory::Success);
    let hits = warm
        .native_codegen
        .as_ref()
        .unwrap()
        .object_reports
        .iter()
        .filter(|r| r.contains("\"object-cache-hit\""))
        .count();
    assert_eq!(
        hits, 2,
        "active objects must remain cached when active set > soft budget"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "llvm-api")]
#[test]
fn multi_process_shared_cache_concurrent_builds_and_eviction_stress() {
    use std::process::Command;
    use zil_base::ResultCategory;
    use zil_driver::{
        CodegenOptions, CompileOptions, NativeBackend, Optimization, Stage, compile_path,
    };

    // Child worker mode: run 5 rounds of mutating a 6-module project sharing the parent's
    // cache directory under a 1-byte soft budget (forcing concurrent eviction on every build).
    if let Ok(spec) = std::env::var("ZIL_SHARED_CACHE_WORKER_SPEC") {
        let parts: Vec<&str> = spec.split('|').collect();
        assert_eq!(parts.len(), 3, "invalid worker spec: {spec}");
        let worker_id: i32 = parts[0].parse().expect("worker id");
        let work_dir = std::path::PathBuf::from(parts[1]);
        let cache_dir = std::path::PathBuf::from(parts[2]);
        std::fs::create_dir_all(&work_dir).expect("create worker dir");
        let exe_path = work_dir.join(if cfg!(windows) { "app.exe" } else { "app" });

        // Shared base modules (identical across workers -> high cache contention & hits)
        for m in 0..4 {
            std::fs::write(
                work_dir.join(format!("m{m}.zil")),
                format!("pub fn f(x: i32) -> i32 {{ return x + {}; }}\n", m + 1),
            )
            .expect("write shared mod");
        }
        let opts = CompileOptions {
            backend: NativeBackend::LlvmApi,
            stop_after: Stage::Build,
            output: Some(exe_path.clone()),
            optimization: Optimization::O0,
            object_cache: Some(cache_dir),
            // 1-byte soft budget forces every worker to run LRU eviction after every single build!
            object_cache_max_bytes: Some(1),
            codegen: CodegenOptions { units: 4, jobs: 2 },
            ..CompileOptions::default()
        };
        for round in 0..5 {
            // Mutate worker-private module each round to trigger misses + concurrent evictions.
            let salt = worker_id * 10 + round;
            std::fs::write(
                work_dir.join("m_priv.zil"),
                format!("pub fn g(x: i32) -> i32 {{ return x + {salt}; }}\n"),
            )
            .expect("write priv mod");
            std::fs::write(
                work_dir.join("main.zil"),
                "import m0;\nimport m1;\nimport m2;\nimport m3;\nimport m_priv;\npub fn main() -> i32 {\n  var s: i32 = m0.f(1) + m1.f(1) + m2.f(1) + m3.f(1) + m_priv.g(1);\n  return s % 120;\n}\n",
            )
            .expect("write main");
            let rep = compile_path(&work_dir.join("main.zil"), &opts);
            assert_eq!(
                rep.status,
                ResultCategory::Success,
                "worker {worker_id} round {round} failed: {:?}",
                rep.diagnostics
            );
            assert_eq!(rep.verification_runs, 1);
            let expected = (14 + 1 + salt) % 120;
            let out = Command::new(&exe_path).output().expect("run worker exe");
            assert_eq!(
                out.status.code(),
                Some(expected),
                "worker {worker_id} round {round} wrong exit code"
            );
        }
        return;
    }

    // Parent coordinator: spawn 4 independent OS child processes sharing one cache directory.
    let dir = std::env::temp_dir().join(format!("zil_mp_cache_stress_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache_dir = dir.join("shared_cache");
    std::fs::create_dir_all(&cache_dir).expect("create shared cache dir");
    let exe = std::env::current_exe().expect("current test binary");

    let mut children = Vec::new();
    for worker_id in 0..4 {
        let work_dir = dir.join(format!("worker_{worker_id}"));
        let spec = format!("{worker_id}|{}|{}", work_dir.display(), cache_dir.display());
        let child = Command::new(&exe)
            .arg("--exact")
            .arg("multi_process_shared_cache_concurrent_builds_and_eviction_stress")
            .arg("--nocapture")
            .env("ZIL_SHARED_CACHE_WORKER_SPEC", spec)
            .spawn()
            .expect("spawn concurrent worker process");
        children.push((worker_id, child));
    }

    for (worker_id, mut child) in children {
        let status = child.wait().expect("wait worker");
        assert!(
            status.success(),
            "concurrent worker process {worker_id} failed with {status}"
        );
    }

    // Ensure no `.stage-*` directory leaked after all 4 processes exited.
    let leaked_stages: Vec<_> = std::fs::read_dir(&cache_dir)
        .expect("read shared cache")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".stage-"))
        .collect();
    assert!(
        leaked_stages.is_empty(),
        "staging dirs leaked after multi-process builds: {:?}",
        leaked_stages
    );

    let _ = std::fs::remove_dir_all(&dir);
}
