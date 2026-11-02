//! Exact phase boundaries, including the unchanged flat-186 source and its
//! previously frozen Types418 / Analyze8673 / Verify10398 minima.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
const NEW: &str = include_str!("impl237_fixtures/original235.zil");
const OLD: &str = "resource R{n:i32}package P{pad:i32;manages old:R;wide:[i32;2];manages fresh:R}fn origin(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let q=p;return give q.fresh}fn main(){}";
fn bounded<T>(r: Result<T, Vec<zil_base::Diagnostic>>) -> bool {
    match r {
        Ok(_) => true,
        Err(ds) => {
            assert!(!ds.is_empty());
            assert!(
                ds.iter()
                    .all(|d| d.category == ResultCategory::ResourceLimit),
                "{ds:?}"
            );
            false
        }
    }
}
fn boundary(mut run: impl FnMut(usize) -> bool, max: usize) -> usize {
    assert!(run(max));
    let (mut lo, mut hi) = (0, max);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if run(mid) { hi = mid } else { lo = mid }
    }
    assert!(run(hi));
    assert!(!run(hi - 1));
    assert!(run(hi));
    hi
}
fn measure(label: &str, text: &str) -> [usize; 3] {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget237.zil", text);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let defaults = Limits::default();
    let module = zil_types::check(&core, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&module, &defaults).unwrap();
    let limits = |n| Limits {
        max_analysis_steps: n,
        ..defaults.clone()
    };
    let types = boundary(
        |n| {
            let r = zil_types::check(&core, &target, &limits(n));
            if let Ok(actual) = &r {
                assert_eq!(actual, &module)
            }
            bounded(r)
        },
        defaults.max_analysis_steps,
    );
    let analyze = boundary(
        |n| {
            let r = zil_analyze::analyze(&module, &limits(n));
            if let Ok(actual) = &r {
                assert_eq!(actual, &plan)
            }
            bounded(r)
        },
        defaults.max_analysis_steps,
    );
    let verify = boundary(
        |n| {
            bounded(zil_verify::verify_module(
                module.clone(),
                plan.clone(),
                &target,
                &limits(n),
            ))
        },
        defaults.max_analysis_steps,
    );
    println!("{label}: types={types},analyze={analyze},verify={verify}");
    if let Some(root) = std::env::var_os("ZIL_TEST_REPORT_DIR") {
        let d = PathBuf::from(root).join(format!("budget237-{label}"));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("source.zil"), text).unwrap();
        fs::write(d.join("minimum.json"),format!("{{\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"n\":true,\"n_minus_one_resource_limit\":true,\"fresh_n\":true}}\n")).unwrap();
    }
    [types, analyze, verify]
}
#[test]
fn generic_product_types_analyze_verify_boundaries() {
    measure("nested", NEW);
}
#[test]
fn empty_table_flat_186_boundaries_are_unchanged() {
    assert_eq!(measure("legacy", OLD), [418, 8673, 10398]);
}
