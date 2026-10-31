//! Reviewer-found ordinary type binders must not borrow a nominal's members.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, Stage};

const INLINE: &str = "struct T{x:i32}fn keep<T:Copy>(x:T)->T=>x;fn main()->i32{let value=keep(T{x:7});return value.x-7}";
const WHERE: &str = "struct T{x:i32}fn keep<T>(x:T)->T where T:Copy=>x;fn main()->i32{let value=keep(T{x:7});return value.x-7}";

fn directory(label: &str) -> PathBuf {
    let directory = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("impl229-shadow-{label}-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "shadow229.zil", text)
}

#[test]
fn unused_symbolic_types_do_not_gain_nominal_members_or_constructors() {
    let mut accepted = Vec::new();
    for (label, text) in [
        (
            "original-inline",
            "struct T{x:i32}fn bad<T:Copy>(x:T)->i32=>x.x;fn main()->i32=>0",
        ),
        (
            "original-where",
            "struct T{x:i32}fn bad<T>(x:T)->i32 where T:Copy=>x.x;fn main()->i32=>0",
        ),
        (
            "unbounded-field",
            "struct T{x:i32}fn bad<T>(x:T)->i32=>x.x;fn main()->i32=>0",
        ),
        (
            "address-field",
            "struct T{x:i32}fn bad<T:Copy>(x:&T)->i32=>x.x;fn main()->i32=>0",
        ),
        (
            "record-construction",
            "struct T{x:i32}fn bad<T:Copy>()->T=>T{x:1};fn main()->i32=>0",
        ),
        (
            "enum-construction",
            "enum T{A,B}fn bad<T:Copy>()->T=>T.A;fn main()->i32=>0",
        ),
    ] {
        let out = directory(label);
        fs::write(out.join("source.zil"), text).unwrap();
        let report = CompilationSession::default().compile_source(
            &source(text),
            &CompileOptions {
                emit_dir: Some(out.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("report.json"), report.to_json()).unwrap();
        if report.status == ResultCategory::Success {
            accepted.push(label);
            continue;
        }
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
        assert_eq!(report.verification_runs, 0);
    }
    assert!(
        accepted.is_empty(),
        "unsound unused definitions accepted: {accepted:?}"
    );
}

fn minimum(text: &str, nodes: bool) -> usize {
    let limits = Limits::default();
    let (mut low, mut high) = (
        0,
        if nodes {
            limits.max_nodes
        } else {
            limits.max_analysis_steps
        },
    );
    while high - low > 1 {
        let middle = (low + high) / 2;
        let mut options = CompileOptions {
            stop_after: Stage::Core,
            ..CompileOptions::default()
        };
        if nodes {
            options.limits.max_nodes = middle;
        } else {
            options.limits.max_analysis_steps = middle;
        }
        let report = CompilationSession::default().compile_source(&source(text), &options);
        match report.status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle,
            _ => panic!("{}", report.to_json()),
        }
    }
    high
}

#[test]
fn real_collision_paths_have_measured_fresh_and_exhausted_boundaries() {
    for (label, text) in [("inline", INLINE), ("where", WHERE)] {
        let out = directory(&format!("budget-{label}"));
        fs::write(out.join("source.zil"), text).unwrap();
        let work = minimum(text, false);
        let nodes = minimum(text, true);
        // Original accepted collision path: inline 281/131, where 380/173.
        // Scoped nominal protection is now paid before symbolic validation.
        assert_eq!(
            (work, nodes),
            if label == "inline" {
                (837, 188)
            } else {
                (936, 230)
            }
        );
        fs::write(out.join("costs.csv"), format!("{label},{work},{nodes}\n")).unwrap();
        eprintln!("{label} work={work} nodes={nodes}");
        for (is_nodes, exact) in [(false, work), (true, nodes)] {
            let mut session = CompilationSession::with_capacity(128);
            for (index, limit) in [exact, exact, exact - 1, exact].into_iter().enumerate() {
                let mut options = CompileOptions {
                    stop_after: Stage::Core,
                    ..CompileOptions::default()
                };
                if is_nodes {
                    options.limits.max_nodes = limit;
                } else {
                    options.limits.max_analysis_steps = limit;
                }
                let report = session.compile_source(&source(text), &options);
                fs::write(
                    out.join(format!("{is_nodes}-{index}.json")),
                    report.to_json(),
                )
                .unwrap();
                assert_eq!(
                    report.status,
                    if index == 2 {
                        ResultCategory::ResourceLimit
                    } else {
                        ResultCategory::Success
                    },
                    "{}",
                    report.to_json()
                );
                assert_eq!(session.active_query_count(), 0);
                if index == 2 {
                    assert_eq!(report.phase, Phase::Expand);
                } else if index > 0 {
                    assert!(report.cache_hit);
                }
            }
        }
    }
}

#[cfg(feature = "llvm-api")]
#[test]
fn actual_nominal_and_symbolic_identity_still_execute() {
    use std::process::Command;
    use zil_driver::{NativeBackend, Optimization};
    for optimization in [Optimization::O0, Optimization::O2] {
        for (label, text) in [("inline", INLINE), ("where", WHERE)] {
            let out = directory(&format!("native-{label}-{optimization:?}"));
            fs::write(out.join("source.zil"), text).unwrap();
            let report = CompilationSession::default().compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend: NativeBackend::LlvmApi,
                    optimization,
                    output: Some(out.join("program")),
                    emit_dir: Some(out.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(out.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(out.join("stdout.bin"), &output.stdout).unwrap();
            fs::write(out.join("stderr.bin"), &output.stderr).unwrap();
            fs::write(
                out.join("exit.txt"),
                format!("{:?}\n", output.status.code()),
            )
            .unwrap();
            assert_eq!(output.status.code(), Some(0));
            assert!(output.stdout.is_empty());
            assert!(output.stderr.is_empty());
        }
    }
}
