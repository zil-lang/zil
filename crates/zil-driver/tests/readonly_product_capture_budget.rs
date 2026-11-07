//! Actual N / N-1 / fresh-N boundaries for the newly admitted source profiles.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
const NEW: &str = include_str!("../../../examples/mainline/readonly_product232/local.zil");
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
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget232.zil", text);
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
        let d = PathBuf::from(root).join(format!("budget232-{label}"));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("source.zil"), text).unwrap();
        fs::write(d.join("minimum.json"),format!("{{\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"n\":true,\"n_minus_one_resource_limit\":true,\"fresh_n\":true}}\n")).unwrap();
    }
    [types, analyze, verify]
}
#[test]
fn default_recursive_product_boundaries() {
    measure("recursive", NEW);
}
#[test]
fn default_flat_product_boundaries() {
    let source = NEW
        .replace(
            "let item = Item { child: Leaf { ticket: Ticket { code: 65 }, value: 7 }, tag: 3 }",
            "let item = Leaf { ticket: Ticket { code: 65 }, value: 10 }",
        )
        .replace("item.child.value + item.tag", "item.value");
    measure("flat", &source);
}
#[test]
fn owning_product_projection_boundaries() {
    let source = NEW
        .replace("let callback = () =>", "let callback = move () =>")
        .replace("    let moved = item\n", "");
    measure("owned", &source);
}
