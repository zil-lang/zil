//! Exact caller-limit boundaries for new Store capture work and an old source.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const OLD_BYTES: &str = "fn make()->impl Fn()->i32{let b=bytes_new();return move()=>i32(bytes_len(b))}fn main()->i32{let callback=make();return callback()}";
const DIRECT: &str = include_str!("impl204_fixtures/01_direct_store_capture.zil");
const PACKAGE: &str = include_str!("impl204_fixtures/02_original201_package_store_capture.zil");
const MUTATING: &str = include_str!("impl204_fixtures/03_mutating_store_capture.zil");

fn boundary(label: &str, source: &str) -> usize {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "budget204.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    let target = TargetProfile::host_llvm();
    let limits = Limits::default();
    let expected = zil_types::check(&core, &target, &limits).unwrap();
    let check = |work| {
        zil_types::check(
            &core,
            &target,
            &Limits {
                max_analysis_steps: work,
                ..limits.clone()
            },
        )
    };
    let (mut low, mut high) = (0, limits.max_analysis_steps);
    while low + 1 < high {
        let work = low + (high - low) / 2;
        match check(work) {
            Ok(module) => {
                assert_eq!(module, expected);
                high = work;
            }
            Err(errors) => {
                assert!(
                    errors.iter().all(|error| {
                        error.phase == Phase::Types
                            && error.category == ResultCategory::ResourceLimit
                    }),
                    "{errors:#?}"
                );
                low = work;
            }
        }
    }
    assert_eq!(check(high).unwrap(), expected);
    let below = check(high - 1).unwrap_err();
    assert!(below.iter().all(|error| {
        error.phase == Phase::Types && error.category == ResultCategory::ResourceLimit
    }));
    assert_eq!(check(high).unwrap(), expected);
    let directory = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "capture204-budget-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("source.zil"), source).unwrap();
    fs::write(directory.join("core.txt"), format!("{core:#?}")).unwrap();
    fs::write(directory.join("typed.txt"), format!("{expected:#?}")).unwrap();
    fs::write(directory.join("below.txt"), format!("{below:#?}")).unwrap();
    fs::write(directory.join("boundary.json"), format!("{{\"types\":{high},\"checks\":\"N, N-1 ResourceLimit, fresh N; every successful Typed module equals default\"}}\n")).unwrap();
    high
}

#[test]
fn new_capture_types_work_has_an_exact_fresh_boundary() {
    for (label, source) in [
        ("direct", DIRECT),
        ("package", PACKAGE),
        ("mutating", MUTATING),
    ] {
        boundary(label, source);
    }
    let nested = PACKAGE
        .replace("impl Fn() -> i32", "impl FnOnce() -> i32")
        .replace(
            "return move () => { return i32(store_len(bag.items)) }",
            "return () => { let child = () => { let taken = give bag.items; return i32(store_len(taken)) }; return child() }",
        );
    assert!(nested.contains("let child"));
    boundary("nested-give", &nested);
}

#[test]
fn legacy_byte_capture_keeps_its_pre204_exact_types_budget() {
    // Independently measured on the old production domain by review/204-by197
    // at a94bd81d, before any 204 production merge. Do not derive this constant
    // from the new implementation or treat the new sources as legacy controls.
    assert_eq!(boundary("legacy-byte", OLD_BYTES), 196);
}
