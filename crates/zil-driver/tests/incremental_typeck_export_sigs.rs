//! End-to-end integration tests for P8 export-signature incremental `typeck`
//! and cross-edit disk/RAM baseline reuse.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zil_base::{Phase, ResultCategory};
use zil_driver::{CompilationSession, CompileOptions, Stage};

static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

fn temp_dir(label: &str) -> PathBuf {
    let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("zil-p8-inc-{label}-{}-{seq}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn write_dag_fixture(dir: &Path) -> PathBuf {
    fs::write(
        dir.join("core.zil"),
        r#"
pub struct CoreData { pub a: i64; pub b: i64 }
pub fn make_core(x: i64) -> CoreData {
    return CoreData { a: x + 1, b: x * 2 }
}
pub fn score_core(d: CoreData) -> i64 {
    return d.a + d.b
}
"#,
    )
    .expect("write core.zil");

    fs::write(
        dir.join("svc_a.zil"),
        r#"
import core
fn priv_a(x: i64) -> i64 {
    return x + 3
}
pub fn run_a(x: i64) -> i64 {
    let d = core.make_core(priv_a(x))
    return core.score_core(d)
}
"#,
    )
    .expect("write svc_a.zil");

    fs::write(
        dir.join("svc_b.zil"),
        r#"
import core
fn priv_b(x: i64) -> i64 {
    return x + 5
}
pub fn run_b(x: i64) -> i64 {
    let d = core.make_core(priv_b(x))
    return core.score_core(d)
}
"#,
    )
    .expect("write svc_b.zil");

    fs::write(
        dir.join("feat_a.zil"),
        r#"
import svc_a
pub fn eval_a(x: i64) -> i64 {
    return svc_a.run_a(x)
}
"#,
    )
    .expect("write feat_a.zil");

    fs::write(
        dir.join("feat_b.zil"),
        r#"
import svc_b
fn helper_b(x: i64) -> i64 {
    return x + 1
}
pub fn eval_b(x: i64) -> i64 {
    return svc_b.run_b(helper_b(x))
}
"#,
    )
    .expect("write feat_b.zil");

    let main_path = dir.join("main.zil");
    fs::write(
        &main_path,
        r#"
import feat_a
import feat_b
fn main() -> i32 {
    return i32(feat_a.eval_a(2) + feat_b.eval_b(2))
}
"#,
    )
    .expect("write main.zil");
    main_path
}

#[test]
fn private_edit_and_new_helper_reuse_untouched_modules_across_cli_sessions() {
    let dir = temp_dir("priv-edit");
    let main_path = write_dag_fixture(&dir);
    let sem_cache = dir.join("sem-cache");
    let exe_path = dir.join(format!("app{}", std::env::consts::EXE_SUFFIX));

    let opts = CompileOptions {
        stop_after: Stage::Build,
        output: Some(exe_path.clone()),
        semantic_cache: Some(sem_cache.clone()),
        ..Default::default()
    };

    // 1. Cold CLI session (ephemeral): builds baseline on disk.
    let rep1 = zil_driver::compile_path(&main_path, &opts);
    assert_eq!(
        rep1.status,
        ResultCategory::Success,
        "cold build: {:?}",
        rep1.diagnostics
    );
    assert_eq!(rep1.verification_runs, 1);
    assert_eq!(rep1.reused_typeck_fns, 0);
    // eval_a(2): priv_a(2)=5 -> make_core(5)=(6,10) -> 16
    // eval_b(2): helper_b(2)=3 -> priv_b(3)=8 -> make_core(8)=(9,16) -> 25
    // total = 41
    let code1 = Command::new(&exe_path).status().expect("run 1").code();
    assert_eq!(code1, Some(41));

    // 2. Edit private body + add brand-new private helper in feat_b (shifts pub fn line & FnIds!).
    fs::write(
        dir.join("feat_b.zil"),
        r#"
import svc_b
fn brand_new_helper(z: i64) -> i64 {
    return z + 2
}
fn helper_b(x: i64) -> i64 {
    // extra comment lines shifting pub fn span
    return brand_new_helper(x)
}
pub fn eval_b(x: i64) -> i64 {
    return svc_b.run_b(helper_b(x))
}
"#,
    )
    .expect("edit feat_b.zil");

    // Fresh ephemeral CLI process loads disk baseline, reuses untouched modules' typeck & certs!
    let rep2 = zil_driver::compile_path(&main_path, &opts);
    assert_eq!(
        rep2.status,
        ResultCategory::Success,
        "priv edit: {:?}",
        rep2.diagnostics
    );
    assert_eq!(rep2.verification_runs, 1);
    assert!(
        rep2.reused_typeck_fns >= 7,
        "expected >=7 untouched root fns reused (core+svc_a+svc_b+feat_a+main), got {}/{}",
        rep2.reused_typeck_fns,
        rep2.total_typeck_fns
    );
    assert!(
        rep2.reused_cert_fns >= 7,
        "expected >=7 function certificates reused across CLI process, got {}",
        rep2.reused_cert_fns
    );
    // eval_b(2): helper_b(2)=4 -> priv_b(4)=9 -> make_core(9)=(10,18) -> 28; 16 + 28 = 44
    let code2 = Command::new(&exe_path).status().expect("run 2").code();
    assert_eq!(code2, Some(44));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn public_signature_change_invalidates_only_dependents_and_catches_errors() {
    let dir = temp_dir("pub-sig");
    let main_path = write_dag_fixture(&dir);
    let sem_cache = dir.join("sem-cache");

    let opts = CompileOptions {
        stop_after: Stage::Verify,
        semantic_cache: Some(sem_cache.clone()),
        ..Default::default()
    };

    let mut session = CompilationSession::with_capacity(64);
    let rep1 = session.compile_path(&main_path, &opts);
    assert_eq!(rep1.status, ResultCategory::Success);
    assert_eq!(rep1.verification_runs, 1);

    // Change public signature of svc_a.run_a WITHOUT updating caller feat_a -> MUST fail typeck!
    fs::write(
        dir.join("svc_a.zil"),
        r#"
import core
fn priv_a(x: i64) -> i64 {
    return x + 3
}
pub fn run_a(x: i64, bonus: i64) -> i64 {
    let d = core.make_core(priv_a(x) + bonus)
    return core.score_core(d)
}
"#,
    )
    .expect("edit svc_a sig");

    let rep_err = session.compile_path(&main_path, &opts);
    assert_eq!(
        rep_err.status,
        ResultCategory::SafeReject,
        "stale caller feat_a must be rejected when svc_a signature changes"
    );
    assert_eq!(rep_err.phase, Phase::Types);

    // Now update feat_a to match the new 2-arg signature of svc_a.run_a.
    fs::write(
        dir.join("feat_a.zil"),
        r#"
import svc_a
pub fn eval_a(x: i64) -> i64 {
    return svc_a.run_a(x, 1)
}
"#,
    )
    .expect("update feat_a");

    // Re-seed baseline then edit svc_a's signature + feat_a together in one step to verify
    // unrelated branches (core, svc_b, feat_b) are STILL reused!
    let rep_ok = session.compile_path(&main_path, &opts);
    assert_eq!(rep_ok.status, ResultCategory::Success);
    assert_eq!(rep_ok.verification_runs, 1);

    fs::write(
        dir.join("svc_a.zil"),
        r#"
import core
fn priv_a(x: i64) -> i64 {
    return x + 3
}
pub fn run_a(x: i64, bonus: i64, extra: i64) -> i64 {
    let d = core.make_core(priv_a(x) + bonus + extra)
    return core.score_core(d)
}
"#,
    )
    .expect("edit svc_a 3-arg");
    fs::write(
        dir.join("feat_a.zil"),
        r#"
import svc_a
pub fn eval_a(x: i64) -> i64 {
    return svc_a.run_a(x, 1, 2)
}
"#,
    )
    .expect("update feat_a 3-arg");

    let rep_partial = session.compile_path(&main_path, &opts);
    assert_eq!(rep_partial.status, ResultCategory::Success);
    assert_eq!(rep_partial.verification_runs, 1);
    // core (2 fns) + svc_b (2 fns) + feat_b (2 fns) + main (1 fn, imports feat_a whose export sig is unchanged!) = 7 reused!
    assert!(
        rep_partial.reused_typeck_fns >= 6,
        "unrelated modules (core, svc_b, feat_b) must stay reused when svc_a pub sig changes, got {}/{}",
        rep_partial.reused_typeck_fns,
        rep_partial.total_typeck_fns
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn corrupted_disk_baseline_auto_heals_cleanly() {
    let dir = temp_dir("tamper");
    let main_path = write_dag_fixture(&dir);
    let sem_cache = dir.join("sem-cache");

    let opts = CompileOptions {
        stop_after: Stage::Verify,
        semantic_cache: Some(sem_cache.clone()),
        ..Default::default()
    };

    let rep1 = zil_driver::compile_path(&main_path, &opts);
    assert_eq!(rep1.status, ResultCategory::Success);

    // Corrupt every .zbase file on disk.
    let mut corrupted = 0usize;
    for entry in fs::read_dir(&sem_cache).expect("read sem_cache").flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("zbase") {
            let mut data = fs::read(&p).expect("read zbase");
            if let Some(last) = data.last_mut() {
                *last ^= 0xAA;
            }
            fs::write(&p, &data).expect("write tampered zbase");
            corrupted += 1;
        }
    }
    assert!(corrupted >= 1, "expected at least one .zbase file");

    // Edit feat_b slightly so exact .zsem misses and forces .zbase read.
    fs::write(
        dir.join("feat_b.zil"),
        r#"
import svc_b
fn helper_b(x: i64) -> i64 {
    return x + 2
}
pub fn eval_b(x: i64) -> i64 {
    return svc_b.run_b(helper_b(x))
}
"#,
    )
    .expect("edit feat_b");

    let rep2 = zil_driver::compile_path(&main_path, &opts);
    assert_eq!(rep2.status, ResultCategory::Success);
    assert_eq!(rep2.verification_runs, 1);
    assert_eq!(
        rep2.reused_typeck_fns, 0,
        "tampered baseline must be rejected"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn struct_field_change_invalidates_transitive_dependents() {
    let dir = temp_dir("struct-field");
    let main_path = write_dag_fixture(&dir);
    let sem_cache = dir.join("sem-cache");

    let opts = CompileOptions {
        stop_after: Stage::Verify,
        semantic_cache: Some(sem_cache.clone()),
        ..Default::default()
    };

    let rep1 = zil_driver::compile_path(&main_path, &opts);
    assert_eq!(rep1.status, ResultCategory::Success);
    assert_eq!(rep1.verification_runs, 1);

    // Change CoreData field layout in `core.zil` (add field `c: i64` and update make_core/score_core)
    // while keeping function signatures identical -> every module transitively importing `core`
    // must re-lower against the new ADT layout without reusing stale Body IR!
    fs::write(
        dir.join("core.zil"),
        r#"
pub struct CoreData { pub a: i64; pub b: i64; pub c: i64 }
pub fn make_core(x: i64) -> CoreData {
    return CoreData { a: x + 1, b: x * 2, c: 10 }
}
pub fn score_core(d: CoreData) -> i64 {
    return d.a + d.b + d.c
}
"#,
    )
    .expect("edit core.zil struct layout");

    let rep2 = zil_driver::compile_path(&main_path, &opts);
    assert_eq!(
        rep2.status,
        ResultCategory::Success,
        "struct layout change must cleanly re-lower dependents: {:?}",
        rep2.diagnostics
    );
    assert_eq!(rep2.verification_runs, 1);
    assert_eq!(
        rep2.reused_typeck_fns, 0,
        "all modules transitively depend on core.CoreData and must invalidate"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn resident_session_multi_edit_sequence_reuses_typeck_and_certs() {
    let dir = temp_dir("resident-seq");
    let main_path = write_dag_fixture(&dir);

    let opts = CompileOptions {
        stop_after: Stage::Verify,
        ..Default::default()
    };

    let mut session = CompilationSession::with_capacity(64);
    // 1. Cold build in resident session.
    let r1 = session.compile_path(&main_path, &opts);
    assert_eq!(r1.status, ResultCategory::Success);
    assert_eq!(r1.verification_runs, 1);
    assert!(!r1.cache_hit);

    // 2. Warm no-edit build -> exact whole-program cache hit (100% reused).
    let r2 = session.compile_path(&main_path, &opts);
    assert_eq!(r2.status, ResultCategory::Success);
    assert_eq!(r2.verification_runs, 1);
    assert!(r2.cache_hit);
    assert!(r2.total_typeck_fns > 0 && r2.reused_typeck_fns == r2.total_typeck_fns);

    // 3. Private edit in svc_b -> reuses core, svc_a, feat_a, feat_b, main (both typeck & certs!)
    // while freshly re-lowering svc_b's 2 functions.
    let orig_svc_b = fs::read_to_string(dir.join("svc_b.zil")).expect("read svc_b");
    fs::write(
        dir.join("svc_b.zil"),
        r#"
import core
fn priv_b(x: i64) -> i64 {
    return x + 11
}
pub fn run_b(x: i64) -> i64 {
    let d = core.make_core(priv_b(x))
    return core.score_core(d)
}
"#,
    )
    .expect("edit svc_b");
    let r3 = session.compile_path(&main_path, &opts);
    assert_eq!(r3.status, ResultCategory::Success);
    assert_eq!(r3.verification_runs, 1);
    assert!(
        r3.reused_typeck_fns >= 7
            && r3.reused_typeck_fns < r3.total_typeck_fns
            && r3.reused_cert_fns >= 7,
        "expected partial >=7/<total reused typeck & cert fns in resident session, got typeck={}/{} cert={}",
        r3.reused_typeck_fns,
        r3.total_typeck_fns,
        r3.reused_cert_fns
    );

    // 4. Revert svc_b -> hits original snapshot in resident LRU (100% reused)!
    fs::write(dir.join("svc_b.zil"), orig_svc_b).expect("revert svc_b");
    let r4 = session.compile_path(&main_path, &opts);
    assert_eq!(r4.status, ResultCategory::Success);
    assert_eq!(r4.verification_runs, 1);
    assert!(r4.cache_hit);
    assert_eq!(r4.reused_typeck_fns, r4.total_typeck_fns);

    let _ = fs::remove_dir_all(&dir);
}
