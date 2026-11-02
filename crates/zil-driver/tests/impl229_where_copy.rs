//! Real source controls for finite where Copy, ordinary checks and shared cost.
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, ProjectSources, Stage};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn output(label: &str) -> PathBuf {
    let directory = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "impl229-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "where229.zil", text)
}

fn check(label: &str, text: &str) -> CompileReport {
    let directory = output(label);
    fs::write(directory.join("source.zil"), text).unwrap();
    let report = CompilationSession::default().compile_source(
        &source(text),
        &CompileOptions {
            emit_dir: Some(directory.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(directory.join("report.json"), report.to_json()).unwrap();
    report
}

const PLAIN: &str = "struct Pair{left:i32,right:i32}fn keep<T:Copy>(x:T)->T=>x;fn main()->i32{let x=keep(7);let y=keep(Pair{left:3,right:5});return x+y.left+y.right-15}";
const ALIAS: &str = "type Number=i32;struct Pair{left:Number,right:Number}fn keep<T:Copy>(x:T)->T=>x;fn main()->i32{let x:Number=keep(7);let y=keep(Pair{left:3,right:5});return x+y.left+y.right-15}";
const WHERE: &str = "type Number=i32;struct Pair{left:Number,right:Number}fn keep<T>(x:T)->T where T:Copy=>x;fn main()->i32{let x:Number=keep(7);let y=keep(Pair{left:3,right:5});return x+y.left+y.right-15}";

fn minimum(text: &str, nodes: bool, stage: Stage) -> usize {
    let defaults = Limits::default();
    let (mut low, mut high) = (
        0,
        if nodes {
            defaults.max_nodes
        } else {
            defaults.max_analysis_steps
        },
    );
    while high - low > 1 {
        let n = (low + high) / 2;
        let mut options = CompileOptions {
            stop_after: stage,
            ..CompileOptions::default()
        };
        if nodes {
            options.limits.max_nodes = n;
        } else {
            options.limits.max_analysis_steps = n;
        }
        let report = CompilationSession::default().compile_source(&source(text), &options);
        match report.status {
            ResultCategory::Success => high = n,
            ResultCategory::ResourceLimit => low = n,
            _ => panic!("{}", report.to_json()),
        }
    }
    high
}

#[test]
fn old_source_budget_controls() {
    let directory = output("old-costs");
    let mut rows = String::new();
    for (label, text) in [("plain", PLAIN), ("alias", ALIAS)] {
        let work = minimum(text, false, Stage::Core);
        let nodes = minimum(text, true, Stage::Core);
        rows.push_str(&format!("{label},{work},{nodes}\n"));
        // Measured before 229 on the same sources and shared name-only base.
        assert_eq!(
            (work, nodes),
            if label == "plain" {
                (589, 220)
            } else {
                (952, 233)
            }
        );
    }
    fs::write(directory.join("costs.csv"), &rows).unwrap();
    println!("{}\n{rows}", directory.display());
}

const ACCEPTED: &[(&str, &str)] = &[
    // 231: the exact former unknown-bound source now has an ordinary nominal
    // obligation. The declaration remains unused, so no actual impl is needed.
    (
        "marker-bound-promoted-231",
        "trait Marker{}fn keep<T>(x:T)->T where T:Copy+Marker=>x;fn main()->i32=>0",
    ),
    ("alias-and-record", WHERE),
    (
        "tuple-copy",
        "fn duplicate<T>(x:T)->(T,T) where T:Copy=>(x,x);fn main()->i32{let p=duplicate((7,true));return p.0.0+p.1.0-14}",
    ),
    (
        "two-independent-binders",
        "fn pair<A,B>(a:A,b:B)->(A,B) where B:Copy,A:Copy=>(a,b);fn main()->i32{let p=pair(9,true);if p.1{return p.0-9}return 1}",
    ),
    (
        "idempotent-bounds",
        "fn keep<T:Copy+Copy>(x:T)->T where T:Copy+Copy,T:Copy=>x;fn main()->i32=>keep(8)-8",
    ),
    (
        "symbolic-helper",
        "fn duplicate<T>(x:T)->(T,T) where T:Copy=>(x,x);fn forward<U>(x:U)->U where U:Copy=>duplicate(x).0;fn main()->i32=>forward(6)-6",
    ),
    (
        "value-namespace",
        "fn keep<T>(T:T)->T where T:Copy=>T;fn main()->i32=>keep(5)-5",
    ),
    (
        "unit-copy",
        "fn keep<T>(x:T)->T where T:Copy=>x;fn main(){keep(())}",
    ),
    (
        "unused-valid",
        "fn duplicate<T>(x:T)->(T,T) where T:Copy=>(x,x);fn main()->i32=>0",
    ),
    (
        "ct-call",
        "fn keep<T>(x:T)->T where T:Copy=>x;fn main()->i32{let n=#(keep(11));return n-11}",
    ),
];

#[test]
fn ordinary_where_sources_reach_the_independent_verifier() {
    for (label, text) in ACCEPTED {
        let report = check(label, text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1, "{label}: {}", report.to_json());
    }
}

#[test]
fn unsupported_clauses_are_not_erased_and_unused_bodies_are_checked() {
    let cases = [
        (
            "unknown-subject",
            "fn keep<T>(x:T)->T where U:Copy=>x;fn main()->i32=>0",
            "declared by this function",
        ),
        (
            "nongeneric-subject",
            "fn keep(x:i32)->i32 where i32:Copy=>x;fn main()->i32=>0",
            "declared by this function",
        ),
        (
            "alias-subject",
            "type Alias=i32;fn keep<T>(x:T)->T where Alias:Copy=>x;fn main()->i32=>0",
            "declared by this function",
        ),
        (
            "tuple-subject",
            "fn keep<T>(x:T)->T where (T,T):Copy=>x;fn main()->i32=>0",
            "direct type parameter",
        ),
        (
            "grouped-subject",
            "fn keep<T>(x:T)->T where (T):Copy=>x;fn main()->i32=>0",
            "direct type parameter",
        ),
        (
            "applied-subject",
            "struct Box<T>{value:T}fn keep<T>(x:T)->T where Box<T>:Copy=>x;fn main()->i32=>0",
            "applied type",
        ),
        (
            "projected-subject",
            "fn keep<T>(x:T)->T where T.Item:Copy=>x;fn main()->i32=>0",
            "qualified or projected",
        ),
        (
            "unknown-bound",
            "trait Marker{}fn keep<T>(x:T)->T where T:Copy+Missing=>x;fn main()->i32=>0",
            "direct built-in Copy",
        ),
        (
            "alias-bound",
            "type Alias=i32;fn keep<T>(x:T)->T where T:Alias=>x;fn main()->i32=>0",
            "direct built-in Copy",
        ),
        (
            "grouped-bound",
            "fn keep<T>(x:T)->T where T:(Copy)=>x;fn main()->i32=>0",
            "direct built-in Copy",
        ),
        (
            "applied-bound",
            "fn keep<T>(x:T)->T where T:Copy<i32> =>x;fn main()->i32=>0",
            "direct built-in Copy",
        ),
        (
            "inline-store-bound",
            "fn keep<T:StoreElement>(x:T)->T where T:Copy=>x;fn main()->i32=>0",
            "cannot mix",
        ),
        (
            "inline-callable-bound",
            "fn keep<T,F:Fn()->i32>(f:&F,x:T)->T where T:Copy=>x;fn main()->i32=>0",
            "cannot mix",
        ),
        (
            "const-binder",
            "fn keep<T,#N:i32>(x:T)->T where T:Copy=>x;fn main()->i32=>0",
            "ordinary type parameters",
        ),
        (
            "lifetime-binder",
            "fn keep<T,life a>(x:T)->T where T:Copy=>x;fn main()->i32=>0",
            "ordinary type parameters",
        ),
        (
            "lifetime-clause",
            "fn keep<T>(x:T)->T where 'a:'b=>x;fn main()->i32=>0",
            "lifetime constraints",
        ),
        (
            "missing-function-binder",
            "fn a<T>(x:T)->T where T:Copy=>x;fn b<U>(x:U)->U where T:Copy=>x;fn main()->i32=>0",
            "declared by this function",
        ),
    ];
    for (label, text, message) in cases {
        let report = check(label, text);
        assert!(
            matches!(
                report.status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.message.contains(message)),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
    for (label, text) in [
        (
            "unused-arithmetic",
            "fn bad<T>(x:T)->T where T:Copy=>x+x;fn main()->i32=>0",
        ),
        (
            "unused-field",
            "fn bad<T>(x:T)->i32 where T:Copy=>x.code;fn main()->i32=>0",
        ),
        (
            "unused-call",
            "fn bad<T>(x:T)->i32 where T:Copy=>x();fn main()->i32=>0",
        ),
        (
            "actual-managed",
            "fn keep<T>(x:T)->T where T:Copy=>x;fn main()->i32{let b=bytes_new();let c=keep(b);return 0}",
        ),
        (
            "actual-address",
            "fn keep<T>(x:T)->T where T:Copy=>x;fn main()->i32{let n=1;let p=keep(&n);return *p-1}",
        ),
    ] {
        let report = check(label, text);
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
}

#[test]
fn where_aliases_and_specialization_share_exact_fresh_and_warm_limits() {
    let directory = output("where-costs");
    let work = minimum(WHERE, false, Stage::Verify);
    let nodes = minimum(WHERE, true, Stage::Verify);
    assert_eq!(
        (work, nodes),
        (1651, 488),
        "measured where + alias + specialization work on one budget"
    );
    fs::write(
        directory.join("costs.csv"),
        format!("where,{work},{nodes}\n"),
    )
    .unwrap();
    for (node_budget, exact) in [(false, work), (true, nodes)] {
        let mut session = CompilationSession::with_capacity(128);
        for (index, n) in [exact, exact, exact - 1, exact].into_iter().enumerate() {
            let mut options = CompileOptions::default();
            if node_budget {
                options.limits.max_nodes = n;
            } else {
                options.limits.max_analysis_steps = n;
            }
            let report = session.compile_source(&source(WHERE), &options);
            fs::write(
                directory.join(format!("{node_budget}-{index}.json")),
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
            if index != 2 {
                assert_eq!(report.verification_runs, 1);
            } else {
                assert_eq!(report.phase, Phase::Expand);
                assert_eq!(report.verification_runs, 0);
                assert!(!report.artifacts.contains_key("executable"));
            }
            if index == 1 || index == 3 {
                assert!(report.cache_hit, "{}", report.to_json());
            }
        }
    }
}

fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(i, (name, text))| {
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
            .collect(),
    }
}

const PROJECT: &[(&str, &str)] = &[
    (
        "main",
        include_str!("../../../examples/mainline/where_copy229/main.zil"),
    ),
    (
        "helper",
        include_str!("../../../examples/mainline/where_copy229/helper.zil"),
    ),
    (
        "model",
        include_str!("../../../examples/mainline/where_copy229/model.zil"),
    ),
];

#[test]
fn project_path_uses_the_same_where_checks() {
    let report = zil_driver::compile_project(&project(PROJECT), &CompileOptions::default());
    fs::write(output("project").join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    let invalid = project(&[
        ("main", "import helper\nfn main()->i32=>0"),
        ("helper", "pub fn bad<T>(x:T)->T where T:Copy=>x+x;"),
    ]);
    let report = zil_driver::compile_project(&invalid, &CompileOptions::default());
    fs::write(
        output("project-unused-invalid").join("report.json"),
        report.to_json(),
    )
    .unwrap();
    let inline = project(&[
        ("main", "import helper\nfn main()->i32=>0"),
        ("helper", "pub fn bad<T:Copy>(x:T)->T=>x+x;"),
    ]);
    let control = zil_driver::compile_project(&inline, &CompileOptions::default());
    fs::write(
        output("project-unused-inline").join("report.json"),
        control.to_json(),
    )
    .unwrap();
    assert_eq!(
        control.status,
        ResultCategory::Unsupported,
        "{}",
        control.to_json()
    );
    assert_eq!(report.status, control.status, "{}", report.to_json());
    assert_eq!(
        report.diagnostics[0].message,
        control.diagnostics[0].message
    );
    assert_eq!(report.phase, Phase::Expand);
    assert_eq!(report.verification_runs, 0);
}

#[cfg(feature = "llvm-api")]
#[test]
fn inline_where_duplicate_and_project_programs_execute_at_o0_and_o2() {
    use std::process::Command;
    use zil_driver::{NativeBackend, Optimization};
    let duplicate = WHERE.replace("where T:Copy", "where T:Copy+Copy,T:Copy");
    for optimization in [Optimization::O0, Optimization::O2] {
        let mut reference_llvm = None;
        for (label, files) in [
            ("inline", vec![("main", ALIAS)]),
            ("where", vec![("main", WHERE)]),
            ("duplicates", vec![("main", duplicate.as_str())]),
            ("ct", vec![("main", ACCEPTED[8].1)]),
            ("project", PROJECT.to_vec()),
        ] {
            let directory = output(&format!("native-{label}-{optimization:?}"));
            for (name, text) in &files {
                fs::write(directory.join(format!("{name}.zil")), text).unwrap();
            }
            let report = zil_driver::compile_project(
                &project(&files),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend: NativeBackend::LlvmApi,
                    optimization,
                    output: Some(directory.join("program")),
                    emit_dir: Some(directory.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(directory.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{label}: {}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let result = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(directory.join("stdout.bin"), &result.stdout).unwrap();
            fs::write(directory.join("stderr.bin"), &result.stderr).unwrap();
            fs::write(
                directory.join("exit.txt"),
                format!("{:?}\n", result.status.code()),
            )
            .unwrap();
            assert_eq!(result.status.code(), Some(0), "{label}: {result:?}");
            assert!(result.stdout.is_empty(), "{label}: {result:?}");
            assert!(result.stderr.is_empty(), "{label}: {result:?}");
            if matches!(label, "inline" | "where" | "duplicates") {
                // Source spans legitimately change the verification fingerprint.
                // Preserve the full raw LLVM file and compare all actual IR.
                let llvm = fs::read_to_string(directory.join("phases/llvm.ll")).unwrap();
                let body = llvm
                    .lines()
                    .filter(|line| !line.starts_with("; verified-fingerprint: "))
                    .collect::<Vec<_>>()
                    .join("\n");
                if label == "inline" {
                    reference_llvm = Some(body);
                } else {
                    assert_eq!(
                        Some(body),
                        reference_llvm,
                        "{label}: LLVM representation must match the inline control"
                    );
                }
            }
        }
    }
}
