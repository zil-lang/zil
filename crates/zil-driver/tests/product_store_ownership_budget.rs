//! Newly visited Product Store trees pay real preflight/layout/provenance work.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
const SOURCE: &str = "resource R{n:i32}package Leaf{manages owner:R;n:i32}package Root{child:Leaf;tag:i32}fn main(){let items:Store<Root>=store_new();store_push(items,Root{child:Leaf{owner:R{n:1},n:2},tag:3})}";
fn limit_only<T>(r: Result<T, Vec<zil_base::Diagnostic>>) -> bool {
    match r {
        Ok(_) => true,
        Err(ds) => {
            assert!(
                ds.iter()
                    .any(|d| d.category == ResultCategory::ResourceLimit),
                "unexpected rejection: {ds:?}"
            );
            false
        }
    }
}
fn boundary(mut run: impl FnMut(usize) -> bool, maximum: usize) -> usize {
    assert!(run(maximum));
    let (mut low, mut high) = (0, maximum);
    while high - low > 1 {
        let mid = low + (high - low) / 2;
        if run(mid) {
            high = mid
        } else {
            low = mid
        };
    }
    assert!(run(high));
    assert!(!run(high - 1));
    assert!(run(high));
    high
}
#[test]
fn new_product_store_has_n_n_minus_one_and_fresh_n_controls() {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget223.zil", SOURCE);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let defaults = Limits::default();
    let typed = zil_types::check(&core, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
    let types = boundary(
        |n| {
            limit_only(zil_types::check(
                &core,
                &target,
                &Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                },
            ))
        },
        defaults.max_analysis_steps,
    );
    let analyze = boundary(
        |n| {
            limit_only(zil_analyze::analyze(
                &typed,
                &Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                },
            ))
        },
        defaults.max_analysis_steps,
    );
    let verify = boundary(
        |n| {
            limit_only(zil_verify::verify_module(
                typed.clone(),
                plan.clone(),
                &target,
                &Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                },
            ))
        },
        defaults.max_analysis_steps,
    );
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let root = PathBuf::from(root).join("budget223");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("source.zil"), SOURCE).unwrap();
        fs::write(root.join("minimum.json"),format!("{{\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"n_pass\":true,\"n_minus_one_resource_limit\":true,\"fresh_n_pass\":true}}\n")).unwrap();
    }
    println!("Product Store logical minimum: Types={types}, Analyze={analyze}, Verify={verify}");
}
