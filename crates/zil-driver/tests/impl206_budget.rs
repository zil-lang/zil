//! Exact phase work boundaries, including an unchanged flat-186 source.
use std::{fmt::Debug, fs, path::PathBuf};
use zil_base::{Diagnostic, FileId, Limits, ModuleId, ResultCategory, SourceUnit, TargetProfile};
const FLAT: &str = "resource R{n:i32}package P{pad:i32;manages old:R;wide:[i32;2];manages fresh:R}fn origin(give old:R)->R contract{result(create)}{let p=P{pad:7,old:give old,wide:[11,13],fresh:R{n:29}};let q=p;return give q.fresh}fn main(){}";
fn boundary<T: PartialEq + Debug>(
    mut run: impl FnMut(usize) -> Result<T, Vec<Diagnostic>>,
) -> (usize, T) {
    let mut low = 0;
    let mut high = Limits::default().max_analysis_steps;
    let original = run(high).unwrap();
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match run(middle) {
            Ok(_) => high = middle,
            Err(errors) => {
                assert!(
                    errors
                        .iter()
                        .all(|d| d.category == ResultCategory::ResourceLimit),
                    "{errors:#?}"
                );
                low = middle;
            }
        }
    }
    let exact = run(high).unwrap();
    assert_eq!(exact, original);
    let errors = run(high - 1).unwrap_err();
    assert!(
        errors
            .iter()
            .all(|d| d.category == ResultCategory::ResourceLimit),
        "{errors:#?}"
    );
    assert_eq!(exact, run(high).unwrap());
    (high, exact)
}
fn measure(label: &str, source: &str) -> (usize, usize) {
    let root = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("product206-budget-{label}-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("source.zil"), source).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget206.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    fs::write(root.join("core.txt"), format!("{core:#?}")).unwrap();
    let (types, module) = boundary(|max_analysis_steps| {
        zil_types::check(
            &core,
            &TargetProfile::host_llvm(),
            &Limits {
                max_analysis_steps,
                ..Limits::default()
            },
        )
    });
    fs::write(root.join("typed.txt"), format!("{module:#?}")).unwrap();
    let (analyze, plan) = boundary(|max_analysis_steps| {
        zil_analyze::analyze(
            &module,
            &Limits {
                max_analysis_steps,
                ..Limits::default()
            },
        )
    });
    fs::write(root.join("candidate.txt"), format!("{plan:#?}")).unwrap();
    fs::write(root.join("boundary.json"),format!("{{\"types\":{types},\"analyze\":{analyze},\"protocol\":\"N / N-1 ResourceLimit / fresh N exact output equality\"}}\n")).unwrap();
    println!("{label}: Types={types}, Analyze={analyze}");
    (types, analyze)
}
#[test]
fn old_flat_package_records_exact_phase_boundaries() {
    // Measured with these identical source bytes and this probe on accepted
    // 205 (61c2d252), before enabling the new Product profile.
    assert_eq!(measure("old-flat", FLAT), (418, 8673));
}
#[test]
fn new_product_leaf_tables_have_exact_fresh_boundaries() {
    for (label, source) in [
        (
            "mixed",
            include_str!("impl206_fixtures/01_mixed_child_created.zil"),
        ),
        (
            "whole",
            include_str!("impl206_fixtures/02_whole_transfer_twice_created.zil"),
        ),
        (
            "wrappers",
            include_str!("impl206_fixtures/03_child_only_wrappers_created.zil"),
        ),
    ] {
        measure(label, source);
    }
}
