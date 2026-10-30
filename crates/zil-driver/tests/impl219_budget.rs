//! Cumulative query costs and immutable raw-cache identities through the driver.
use std::{collections::BTreeMap, fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile};
use zil_driver::{CompilationSession, CompileOptions, ProjectSources, QueryDisposition, Stage};

const PROGRAM: &str = "fn count(n:i32)->i32{var i=0;while i<n{i+=1}return i}#if(count(3)==3){fn selected()->i32=>3}else{fn selected()->i32=>missing}fn main()->i32{#if(count(4)==4){let n=#(count(5))}else{let n=missing}return n-5}";
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "budget219.zil", text)
}
fn directory(label: &str) -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("budget219-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&p).unwrap();
    p
}
fn threshold(stage: Stage, nodes: bool) -> usize {
    let mut lo = 0;
    let mut hi = if nodes { 20_000 } else { 2_000 };
    let mut session = CompilationSession::with_capacity(256);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        let mut options = CompileOptions {
            stop_after: stage,
            ..CompileOptions::default()
        };
        if nodes {
            options.limits.max_nodes = mid;
        } else {
            options.limits.max_ct_steps = mid as u64;
        }
        let report = session.compile_source(&source(PROGRAM), &options);
        if report.status == ResultCategory::Success {
            hi = mid;
        } else {
            assert_eq!(
                report.status,
                ResultCategory::ResourceLimit,
                "{}",
                report.to_json()
            );
            lo = mid;
        }
        assert_eq!(session.active_query_count(), 0);
    }
    hi
}

#[test]
fn selection_and_later_constants_share_steps_nodes_and_fresh_request_recovery() {
    let core_steps = threshold(Stage::Core, false);
    let full_steps = threshold(Stage::Verify, false);
    assert!(
        full_steps > core_steps,
        "ordinary #(count(5)) must consume additional shared steps"
    );
    let nodes = threshold(Stage::Verify, true);
    let out = directory("thresholds");
    fs::write(out.join("input.zil"), PROGRAM).unwrap();
    fs::write(
        out.join("thresholds.json"),
        format!(
            "{{\"core_steps\":{core_steps},\"full_steps\":{full_steps},\"full_nodes\":{nodes}}}"
        ),
    )
    .unwrap();
    for (label, budget, nodes) in [("steps", full_steps, false), ("nodes", nodes, true)] {
        let mut session = CompilationSession::with_capacity(128);
        for (attempt, value) in [budget, budget - 1, budget, budget].into_iter().enumerate() {
            let mut options = CompileOptions {
                emit_dir: Some(out.join(format!("{label}-{attempt}"))),
                ..CompileOptions::default()
            };
            if nodes {
                options.limits.max_nodes = value;
            } else {
                options.limits.max_ct_steps = value as u64;
            }
            let report = session.compile_source(&source(PROGRAM), &options);
            fs::write(
                out.join(format!("{label}-{attempt}.json")),
                report.to_json(),
            )
            .unwrap();
            assert_eq!(
                report.status,
                if attempt == 1 {
                    ResultCategory::ResourceLimit
                } else {
                    ResultCategory::Success
                },
                "{}",
                report.to_json()
            );
            assert_eq!(session.active_query_count(), 0);
            if attempt != 1 {
                assert_eq!(report.verification_runs, 1);
            }
            if attempt == 3 {
                assert!(report.cache_hit);
            }
        }
    }
}

