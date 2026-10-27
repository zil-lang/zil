//! File-discovery cache correctness is checked by executing edited callees.
use std::{fs, path::PathBuf, process::Command};
use zil_base::ResultCategory;
use zil_driver::{CompilationSession, CompileOptions, Optimization, Stage};
#[test]
fn warm_filesystem_edits_import_order_and_failed_requests_do_not_reuse_stale_syntax() {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("files158-{}", std::process::id()));
    for opt in [Optimization::O0, Optimization::O2] {
        let dir = base.join(format!("{opt:?}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("zil.toml"), "[package]\nname=\"files158\"\n").unwrap();
        fs::write(dir.join("aux.zil"), "pub fn offset()->i32=>1").unwrap();
        let root = dir.join("main.zil");
        let leaf = dir.join("leaf.zil");
        let mut warm = CompilationSession::with_capacity(1024);
        for (step, (value, reverse)) in [
            (2, false),
            (2, false),
            (7, false),
            (7, true),
            (11, true),
            (3, false),
        ]
        .into_iter()
        .enumerate()
        {
            fs::write(
                &root,
                if reverse {
                    "import aux; import leaf; fn main()->i32=>leaf.value()+aux.offset()"
                } else {
                    "import leaf; import aux; fn main()->i32=>leaf.value()+aux.offset()"
                },
            )
            .unwrap();
            fs::write(
                &leaf,
                format!("fn private()->i32=>{value}; pub fn value()->i32=>private()"),
            )
            .unwrap();
            let out = dir.join(format!("step{step}"));
            fs::create_dir_all(&out).unwrap();
            let options = CompileOptions {
                stop_after: Stage::Build,
                optimization: opt,
                output: Some(out.join("warm")),
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            };
            let r = warm.compile_path(&root, &options);
            fs::write(out.join("compile.json"), r.to_json()).unwrap();
            assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
            assert_eq!(r.verification_runs, 1);
            assert_eq!(warm.active_query_count(), 0);
            let p = Command::new(out.join("warm")).output().unwrap();
            fs::write(out.join("native.stdout"), &p.stdout).unwrap();
            fs::write(out.join("native.stderr"), &p.stderr).unwrap();
            assert_eq!(p.status.code(), Some(value + 1));
            if step == 1 {
                assert!(r.cache_hit)
            }
            if step == 2 {
                fs::write(&leaf, "pub fn value()->bool=>true").unwrap();
                let bad = warm.compile_path(&root, &CompileOptions::default());
                fs::write(out.join("bad-signature.json"), bad.to_json()).unwrap();
                assert_eq!(bad.status, ResultCategory::SafeReject);
                assert_eq!(warm.active_query_count(), 0);
            }
            if step == 3 {
                let amb = dir.join("leaf");
                fs::create_dir_all(&amb).unwrap();
                fs::write(amb.join("mod.zil"), "pub fn value()->i32=>99").unwrap();
                let bad = warm.compile_path(&root, &CompileOptions::default());
                fs::write(out.join("ambiguous.json"), bad.to_json()).unwrap();
                assert_eq!(bad.status, ResultCategory::SafeReject);
                fs::remove_dir_all(amb).unwrap();
                assert_eq!(warm.active_query_count(), 0);
            }
            if step == 4 {
                let mut low = CompileOptions::default();
                low.limits.max_tokens = 2;
                let bad = warm.compile_path(&root, &low);
                fs::write(out.join("low-quota.json"), bad.to_json()).unwrap();
                assert_eq!(bad.status, ResultCategory::ResourceLimit);
                assert_eq!(warm.active_query_count(), 0);
            }
        }
    }
}
