//! Opt-in observation must preserve source semantics, cache authority and failure publication.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, QueryDisposition, Stage, compile_path,
    compile_source,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zil-stage-timings-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "timing.zil", text)
}
fn names(report: &CompileReport) -> Vec<&'static str> {
    report
        .timings
        .as_ref()
        .unwrap()
        .spans
        .iter()
        .map(|span| span.name)
        .collect()
}
fn closed_tree(report: &CompileReport) {
    let spans = &report.timings.as_ref().expect("requested timings").spans;
    assert!(!spans.is_empty());
    assert_eq!(spans[0].outcome, report.status.as_str());
    for (index, span) in spans.iter().enumerate() {
        assert_eq!(span.id, index);
        assert_ne!(span.outcome, "running", "{}", report.to_json());
        if let Some(parent) = span.parent {
            assert!(parent < span.id);
            let parent = &spans[parent];
            assert!(parent.start_ns <= span.start_ns);
            assert!(span.start_ns + span.wall_ns <= parent.start_ns + parent.wall_ns);
        }
    }
}

#[test]
fn timing_switch_is_not_a_query_key_and_verify_still_runs() {
    let source = input("fn main()->i32 { return 42; }");
    let mut session = CompilationSession::default();
    let mut options = CompileOptions {
        stop_after: Stage::Lower,
        ..CompileOptions::default()
    };
    let baseline = session.compile_source(&source, &options);
    assert_eq!(
        baseline.status,
        ResultCategory::Success,
        "{}",
        baseline.to_json()
    );
    assert!(baseline.timings.is_none());
    assert!(baseline.to_json().contains("\"timings\":null"));
    options.timings = true;
    let traced = session.compile_source(&source, &options);
    assert_eq!(traced.status, baseline.status);
    assert_eq!(traced.input_fingerprint, baseline.input_fingerprint);
    assert_eq!(traced.verification_runs, 1);
    assert!(traced.query_events.iter().all(|event| {
        matches!(event.phase, Phase::Verify | Phase::Lower)
            || event.disposition == QueryDisposition::Reused
    }));
    assert!(!names(&traced).contains(&"execute"));
    assert!(names(&traced).contains(&"verify"));
    assert!(names(&traced).contains(&"lower"));
    closed_tree(&traced);
    options.timings = false;
    let again = session.compile_source(&source, &options);
    assert_eq!(again.status, traced.status);
    assert_eq!(again.query_events, traced.query_events);
    assert_eq!(again.verification_runs, 1);
    assert!(again.timings.is_none());
}