#[test]
fn target_and_limits_invalidate_selection_but_original_no_selection_paths_keep_their_identity() {
    let out = directory("cache");
    let mut session = CompilationSession::with_capacity(128);
    let input = source(
        "fn enabled()->bool=>true;fn main()->i32{#if(enabled()){return 0}else{return missing}}",
    );
    let native = CompileOptions {
        emit_dir: Some(out.join("native")),
        ..CompileOptions::default()
    };
    let first = session.compile_source(&input, &native);
    assert_eq!(first.status, ResultCategory::Success, "{}", first.to_json());
    let selected = fs::read(&first.artifacts["selection"]).unwrap();
    let hot = session.compile_source(
        &input,
        &CompileOptions {
            emit_dir: Some(out.join("hot")),
            ..native.clone()
        },
    );
    assert_eq!(hot.status, ResultCategory::Success, "{}", hot.to_json());
    assert_eq!(selected, fs::read(&hot.artifacts["selection"]).unwrap());
    assert!(
        hot.query_events
            .iter()
            .any(|e| e.phase == Phase::Expand && e.disposition == QueryDisposition::Reused)
    );
    assert_eq!(hot.verification_runs, 1);
    let windows = session.compile_source(
        &input,
        &CompileOptions {
            target: TargetProfile::windows_x64_llvm(),
            ..native.clone()
        },
    );
    assert_eq!(
        windows.status,
        ResultCategory::Success,
        "{}",
        windows.to_json()
    );
    assert!(
        windows
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Parse && e.disposition == QueryDisposition::Reused)
    );
    assert!(
        windows
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Expand && e.disposition == QueryDisposition::Executed)
    );
    let low = session.compile_source(
        &input,
        &CompileOptions {
            limits: Limits {
                max_ct_steps: 0,
                ..Limits::default()
            },
            ..native
        },
    );
    assert_eq!(
        low.status,
        ResultCategory::ResourceLimit,
        "{}",
        low.to_json()
    );
    assert_eq!(session.active_query_count(), 0);
    let old = source("// #if is only a comment\nfn main()->i32=>0");
    let options = CompileOptions {
        limits: Limits {
            max_ct_steps: 0,
            ..Limits::default()
        },
        ..CompileOptions::default()
    };
    let first = session.compile_source(&old, &options);
    assert_eq!(first.status, ResultCategory::Success, "{}", first.to_json());
    let changed = session.compile_source(
        &old,
        &CompileOptions {
            target: TargetProfile::windows_x64_llvm(),
            ..options
        },
    );
    assert_eq!(
        changed.status,
        ResultCategory::Success,
        "{}",
        changed.to_json()
    );
    assert!(
        changed
            .query_events
            .iter()
            .any(|e| e.phase == Phase::Expand && e.disposition == QueryDisposition::Reused)
    );
}

#[test]
fn project_source_dependency_edits_reselect_with_fresh_independent_verification() {
    let out = directory("project-cache");
    let mut session = CompilationSession::with_capacity(128);
    let project = |value: bool| ProjectSources {
        root: ModuleId(0),
        modules: vec![
            SourceUnit::new(
                FileId(0),
                ModuleId(0),
                "main.zil",
                "import config\nfn main()->i32{#if(config.enabled()){return 0}else{return 1}}",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "config.zil",
                format!("fn helper()->bool=>{value};pub fn enabled()->bool=>helper()"),
            ),
        ],
        paths: BTreeMap::from([(vec!["config".into()], ModuleId(1))]),
    };
    for (attempt, value) in [true, true, false, true].into_iter().enumerate() {
        let report = session.compile_project(
            &project(value),
            &CompileOptions {
                emit_dir: Some(out.join(attempt.to_string())),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join(format!("{attempt}.json")), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let trace = fs::read_to_string(&report.artifacts["selection"]).unwrap();
        assert!(
            trace.contains(&format!("\"selected_then\":{value}")),
            "{trace}"
        );
        if attempt == 2 {
            assert!(
                report.query_events.iter().any(
                    |e| e.phase == Phase::Expand && e.disposition == QueryDisposition::Executed
                )
            );
        }
        if attempt == 1 {
            assert!(
                report
                    .query_events
                    .iter()
                    .any(|e| e.phase == Phase::Expand && e.disposition == QueryDisposition::Reused)
            );
        }
        assert_eq!(session.active_query_count(), 0);
    }
}
