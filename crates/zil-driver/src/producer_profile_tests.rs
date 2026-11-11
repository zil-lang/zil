//! Detailed costs are subdivisions of actual requests, including rejected input.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use zil_base::{FileId, ModuleId};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-producer-profile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn project(&self, value: &str) -> ProjectSources {
        let main = "import left\nimport right\nfn value(x:i32)->i32{if x<5{return left.value(x)+right.value(x);}else{return right.value(x);}}";
        let left =
            format!("pub fn value(x:i32)->i32{{if x<7{{return x+{value};}}else{{return x+2;}}}}");
        let right = "pub fn value(x:i32)->i32{if x<7{return x+3;}else{return x+4;}}";
        let modules = [("main", main), ("left", left.as_str()), ("right", right)]
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
                let path = self.0.join(format!("{name}.zil"));
                fs::write(&path, text).unwrap();
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    path.to_string_lossy(),
                    *text,
                )
            })
            .collect();
        ProjectSources {
            root: ModuleId(0),
            modules,
            paths: BTreeMap::from([
                (vec!["left".into()], ModuleId(1)),
                (vec!["right".into()], ModuleId(2)),
            ]),
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn options(arm: u8, profile: bool) -> CompileOptions {
    CompileOptions {
        producer_reuse: arm != 0,
        reactive_queries: arm == 2,
        profile_compilation: profile,
        ..Default::default()
    }
}
fn same_behavior(actual: &CompileReport, expected: &CompileReport) {
    assert_eq!(
        actual.status,
        expected.status,
        "{}\n{}",
        actual.to_json(),
        expected.to_json()
    );
    assert_eq!(actual.phase, expected.phase);
    assert_eq!(actual.diagnostics, expected.diagnostics);
    assert_eq!(actual.input_fingerprint, expected.input_fingerprint);
    assert_eq!(actual.source_files, expected.source_files);
    assert_eq!(actual.scalar_interfaces, expected.scalar_interfaces);
    assert_eq!(actual.verification_runs, expected.verification_runs);
}
fn type_parts(work: &zil_types::FunctionStats) -> u128 {
    [
        work.shape_resolution_ns,
        work.context_construction_ns,
        work.projection_context_ns,
        work.core_validation_ns,
        work.inventory_ns,
        work.graph_maintenance_ns,
        work.evaluator_ns,
        work.cfg_reuse_ns,
        work.cfg_execute_ns,
        work.cfg_store_ns,
    ]
    .into_iter()
    .map(u128::from)
    .sum()
}
fn producer_parts(work: &zil_expand::ProducerStats) -> u128 {
    [
        work.context_ns,
        work.production_ns,
        work.reuse_ns,
        work.assembly_ns,
    ]
    .into_iter()
    .map(u128::from)
    .sum()
}
fn subdivisions(
    types: Option<zil_types::FunctionStats>,
    producer: Option<&zil_expand::ProducerStats>,
    phases: &BTreeMap<Phase, u128>,
) {
    if let Some(work) = types {
        assert!(work.cost_profiled);
        assert!(
            type_parts(&work) <= *phases.get(&Phase::Types).unwrap_or(&0),
            "Types subdivisions exceed phase: {work:?}, {phases:?}"
        );
        assert!(type_parts(&work) <= work.total_ns as u128);
        assert!(work.body_key_ns <= work.evaluator_ns);
    }
    if let Some(work) = producer {
        assert!(
            producer_parts(work) <= *phases.get(&Phase::Expand).unwrap_or(&0),
            "Expand subdivisions exceed phase: {work:?}, {phases:?}"
        );
    }
}
fn profiled(report: &CompileReport) {
    assert!(report.cost_profiled);
    subdivisions(
        report.function_type_work,
        report.producer_work.as_ref(),
        &report.phase_timings_ns,
    );
    for attempt in &report.attempts {
        subdivisions(
            attempt.function_type_work,
            attempt.producer_work.as_ref(),
            &attempt.phase_timings_ns,
        );
    }
    let classified: u128 = report
        .phase_timings_ns
        .values()
        .chain(report.driver_timings_ns.values())
        .copied()
        .sum();
    assert!(
        classified <= (report.seconds * 1_000_000_000.0) as u128,
        "phase/driver costs exceed complete request wall: {}",
        report.to_json()
    );
    let exclusive = report.exclusive_costs_json();
    assert_ne!(exclusive, "null");
    assert!(exclusive.contains("\"overlap_ns\":0"), "{exclusive}");
    assert!(
        exclusive.contains("\"overlap_errors_ns\":{}"),
        "{exclusive}"
    );
    assert!(!exclusive.contains("types_overlap_error"), "{exclusive}");
    assert!(!exclusive.contains("expand_overlap_error"), "{exclusive}");
}
fn unprofiled(report: &CompileReport) {
    assert!(!report.cost_profiled);
    assert!(report.driver_timings_ns.is_empty());
    assert_eq!(report.exclusive_costs_json(), "null");
    let check = |types: Option<zil_types::FunctionStats>,
                 producer: Option<&zil_expand::ProducerStats>| {
        if let Some(work) = types {
            assert!(!work.cost_profiled);
            assert_eq!(
                type_parts(&work) + work.total_ns as u128 + work.body_key_ns as u128,
                0
            );
        }
        if let Some(work) = producer {
            assert_eq!(producer_parts(work) + work.total_ns as u128, 0);
        }
    };
    check(report.function_type_work, report.producer_work.as_ref());
    for attempt in &report.attempts {
        assert!(attempt.driver_timings_ns.is_empty());
        check(attempt.function_type_work, attempt.producer_work.as_ref());
    }
}

#[test]
fn detailed_abc_costs_partition_cold_edit_error_fix_and_unchanged_requests() {
    for arm in 0..3 {
        let directory = Directory::new();
        let mut session = CompilationSession::default();
        for (index, value) in ["1", "5", "missing", "6", "6"].into_iter().enumerate() {
            let project = directory.project(value);
            let report = session.compile_project(&project, &options(arm, true));
            let fresh = CompilationSession::default().compile_project(&project, &options(0, false));
            same_behavior(&report, &fresh);
            assert_eq!(report.status == ResultCategory::Success, index != 2);
            assert_eq!(report.verification_runs, usize::from(index != 2));
            profiled(&report);
            if index == 0 {
                let work = report
                    .function_type_work
                    .expect("cold Types really executed");
                assert!(work.executed_functions > 0);
                assert!(work.total_ns > 0);
                if arm != 0 {
                    let work = report
                        .producer_work
                        .as_ref()
                        .expect("cold producer really executed");
                    assert_eq!(work.lower_modules, 3);
                    assert!(work.production_ns > 0);
                }
            }
            if index == 1 && arm != 0 {
                let work = report
                    .producer_work
                    .as_ref()
                    .expect("edited producer really executed");
                assert!(work.lower_modules > 0);
                assert!(work.reused_lower_modules > 0);
            }
            assert_eq!(session.active_query_count(), 0);
            if index == 4 {
                assert!(report.function_type_work.is_none());
                assert!(report.producer_work.is_none());
            }
        }
    }
}

#[test]
fn disabled_detailed_clocks_preserve_current_abc_results_and_verification() {
    for arm in 0..3 {
        let directory = Directory::new();
        let mut ordinary = CompilationSession::default();
        let mut instrumented = CompilationSession::default();
        for value in ["1", "5", "missing", "6", "6"] {
            let project = directory.project(value);
            let report = ordinary.compile_project(&project, &options(arm, false));
            let measured = instrumented.compile_project(&project, &options(arm, true));
            same_behavior(&report, &measured);
            assert_eq!(report.verification_runs, usize::from(value != "missing"));
            unprofiled(&report);
            profiled(&measured);
            assert_eq!(ordinary.active_query_count(), 0);
            assert_eq!(instrumented.active_query_count(), 0);
        }
    }
}
