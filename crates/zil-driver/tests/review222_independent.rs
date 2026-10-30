//! Non-author 222 review: independent name and cumulative-budget controls.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, Stage};

fn out() -> PathBuf {
    let p = PathBuf::from(std::env::var_os("ZIL_TEST_REPORT_DIR").unwrap()).join("review222-api");
    fs::create_dir_all(&p).unwrap();
    p
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review222.zil", text)
}

#[test]
fn nominal_constructor_uses_the_same_namespace_after_single_generation() {
    let plain = "struct Cell{x:i32}fn main()->i32{let Cell=8;let c=Cell{x:3};return c.x+Cell-11}";
    let generated = "struct Cell{x:i32}fn main()->i32{#for(n in [3]){let Cell=8;let c=Cell{x:n};return c.x+Cell-11}}";
    let p = out();
    let mut results = Vec::new();
    for (name, text) in [
        ("namespace-plain", plain),
        ("namespace-generated", generated),
    ] {
        fs::write(p.join(format!("{name}.zil")), text).unwrap();
        let report =
            CompilationSession::default().compile_source(&source(text), &CompileOptions::default());
        fs::write(p.join(format!("{name}.json")), report.to_json()).unwrap();
        results.push(report.status);
    }
    assert_eq!(
        results[0],
        ResultCategory::Success,
        "ordinary control changed"
    );
    assert_eq!(
        results[1], results[0],
        "one generation must preserve the ordinary constructor binding"
    );
}

const BUDGET: &str = "fn extent(k:i32)->i32{if k<1{return 1}return k}fn main()->i32{var sum=0;#for(n in [2,3]){#for(i in 0..extent(n)){#if(i<2){sum+=i}}}let later=#(extent(4));return sum+later-6}";

fn minimum(stage: Stage, nodes: bool) -> usize {
    let (mut low, mut high) = (0, if nodes { 90_000 } else { 10_000 });
    while high - low > 1 {
        let n = (low + high) / 2;
        let mut limits = Limits::default();
        if nodes {
            limits.max_nodes = n;
        } else {
            limits.max_ct_steps = n as u64;
        }
        let report = CompilationSession::default().compile_source(
            &source(BUDGET),
            &CompileOptions {
                stop_after: stage,
                limits,
                ..CompileOptions::default()
            },
        );
        if report.status == ResultCategory::Success {
            high = n;
        } else {
            assert_eq!(
                report.status,
                ResultCategory::ResourceLimit,
                "{}",
                report.to_json()
            );
            low = n;
        }
    }
    high
}

#[test]
fn nested_generations_conditions_and_later_query_share_fresh_and_warm_limits() {
    let core = minimum(Stage::Core, false);
    let full = minimum(Stage::Verify, false);
    let nodes = minimum(Stage::Verify, true);
    assert!(full > core);
    let p = out();
    fs::write(p.join("budget.zil"), BUDGET).unwrap();
    fs::write(
        p.join("minimum.json"),
        format!("{{\"core_steps\":{core},\"full_steps\":{full},\"full_nodes\":{nodes}}}\n"),
    )
    .unwrap();
    for (label, exact, node_budget) in [("steps", full, false), ("nodes", nodes, true)] {
        let mut session = CompilationSession::with_capacity(128);
        for (attempt, n) in [exact, exact, exact - 1, exact].into_iter().enumerate() {
            let mut limits = Limits::default();
            if node_budget {
                limits.max_nodes = n;
            } else {
                limits.max_ct_steps = n as u64;
            }
            let report = session.compile_source(
                &source(BUDGET),
                &CompileOptions {
                    limits,
                    ..CompileOptions::default()
                },
            );
            fs::write(p.join(format!("{label}-{attempt}.json")), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                if attempt == 2 {
                    ResultCategory::ResourceLimit
                } else {
                    ResultCategory::Success
                },
                "{}",
                report.to_json()
            );
            assert_eq!(session.active_query_count(), 0);
            if attempt != 2 {
                assert_eq!(report.verification_runs, 1);
            }
            if attempt == 1 || attempt == 3 {
                assert!(report.cache_hit);
            }
        }
    }
}
