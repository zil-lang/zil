//! Original ordinary consumers for recursive owning prefix storage.
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompileOptions, ProjectSources, Stage, compile_project};
const CASES: &[(&str, &str)] = &[
    (
        "drains",
        include_str!("../../../tests/fixtures/nested-store/drains.zil"),
    ),
    (
        "packages",
        include_str!("../../../tests/fixtures/nested-store/packages.zil"),
    ),
    (
        "direct-store",
        include_str!("../../../tests/fixtures/nested-store/direct-store.zil"),
    ),
    (
        "direct-ticket",
        include_str!("../../../tests/fixtures/nested-store/direct-ticket.zil"),
    ),
    (
        "direct-byte-buffer",
        include_str!("../../../tests/fixtures/nested-store/direct-byte-buffer.zil"),
    ),
];
fn project(source: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: [
            ("main.zil", source),
            ("std.iter.zil", include_str!("../../../lib/std/iter.zil")),
            ("std.store.zil", include_str!("../../../lib/std/store.zil")),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (name, text))| SourceUnit::new(FileId(i as u32), ModuleId(i as u32), name, text))
        .collect(),
        paths: [
            (vec!["std".into(), "iter".into()], ModuleId(1)),
            (vec!["std".into(), "store".into()], ModuleId(2)),
        ]
        .into(),
    }
}
#[test]
fn original_finite_recursive_owners_reach_types_and_analyze() {
    let mut failures = Vec::new();
    for stage in [Stage::Types, Stage::Analyze] {
        for (name, source) in CASES {
            let report = compile_project(
                &project(source),
                &CompileOptions {
                    stop_after: stage,
                    ..CompileOptions::default()
                },
            );
            eprintln!("recursive Store {name}/{stage:?}: {}", report.to_json());
            if report.status != ResultCategory::Success {
                failures.push(format!("{name}/{stage:?}"))
            }
        }
    }
    assert!(
        failures.is_empty(),
        "unclosed actual recursive consumers: {failures:?}"
    );
}

#[test]
fn actual_nested_loans_survive_helpers_and_block_each_parent_retirement() {
    let source = CASES
        .iter()
        .find(|(name, _)| *name == "packages")
        .unwrap()
        .1;
    let with_helper = source.replace(
        "fn main()->i32",
        "fn grow(items:Store<Item>)->bool{return store_reserve(items,2)}fn main()->i32",
    );
    let valid = with_helper.replace("if !store_reserve(row.inner,2)", "if !grow(row.inner)");
    let report = compile_project(
        &project(&valid),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    let negative=[
        ("shared forwarded manager",valid.replace("let row=store_at_mut(outer,0);if !grow", "let row=store_at(outer,0);if !grow")),
        ("outer reserve with live child",source.replace("total=item.value", "store_reserve(outer,4);total=item.value")),
        ("inner reserve with live child",source.replace("let row=store_at(outer,0);let item=store_at(row.inner,0);total=item.value", "let row=store_at_mut(outer,0);let item=store_at(row.inner,0);store_reserve(row.inner,4);total=item.value")),
        ("whole owner consumption",source.replace("total=item.value", "let moved=iter.drain(give outer);total=item.value").replace("extern ","import std.iter\nextern ")),
    ];
    for (name, source) in negative {
        let report = compile_project(
            &project(&source),
            &CompileOptions {
                stop_after: Stage::Analyze,
                ..CompileOptions::default()
            },
        );
        eprintln!("nested loan {name}: {}", report.to_json());
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            report.to_json()
        );
    }
}