#[test]
fn request_root_includes_real_session_capacity_eviction() {
    let mut session = CompilationSession::with_capacity(8);
    let first = input("fn main()->i32=>41");
    let second = input("fn main()->i32=>42");
    for source in [&first, &second] {
        let report = session.compile_source(source, &CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
    }
    assert!(
        session.cached_query_count() > 8,
        "exercise the actual capacity guard"
    );
    let report = session.compile_source(
        &second,
        &CompileOptions {
            timings: true,
            ..CompileOptions::default()
        },
    );
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    assert!(
        report
            .query_events
            .iter()
            .all(|event| event.disposition == QueryDisposition::Executed)
    );
    let spans = &report.timings.as_ref().unwrap().spans;
    let eviction = spans
        .iter()
        .find(|span| span.name == "session_eviction")
        .expect("real clear must be measured");
    assert_eq!(eviction.parent, Some(0));
    assert_eq!(eviction.outcome, "ok");
    closed_tree(&report);
}

#[test]
fn rejected_and_cancelled_requests_keep_only_actual_intervals() {
    let options = CompileOptions {
        timings: true,
        ..CompileOptions::default()
    };
    let rejected = compile_source(&input("fn main( {"), &options);
    assert_eq!(rejected.status, ResultCategory::SafeReject);
    assert_eq!(rejected.verification_runs, 0);
    assert!(names(&rejected).contains(&"parse"));
    assert!(!names(&rejected).contains(&"verify"));
    closed_tree(&rejected);
    options.cancellation.cancel();
    let cancelled = compile_source(&input("fn main()->i32=>0"), &options);
    assert_eq!(cancelled.status, ResultCategory::Cancelled);
    assert_eq!(names(&cancelled), vec!["compile_source"]);
    closed_tree(&cancelled);
}

#[test]
fn filesystem_discovery_failure_and_parse_stop_have_distinct_scopes() {
    let directory = Dir::new();
    let path = directory.0.join("main.zil");
    fs::write(&path, "import missing; fn main()->i32=>0").unwrap();
    let mut options = CompileOptions {
        timings: true,
        ..CompileOptions::default()
    };
    let failed = compile_path(&path, &options);
    assert_ne!(failed.status, ResultCategory::Success);
    assert!(names(&failed).contains(&"source_discovery"));
    assert!(names(&failed).contains(&"source_read"));
    assert!(!names(&failed).contains(&"compile_project"));
    closed_tree(&failed);
    options.stop_after = Stage::Parse;
    let parsed = compile_path(&path, &options);
    assert_eq!(
        parsed.status,
        ResultCategory::Success,
        "{}",
        parsed.to_json()
    );
    assert!(names(&parsed).contains(&"compile_source"));
    assert!(!names(&parsed).contains(&"verify"));
    closed_tree(&parsed);
}

#[cfg(all(feature = "llvm-api", target_os = "linux"))]
#[test]
fn native_cold_hit_and_link_failure_preserve_bytes_and_current_verification() {
    use zil_driver::{CodegenOptions, NativeBackend};
    let directory = Dir::new();
    let path = directory.0.join("main.zil");
    fs::write(&path, "import leaf; fn main()->i32=>leaf.answer()").unwrap();
    fs::write(directory.0.join("leaf.zil"), "pub fn answer()->i32=>42").unwrap();
    let mut options = CompileOptions {
        backend: NativeBackend::LlvmApi,
        stop_after: Stage::Build,
        output: Some(directory.0.join("program")),
        codegen: CodegenOptions { units: 2, jobs: 2 },
        ..CompileOptions::default()
    };
    let baseline = compile_path(&path, &options);
    assert_eq!(
        baseline.status,
        ResultCategory::Success,
        "{}",
        baseline.to_json()
    );
    let executable = fs::read(&baseline.artifacts["executable"]).unwrap();
    let sidecar = fs::read(&baseline.artifacts["llvm"]).unwrap();
    options.timings = true;
    options.object_cache = Some(directory.0.join("objects"));
    let cold = compile_path(&path, &options);
    assert_eq!(cold.status, ResultCategory::Success, "{}", cold.to_json());
    for phase in [
        "verify",
        "lower",
        "machine_lower",
        "llvm_text_render",
        "cgu_partition",
        "cache_setup",
        "codegen_units",
        "unit",
        "llvm",
        "llvm_setup",
        "machine_preflight",
        "llvm_builder",
        "llvm_verify_before",
        "llvm_optimize",
        "llvm_verify_after",
        "llvm_object_emit",
        "object_write",
        "link",
        "publication",
        "cache_store",
    ] {
        assert!(
            names(&cold).contains(&phase),
            "missing {phase}: {}",
            cold.to_json()
        );
    }
    closed_tree(&cold);
    let hit = compile_path(&path, &options);
    assert_eq!(hit.status, ResultCategory::Success, "{}", hit.to_json());
    assert_eq!(hit.verification_runs, 1);
    assert!(names(&hit).contains(&"cached_machine_preflight"));
    assert!(names(&hit).contains(&"object_write"));
    assert!(names(&hit).contains(&"link"));
    assert!(!names(&hit).contains(&"llvm_builder"));
    let native = hit.native_codegen.as_ref().unwrap();
    assert_eq!(native.object_cache.hits, native.actual_units);
    assert_eq!(fs::read(&hit.artifacts["executable"]).unwrap(), executable);
    assert_eq!(fs::read(&hit.artifacts["llvm"]).unwrap(), sidecar);
    assert_eq!(
        std::process::Command::new(&hit.artifacts["executable"])
            .status()
            .unwrap()
            .code(),
        Some(42)
    );
    closed_tree(&hit);
    options.clang = Some(directory.0.join("missing-link-driver"));
    options.output = Some(directory.0.join("failed-program"));
    let failed = compile_path(&path, &options);
    assert_eq!(
        failed.status,
        ResultCategory::InfraError,
        "{}",
        failed.to_json()
    );
    assert!(
        failed
            .timings
            .as_ref()
            .unwrap()
            .spans
            .iter()
            .any(|span| span.name == "link" && span.outcome == "error")
    );
    assert!(!names(&failed).contains(&"publication"));
    assert!(!names(&failed).contains(&"cache_store"));
    assert!(!options.output.unwrap().exists());
    assert_eq!(fs::read(&hit.artifacts["executable"]).unwrap(), executable);
    closed_tree(&failed);
}
