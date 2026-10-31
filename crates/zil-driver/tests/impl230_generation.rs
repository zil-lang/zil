//! Generated enums keep the same nominal and Copy rules as handwritten enums.
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization, ProjectSources,
    Stage,
};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "generated-enums230.zil", text)
}
fn directory(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("enums230-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&path).unwrap();
    path
}
fn check(label: &str, text: &str) -> CompileReport {
    let path = directory(label);
    fs::write(path.join("source.zil"), text).unwrap();
    let report = CompilationSession::default().compile_source(
        &source(text),
        &CompileOptions {
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    report
}
const EVENTS: &str = include_str!("../../../examples/mainline/ct_enums230/events.zil");
const PLAIN: &str = include_str!("../../../examples/mainline/ct_enums230/events_plain.zil");
const WIDTHS: &str = "#for(n in [1,2]){enum Event{Empty,Data([i32;n])}#if(n==1){pub fn one()->i32{let e=Event.Data([3]);match e{Event.Empty()=>{return 0},Event.Data(x)=>{return x[0]}}}}else{pub fn two()->i32{let e=Event.Data([4,5]);match e{Event.Empty()=>{return 0},Event.Data(x)=>{return x[0]+x[1]}}}}}fn main()->i32=>one()+two()-12";
const NESTED: &str = "#for(n in [3]){enum E{A(i32)}#for(m in [4]){enum E{B(bool)}pub fn inner()->i32{let e=E.B(true);match e{E.B(true)=>{return m},E.B(false)=>{return 0}}}}pub fn outer()->i32{let e=E.A(n);match e{E.A(v)=>{return v}}}}fn main()->i32=>inner()+outer()-7";

#[test]
fn enum_generation_types_constructors_patterns_and_shadows_reach_verify() {
    for (label, text) in [
        ("events", EVENTS),
        ("events-plain", PLAIN),
        ("widths", WIDTHS),
        ("nested", NESTED),
        ("unit", "#for(n in [1]){enum E{A}}fn main()->i32=>0"),
        (
            "public",
            "#for(n in [1]){pub enum E{A,B(i32)}}fn main()->i32{let e=E.B(4);match e{E.A()=>{return 1},E.B(x)=>{return x-4}}}",
        ),
        (
            "generic-shadow",
            "#for(n in [1]){enum T{A}fn id<T:Copy>(x:T)->T=>x;pub fn run()->i32{if id::<bool>(true){return 0}return 1}}fn main()->i32=>run()",
        ),
        (
            "value-pattern-shadow",
            "#for(n in [7]){enum E{A(i32),B}pub fn run()->i32{let e=E.A(n);let E=1;match e{E.A(x)=>{return x+E-8},E.B()=>{return 1}}}}fn main()->i32=>run()",
        ),
        (
            "type-iterator-name",
            "#for(n in [3]){enum n{A(i32)}type Event=n;pub fn run()->i32{let e=Event.A(n);match e{Event.A(x)=>{return x-3}}}}fn main()->i32=>run()",
        ),
        (
            "empty",
            "#for(n in []){enum E{A(Missing)}}fn main()->i32=>0",
        ),
        (
            "zero-tuple",
            "#for(n in [1]){enum E{A(),B(())}pub fn run()->i32{let e=E.B(());match e{E.A()=>{return 1},E.B(x)=>{x;return 0}}}}fn main()->i32=>run()",
        ),
    ] {
        let report = check(label, text);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn nominal_separation_names_and_ordinary_payload_rejections_survive() {
    for (label, text) in [
        (
            "private-escape",
            "#for(n in [1]){enum E{A}}fn main()->i32{let e=E.A;return 0}",
        ),
        (
            "duplicate-private",
            "#for(n in [1]){enum E{A}enum E{B}}fn main()->i32=>0",
        ),
        (
            "duplicate-cross-kind",
            "#for(n in [1]){enum E{A}type E=i32}fn main()->i32=>0",
        ),
        (
            "duplicate-public",
            "#for(n in [1,2]){pub enum E{A}}fn main()->i32=>0",
        ),
        (
            "nominal-iteration",
            "#for(n in [1,2]){enum E{A(i32)}#if(n==1){pub fn make()->E=>E.A(n)}else{pub fn accept(e:E)->i32{match e{E.A(x)=>{return x}}}}}fn main()->i32=>accept(make())",
        ),
        (
            "duplicate-variant",
            "#for(n in [1]){enum E{A,A}}fn main()->i32=>0",
        ),
        ("recursive", "#for(n in [1]){enum E{A(E)}}fn main()->i32=>0"),
        (
            "unknown-payload",
            "#for(n in [1]){enum E{A(Missing)}}fn main()->i32=>0",
        ),
        (
            "variant-attribute",
            "#for(n in [1]){enum E{#[missing(n)]A}}fn main()->i32=>0",
        ),
        (
            "declaration-attribute",
            "#for(n in [1]){#[missing(n)]enum E{A}}fn main()->i32=>0",
        ),
        (
            "nonexhaustive",
            "#for(n in [1]){enum E{A,B}pub fn run(e:E)->i32{match e{E.A()=>{return 0}}}}fn main()->i32=>0",
        ),
        (
            "generic-no-privilege",
            "#for(n in [1]){enum T{A}fn bad<T:Copy>(x:T)->i32{match x{T.A()=>{return 0}}}}fn main()->i32=>0",
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
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
    for name in ["i32", "ByteBuffer", "Store", "Copy"] {
        // Retain the handwritten diagnostic as a baseline observation. The
        // pre-existing ordinary enum i32 path accepts its unused declaration;
        // generated names still obey the explicit 228/230 reserved-name rule.
        check(
            &format!("reserved-plain-{name}"),
            &format!("enum {name}{{A}}fn main()->i32=>0"),
        );
        let generated = check(
            &format!("reserved-generated-{name}"),
            &format!("#for(n in [1]){{enum {name}{{A}}}}fn main()->i32=>0"),
        );
        assert_eq!(
            generated.status,
            ResultCategory::SafeReject,
            "{name}: {}",
            generated.to_json()
        );
        assert_eq!(
            generated.phase,
            Phase::Expand,
            "{name}: {}",
            generated.to_json()
        );
        assert_eq!(generated.verification_runs, 0);
    }
}

#[test]
fn new_enum_profile_keeps_generation_and_configready_boundaries() {
    for (label, declaration) in [
        ("generic", "enum E<T>{A(T)}"),
        ("record", "enum E{A{x:i32}}"),
        ("owned", "enum E{A(ByteBuffer)}"),
        ("resource", "resource R{x:i32}"),
        ("package", "package P{x:i32}"),
        ("trait", "trait T{fn value(self:Self)->i32;}"),
    ] {
        let report = check(
            label,
            &format!("#for(n in [1]){{{declaration}}}fn main()->i32=>0"),
        );
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
    let report = check(
        "configready",
        "#for(n in [1]){pub enum E{A}}#if(E){fn selected()->i32=>0}fn main()->i32=>0",
    );
    assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-CT-PHASE-DEPENDENCY")
    );
}

#[test]
fn public_generated_enum_crosses_real_module_boundaries() {
    let path = directory("project");
    let main = "import events\nfn main()->i32{let e=events.Event.Value(7);return events.read(e)-7}";
    let events = "#for(n in [1]){pub enum Event{Empty,Value(i32)}pub fn read(e:Event)->i32{match e{Event.Empty()=>{return 0},Event.Value(x)=>{return x}}}}";
    fs::write(path.join("main.zil"), main).unwrap();
    fs::write(path.join("events.zil"), events).unwrap();
    let report = CompilationSession::default().compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![
                SourceUnit::new(FileId(0), ModuleId(0), "main.zil", main),
                SourceUnit::new(FileId(1), ModuleId(1), "events.zil", events),
            ],
            paths: BTreeMap::from([(vec!["events".into()], ModuleId(1))]),
        },
        &CompileOptions {
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

#[test]
fn enum_generation_limits_charge_every_iteration_and_warm_replay() {
    for nodes in [true, false] {
        let (mut lo, mut hi) = (0, if nodes { 100_000 } else { 1_000_000 });
        let run = |n, session: &mut CompilationSession| {
            let mut limits = Limits::default();
            if nodes {
                limits.max_nodes = n;
            } else {
                limits.max_analysis_steps = n;
            }
            session.compile_source(
                &source(WIDTHS),
                &CompileOptions {
                    limits,
                    ..CompileOptions::default()
                },
            )
        };
        assert_eq!(
            run(hi, &mut CompilationSession::default()).status,
            ResultCategory::Success
        );
        while hi - lo > 1 {
            let n = (hi + lo) / 2;
            let report = run(n, &mut CompilationSession::default());
            if report.status == ResultCategory::Success {
                hi = n;
            } else {
                assert_eq!(
                    report.status,
                    ResultCategory::ResourceLimit,
                    "{}",
                    report.to_json()
                );
                lo = n;
            }
        }
        let path = directory(if nodes { "budget-nodes" } else { "budget-work" });
        fs::write(path.join("source.zil"), WIDTHS).unwrap();
        fs::write(path.join("minimum.json"), format!("{{\"minimum\":{hi}}}\n")).unwrap();
        let mut session = CompilationSession::with_capacity(128);
        for (attempt, n) in [hi, hi, hi - 1, hi].into_iter().enumerate() {
            let report = run(n, &mut session);
            fs::write(
                path.join(format!("attempt-{attempt}.json")),
                report.to_json(),
            )
            .unwrap();
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
        let fresh = run(hi, &mut CompilationSession::default());
        fs::write(path.join("fresh.json"), fresh.to_json()).unwrap();
        assert_eq!(fresh.status, ResultCategory::Success, "{}", fresh.to_json());
    }
}

#[test]
fn generated_enums_and_handwritten_events_execute_at_o0_and_o2() {
    for optimization in [Optimization::O0, Optimization::O2] {
        for (label, text) in [
            ("events", EVENTS),
            ("plain", PLAIN),
            ("widths", WIDTHS),
            ("nested", NESTED),
        ] {
            let path = directory(&format!("native-{optimization:?}-{label}"));
            let executable = path.join("program");
            fs::write(path.join("source.zil"), text).unwrap();
            let backend = if cfg!(feature = "llvm-api") {
                NativeBackend::LlvmApi
            } else {
                NativeBackend::Clang
            };
            let report = CompilationSession::default().compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    output: Some(executable.clone()),
                    emit_dir: Some(path.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(path.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{label}: {}",
                report.to_json()
            );
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let result = Command::new(&executable).output().unwrap();
            fs::write(path.join("stdout.log"), &result.stdout).unwrap();
            fs::write(path.join("stderr.log"), &result.stderr).unwrap();
            fs::write(
                path.join("exit.txt"),
                format!("{:?}\n", result.status.code()),
            )
            .unwrap();
            assert_eq!(
                result.status.code(),
                Some(0),
                "{optimization:?}/{label}: {result:?}"
            );
            assert!(
                result.stdout.is_empty() && result.stderr.is_empty(),
                "{result:?}"
            );
        }
    }
}
