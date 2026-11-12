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

#[test]
fn saved_nested_lending_result_retains_the_complete_manager_chain() {
    let source = include_str!("../../../tests/fixtures/nested-store/lending.zil");
    let report = compile_project(
        &project(source),
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
    let wrong = source.replace(
        "Option.Some(item)=>{return item.value-42}",
        "Option.Some(item)=>{store_reserve(catalog.rows,4);return item.value-42}",
    );
    let report = compile_project(
        &project(&wrong),
        &CompileOptions {
            stop_after: Stage::Analyze,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
}

#[path = "../../../tests/fixtures/nested-store/raw-managers.rs"]
mod raw_managers;
#[test]
fn independently_written_prefix_inputs_enter_both_analyzer_transports() {
    use raw_managers::Scenario;
    let success = [
        Scenario::default(),
        Scenario {
            mutable_parent: true,
            ..Scenario::default()
        },
        Scenario {
            mutable_parent: true,
            mutable_leaf: true,
            ..Scenario::default()
        },
        Scenario {
            mutable_parent: true,
            reserve_before: true,
            through_helper: true,
            ..Scenario::default()
        },
    ];
    let reject = [
        Scenario {
            mutable_leaf: true,
            ..Scenario::default()
        },
        Scenario {
            reserve_before: true,
            ..Scenario::default()
        },
        Scenario {
            mutable_parent: true,
            reserve_inner_live: true,
            ..Scenario::default()
        },
        Scenario {
            reserve_outer_live: true,
            ..Scenario::default()
        },
        Scenario {
            retire_outer_live: true,
            ..Scenario::default()
        },
        Scenario {
            reserve_before: true,
            through_helper: true,
            ..Scenario::default()
        },
    ];
    let limits = zil_base::Limits::default();
    for (expected, cases) in [(true, success.as_slice()), (false, reject.as_slice())] {
        for (i, scenario) in cases.iter().enumerate() {
            let (module, manual) = raw_managers::request(*scenario);
            let wire = zil_ir::wire::encode_module_candidate(&module, &manual, &limits).unwrap();
            let (decoded, _) = zil_ir::wire::decode_module_candidate(&wire, &limits).unwrap();
            for (transport, module) in [("raw", &module), ("wire", &decoded)] {
                let result = zil_analyze::analyze(module, &limits);
                eprintln!(
                    "independent prefix A {expected}/{i}/{transport}: {:?}",
                    result
                        .as_ref()
                        .map(|_| ())
                        .map_err(|e| e.iter().map(|e| &e.message).collect::<Vec<_>>())
                );
                assert_eq!(
                    result.is_ok(),
                    expected,
                    "{expected}/{i}/{transport}: {result:?}"
                );
                if let Err(errors) = result {
                    assert!(
                        errors
                            .iter()
                            .all(|e| e.category == ResultCategory::SafeReject),
                        "{errors:?}"
                    )
                }
            }
        }
    }
}

#[test]
fn every_original_allocation_edge_and_moving_growth_preserve_nested_cleanup() {
    use std::os::unix::fs::PermissionsExt;
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use zil_driver::{NativeBackend, Optimization};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn quote(path: &std::path::Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
    }
    type Failure = (i32, &'static [u8], usize, usize);
    const RETRIES: [Failure; 6] = [(0, b"BSAR", 3, 3); 6];
    struct Case {
        name: &'static str,
        trace: &'static [u8],
        heads: usize,
        moves: usize,
        failures: &'static [Failure],
        retries: bool,
    }
    let cases = [
        Case {
            name: "drains",
            trace: b"AB",
            heads: 3,
            moves: 1,
            failures: &[
                (1, b"", 0, 0),
                (1, b"", 1, 0),
                (2, b"BA", 2, 0),
                (4, b"AB", 3, 0),
            ],
            retries: false,
        },
        Case {
            name: "packages",
            trace: b"AR",
            heads: 2,
            moves: 2,
            failures: &[
                (1, b"", 0, 0),
                (2, b"A", 1, 0),
                (3, b"AR", 2, 0),
                (4, b"AR", 2, 1),
            ],
            retries: false,
        },
        Case {
            name: "direct-store",
            trace: b"A",
            heads: 2,
            moves: 1,
            failures: &[(1, b"", 0, 0), (2, b"A", 1, 0), (3, b"A", 2, 0)],
            retries: false,
        },
        Case {
            name: "direct-ticket",
            trace: b"BA",
            heads: 1,
            moves: 1,
            failures: &[(1, b"", 0, 0), (2, b"AB", 1, 0)],
            retries: false,
        },
        Case {
            name: "direct-byte-buffer",
            trace: b"",
            heads: 2,
            moves: 1,
            failures: &[(1, b"", 0, 0), (2, b"", 1, 0), (3, b"", 2, 0)],
            retries: false,
        },
        Case {
            name: "recovery",
            trace: b"BSAR",
            heads: 3,
            moves: 3,
            failures: &RETRIES,
            retries: true,
        },
    ];
    for Case {
        name,
        trace,
        heads,
        moves,
        failures,
        retries,
    } in cases
    {
        let source = if name == "recovery" {
            include_str!("../../../tests/fixtures/nested-store/recovery.zil")
        } else {
            CASES
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .unwrap()
                .1
        };
        for backend in [NativeBackend::Clang, NativeBackend::LlvmApi] {
            if backend == NativeBackend::LlvmApi && !cfg!(feature = "llvm-api") {
                continue;
            }
            for optimization in [Optimization::O0, Optimization::O2] {
                let base = std::env::var_os("ZIL_TEST_REPORT_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("../../out/runs/nested-store-native")
                    });
                let dir = base.join(format!(
                    "{name}-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(&dir).unwrap();
                let clang = std::env::var_os("ZIL_CLANG")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| "clang".into());
                let observer = dir.join("allocation.o");
                let output = Command::new(&clang)
                    .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-c"])
                    .arg(
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("../../tests/fixtures/nested-store/allocation.c"),
                    )
                    .arg("-o")
                    .arg(&observer)
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{output:?}");
                let wrapper = dir.join("link.sh");
                fs::write(&wrapper,format!("#!/bin/sh\nset -eu\nlink=1\nfor arg in \"$@\"; do case \"$arg\" in -c|-S|-E|--version) link=0;; esac; done\nif [ \"$link\" = 1 ]; then exec {} \"$@\" -x none {} -Wl,--wrap=realloc -Wl,--wrap=free; fi\nexec {} \"$@\"\n",quote(&clang),quote(&observer),quote(&clang))).unwrap();
                fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
                let executable = dir.join("program");
                let report = compile_project(
                    &project(source),
                    &CompileOptions {
                        stop_after: Stage::Build,
                        backend,
                        optimization,
                        clang: Some(wrapper),
                        output: Some(executable.clone()),
                        ..CompileOptions::default()
                    },
                );
                fs::write(dir.join("compile.json"), report.to_json()).unwrap();
                assert_eq!(
                    report.status,
                    ResultCategory::Success,
                    "{name}/{backend:?}/{optimization:?}: {}",
                    report.to_json()
                );
                let generated = report.native_codegen.as_ref().unwrap();
                assert_eq!(generated.backend, backend);
                assert!(generated.fallback.is_none());
                assert_eq!(report.verification_runs, 1);
                for at in 0..=failures.len() {
                    let (exit, trace, acquired, moved) = if at == 0 {
                        (0, trace, heads, moves)
                    } else {
                        failures[at - 1]
                    };
                    let mut command = Command::new(&executable);
                    if at > 0 {
                        command.env("ZIL_NESTED_FAIL_AT", at.to_string());
                    }
                    let output = command.output().unwrap();
                    fs::write(dir.join(format!("run-{at}.txt")), format!("{output:?}")).unwrap();
                    assert_eq!(
                        output.status.code(),
                        Some(exit),
                        "{name}/{backend:?}/{optimization:?}/{at}: {output:?}"
                    );
                    assert!(output.stderr.is_empty(), "{output:?}");
                    let split = output.stdout.iter().position(|byte| *byte == b'|').unwrap();
                    assert_eq!(
                        &output.stdout[..split],
                        trace,
                        "{name}/{backend:?}/{optimization:?}/{at}"
                    );
                    let counts = std::str::from_utf8(&output.stdout[split + 1..])
                        .unwrap()
                        .split(':')
                        .map(|n| n.parse::<usize>().unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(
                        counts,
                        [
                            if at == 0 {
                                failures.len()
                            } else if retries {
                                failures.len() + 1
                            } else {
                                at
                            },
                            acquired,
                            acquired,
                            0,
                            moved,
                            0
                        ],
                        "{name}/{backend:?}/{optimization:?}/{at}"
                    );
                    eprintln!(
                        "nested native {name}/{backend:?}/{optimization:?} fail={at}: {:?}",
                        String::from_utf8_lossy(&output.stdout)
                    );
                }
            }
        }
    }
}
