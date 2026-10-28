//! The real four-module consumer crosses the three independently prepared slices.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, CompileReport, ProjectSources, compile_project};
static NEXT: AtomicUsize = AtomicUsize::new(0);
const MAIN: &str = include_str!("../../../examples/mainline/zui_language181/composed.zil");
const MODEL: &str = include_str!("../../../examples/mainline/zui_language181/model.zil");
const COLLECTION: &str = include_str!("../../../examples/mainline/zui_language181/collection.zil");
const STATE: &str = include_str!("../../../examples/mainline/zui_language181/state.zil");

fn check(files: &[(&str, &str)]) -> CompileReport {
    let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = base.join(format!(
        "composition181-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    let project = ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                fs::write(path.join(format!("{name}.zil")), text).unwrap();
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect::<BTreeMap<_, _>>(),
    };
    let report = compile_project(
        &project,
        &CompileOptions {
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("compile.json"), report.to_json()).unwrap();
    report
}

#[test]
fn exact_generic_state_simultaneously_uses_callback_store_window_and_copy_sibling() {
    let report = check(&[
        ("main", MAIN),
        ("model", MODEL),
        ("collection", COLLECTION),
        ("state", STATE),
    ]);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    assert_eq!(report.source_files.len(), 4);
}

#[test]
fn the_same_state_still_rejects_structural_store_mutation_during_the_window() {
    let invalid = STATE.replace(
        "let again = callback()",
        "collection.reserve(current.items, count + 20)\n        let again = callback()",
    );
    let report = check(&[
        ("main", MAIN),
        ("model", MODEL),
        ("collection", COLLECTION),
        ("state", &invalid),
    ]);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-BORROW-CONFLICT")
    );
}

#[test]
fn a_shared_window_does_not_gain_write_permission_from_generic_state() {
    let invalid = STATE.replace(
        "store_at_mut(current.items, index)",
        "store_at(current.items, index)",
    );
    let report = check(&[
        ("main", MAIN),
        ("model", MODEL),
        ("collection", COLLECTION),
        ("state", &invalid),
    ]);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn concrete_address_projection_is_repeatable_but_never_grants_unknown_fields_or_ownership() {
    let good = "resource R{n:i32}package Item{manages r:R;value:i32}fn read<T>(p:&mut Item)->i32{let first=p.value;let second=p.value;return first+second}fn main()->i32{let s:Store<Item>=store_new();store_reserve(s,1);store_push(s,Item{r:R{n:0},value:7});let p=store_at_mut(s,0);return read::<i32>(p)-14}";
    let report = check(&[("main", good)]);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    for definition in [
        "fn bad<T:StoreElement>(p:&T)->i32=>p.value",
        "fn bad<T>(p:&Item)->R=>p.r",
        "fn bad<T>(p:&Item)->Item=>*p",
        "fn bad<T>(p:&Item){p.value=8}",
        "fn bad<T>(p:&mut Item){let a=p;let b=p;}",
    ] {
        let source = format!(
            "resource R{{n:i32}}package Item{{manages r:R;value:i32}}{definition};fn main(){{}}"
        );
        let report = check(&[("main", &source)]);
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{definition}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
}
