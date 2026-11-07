//! Non-author review of generated enums at production commit 2c8acce8f.
//! Sources and actual reports are retained even when an assertion fails.
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization, ProjectSources,
    Stage,
};

fn directory(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review230-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&path).unwrap();
    path
}

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text)
}

fn project(inputs: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: inputs
            .iter()
            .enumerate()
            .map(|(index, (name, text))| {
                SourceUnit::new(
                    FileId(index as u32),
                    ModuleId(index as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: inputs
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, (name, _))| (vec![(*name).into()], ModuleId(index as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn compile(
    label: &str,
    inputs: &[(&str, &str)],
    mut options: CompileOptions,
) -> (CompileReport, PathBuf) {
    let path = directory(label);
    for (name, text) in inputs {
        fs::write(path.join(format!("{name}.zil")), text).unwrap();
    }
    options.emit_dir = Some(path.join("phases"));
    if matches!(options.stop_after, Stage::Build) {
        options.output = Some(path.join("program"));
    }
    let inputs = project(inputs);
    let mut session = CompilationSession::default();
    let report = if inputs.modules.len() == 1 {
        session.compile_source(&inputs.modules[0], &options)
    } else {
        session.compile_project(&inputs, &options)
    };
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    assert_eq!(session.active_query_count(), 0);
    (report, path)
}

fn accepted(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

fn rejected(report: &CompileReport) {
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        report.to_json()
    );
    assert!(
        !matches!(report.phase, Phase::Lex | Phase::Parse),
        "a malformed fixture is not a semantic counterexample: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(!report.artifacts.contains_key("executable"));
}

const FORWARD: &str = r#"
#for(step in [3]) {
    type Word = Payload
    enum Signal { Empty, step(Word, [i32; step]) }
    struct Payload { value:i32 }
    pub fn run()->i32 {
        let value = Signal.step(Payload{value:37}, [1,2,3])
        match value {
            Signal.Empty()=>{return 1},
            Signal.step(step, values)=>{return step.value+values[0]+values[1]+values[2]-43}
        }
    }
}
fn main()->i32=>run()
"#;

const FORWARD_PLAIN: &str = r#"
type Word = Payload
enum Signal { Empty, step(Word, [i32; 3]) }
struct Payload { value:i32 }
pub fn run()->i32 {
    let value = Signal.step(Payload{value:37}, [1,2,3])
    match value {
        Signal.Empty()=>{return 1},
        Signal.step(step, values)=>{return step.value+values[0]+values[1]+values[2]-43}
    }
}
fn main()->i32=>run()
"#;

const MODULE_MAIN: &str =
    "import events as e\nfn main()->i32{let value=e.Choice.Data(33);return e.score(value)-42}";
const WIRE_MODULE: &str = "pub enum Message{Data(i32),Empty}pub fn score(value:Message)->i32{match value{Message.Data(x)=>{return x},Message.Empty()=>{return 0}}}";
const EVENTS_MODULE: &str = r#"
import wire
#for(width in [1]) {
    enum wire { Local(i32) }
    type Local = wire
    enum Message { Data(i32), Empty }
    pub type Choice = Message
    fn local_score(value:Local)->i32 {
        match value { Local.Local(x)=>{return x} }
    }
    pub fn score(value:Choice)->i32 {
        let foreign=wire.Message.Data(5)
        let remote=wire.score(foreign)
        match value {
            Choice.Data(x)=>{return x+local_score(Local.Local(4))+remote},
            Choice.Empty()=>{return 0}
        }
    }
}
"#;
// Alpha-expand the generated private enum to avoid an ordinary top-level
// declaration colliding with its import. The module qualifier stays `wire`.
const EVENTS_PLAIN: &str = r#"
import wire
enum LocalPacket { Local(i32) }
type Local = LocalPacket
enum Message { Data(i32), Empty }
pub type Choice = Message
fn local_score(value:Local)->i32 {
    match value { Local.Local(x)=>{return x} }
}
pub fn score(value:Choice)->i32 {
    let foreign=wire.Message.Data(5)
    let remote=wire.score(foreign)
    match value {
        Choice.Data(x)=>{return x+local_score(Local.Local(4))+remote},
        Choice.Empty()=>{return 0}
    }
}
"#;

#[test]
fn forward_payload_alias_variant_spelling_and_pattern_binder_are_independent() {
    for (label, text) in [("forward", FORWARD), ("forward-plain", FORWARD_PLAIN)] {
        accepted(&compile(label, &[("main", text)], CompileOptions::default()).0);
    }
}

#[test]
fn qualified_import_and_same_spelled_generated_enum_keep_separate_identities() {
    for (label, events) in [
        ("qualified", EVENTS_MODULE),
        ("qualified-plain", EVENTS_PLAIN),
    ] {
        accepted(
            &compile(
                label,
                &[
                    ("main", MODULE_MAIN),
                    ("events", events),
                    ("wire", WIRE_MODULE),
                ],
                CompileOptions::default(),
            )
            .0,
        );
    }
    let missing = compile(
        "qualified-missing-member",
        &[
            (
                "main",
                "import data\n#for(n in [1]){enum data{Wrap(data.Missing)}}fn main()->i32=>0",
            ),
            ("data", "pub enum Present{Empty}"),
        ],
        CompileOptions::default(),
    )
    .0;
    rejected(&missing);
    assert!(
        missing
            .diagnostics
            .iter()
            .any(|d| d.code == "E-MODULE-MEMBER")
    );
}

#[test]
fn transparent_aliases_do_not_merge_distinct_generation_rounds() {
    let generated = "#for(n in [1,2]){enum E{Data(i32)}type Selected=E;#if(n==1){pub fn make()->Selected=>Selected.Data(n)}else{pub fn accept(value:Selected)->i32{match value{Selected.Data(x)=>{return x}}}}}fn main()->i32=>accept(make())";
    let plain = "enum First{Data(i32)}type FirstAlias=First;enum Second{Data(i32)}type SecondAlias=Second;pub fn make()->FirstAlias=>FirstAlias.Data(1);pub fn accept(value:SecondAlias)->i32{match value{SecondAlias.Data(x)=>{return x}}}fn main()->i32=>accept(make())";
    for (label, text) in [("round-alias", generated), ("round-alias-plain", plain)] {
        let report = compile(label, &[("main", text)], CompileOptions::default()).0;
        rejected(&report);
        assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "E-TYPE-MISMATCH")
        );
    }
}

#[test]
fn same_layout_generated_public_enums_remain_nominal_between_modules() {
    let left = "#for(n in [4]){pub enum E{Value(i32)}pub fn make()->E=>E.Value(n);pub fn consume(value:E)->i32{match value{E.Value(x)=>{return x}}}}";
    let right = "#for(n in [4]){pub enum E{Value(i32)}pub fn consume(value:E)->i32{match value{E.Value(x)=>{return x}}}}";
    for (label, main, good) in [
        (
            "module-own-identity",
            "import left\nimport right\nfn main()->i32=>left.consume(left.make())-4",
            true,
        ),
        (
            "module-distinct-identity",
            "import left\nimport right\nfn main()->i32=>right.consume(left.make())-4",
            false,
        ),
    ] {
        let report = compile(
            label,
            &[("main", main), ("left", left), ("right", right)],
            CompileOptions::default(),
        )
        .0;
        if good {
            accepted(&report);
        } else {
            rejected(&report);
            assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.code == "E-TYPE-MISMATCH")
            );
        }
    }
}

#[test]
fn generic_type_binder_shadows_alias_and_does_not_leak_to_later_functions() {
    let good = "#for(n in [1]){enum E{A}type Choice=E;fn id<Choice:Copy>(value:Choice)->Choice=>value;fn later(value:Choice)->i32{match value{Choice.A()=>{return 0}}}pub fn run()->i32=>id::<i32>(later(Choice.A))}fn main()->i32=>run()";
    accepted(
        &compile(
            "generic-alias-shadow",
            &[("main", good)],
            CompileOptions::default(),
        )
        .0,
    );
    let bad = "#for(n in [1]){enum E{A}type Choice=E;fn bad<Choice:Copy>(value:Choice)->i32{match value{Choice.A()=>{return 0}}}}fn main()->i32=>0";
    rejected(
        &compile(
            "generic-alias-no-privilege",
            &[("main", bad)],
            CompileOptions::default(),
        )
        .0,
    );
}

#[test]
fn address_payloads_and_recursive_alias_payloads_keep_ordinary_rejections() {
    for (label, declarations) in [
        ("address", "enum E{Borrow(&i32)}"),
        ("recursive-alias", "type Back=E;enum E{Again(Back)}"),
    ] {
        for generated in [false, true] {
            let text = if generated {
                format!("#for(n in [1]){{{declarations}}}fn main()->i32=>0")
            } else {
                format!("{declarations}fn main()->i32=>0")
            };
            let report = compile(
                &format!("payload-{label}-{generated}"),
                &[("main", &text)],
                CompileOptions::default(),
            )
            .0;
            rejected(&report);
            assert!(matches!(report.phase, Phase::Expand | Phase::Types));
        }
    }
}

#[test]
fn payload_constant_substitution_rechecks_changed_source_in_a_warm_session() {
    let valid = "#for(width in [2]){enum E{A([i32;width])}pub fn run()->i32{let value=E.A([11,31]);match value{E.A(xs)=>{return xs[0]+xs[1]-42}}}}fn main()->i32=>run()";
    let invalid = valid.replace("[11,31]", "[11]");
    let mut session = CompilationSession::with_capacity(128);
    for (attempt, text) in [valid, valid, invalid.as_str(), valid]
        .into_iter()
        .enumerate()
    {
        let path = directory(&format!("payload-edit-{attempt}"));
        fs::write(path.join("main.zil"), text).unwrap();
        let report = session.compile_source(
            &source(text),
            &CompileOptions {
                emit_dir: Some(path.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(path.join("report.json"), report.to_json()).unwrap();
        if attempt == 2 {
            rejected(&report);
            assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|d| d.code == "E-ARRAY-LENGTH")
            );
        } else {
            accepted(&report);
        }
        if attempt == 1 || attempt == 3 {
            assert!(report.cache_hit, "{}", report.to_json());
        }
        assert_eq!(session.active_query_count(), 0);
    }
}

const BUDGET_SOURCE: &str = "#for(width in [1,2,3]){type Slot=Payload;enum Packet{One(Slot),Pair((bool,i32)),Pad([bool;width]),Empty}struct Payload{bytes:[i32;width]}}fn main()->i32=>0";

#[test]
fn variant_payload_visits_share_request_limits_with_cold_and_warm_replay() {
    for nodes in [false, true] {
        let path = directory(if nodes { "limit-nodes" } else { "limit-work" });
        fs::write(path.join("main.zil"), BUDGET_SOURCE).unwrap();
        let run = |allowance, session: &mut CompilationSession| {
            let mut limits = Limits::default();
            if nodes {
                limits.max_nodes = allowance;
            } else {
                limits.max_analysis_steps = allowance;
            }
            session.compile_source(
                &source(BUDGET_SOURCE),
                &CompileOptions {
                    limits,
                    ..CompileOptions::default()
                },
            )
        };
        let (mut low, mut high) = (0, if nodes { 100_000 } else { 1_000_000 });
        accepted(&run(high, &mut CompilationSession::default()));
        while high - low > 1 {
            let probe = (low + high) / 2;
            let report = run(probe, &mut CompilationSession::default());
            match report.status {
                ResultCategory::Success => high = probe,
                ResultCategory::ResourceLimit => low = probe,
                _ => panic!("{}", report.to_json()),
            }
        }
        fs::write(
            path.join("minimum.json"),
            format!("{{\"minimum\":{high},\"nodes\":{nodes}}}\n"),
        )
        .unwrap();
        let mut session = CompilationSession::with_capacity(128);
        for (attempt, allowance) in [high, high, high - 1, high].into_iter().enumerate() {
            let report = run(allowance, &mut session);
            fs::write(
                path.join(format!("attempt-{attempt}.json")),
                report.to_json(),
            )
            .unwrap();
            if attempt == 2 {
                assert_eq!(
                    report.status,
                    ResultCategory::ResourceLimit,
                    "{}",
                    report.to_json()
                );
                assert!(!report.artifacts.contains_key("executable"));
            } else {
                accepted(&report);
            }
            if attempt == 1 || attempt == 3 {
                assert!(report.cache_hit, "{}", report.to_json());
            }
            assert_eq!(session.active_query_count(), 0);
        }
        let mut fresh = CompilationSession::default();
        let report = run(high, &mut fresh);
        fs::write(path.join("fresh.json"), report.to_json()).unwrap();
        accepted(&report);
        assert_eq!(fresh.active_query_count(), 0);
    }
}

#[test]
fn ordinary_expansion_and_generated_sources_execute_at_o0_and_o2() {
    for optimization in [Optimization::O0, Optimization::O2] {
        for (label, inputs) in [
            ("forward", vec![("main", FORWARD)]),
            ("forward-plain", vec![("main", FORWARD_PLAIN)]),
            (
                "qualified",
                vec![
                    ("main", MODULE_MAIN),
                    ("events", EVENTS_MODULE),
                    ("wire", WIRE_MODULE),
                ],
            ),
            (
                "qualified-plain",
                vec![
                    ("main", MODULE_MAIN),
                    ("events", EVENTS_PLAIN),
                    ("wire", WIRE_MODULE),
                ],
            ),
        ] {
            let backend = if cfg!(feature = "llvm-api") {
                NativeBackend::LlvmApi
            } else {
                NativeBackend::Clang
            };
            let (report, path) = compile(
                &format!("native-{optimization:?}-{label}"),
                &inputs,
                CompileOptions {
                    stop_after: Stage::Build,
                    backend,
                    optimization,
                    ..CompileOptions::default()
                },
            );
            accepted(&report);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            let output = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(path.join("stdout.log"), &output.stdout).unwrap();
            fs::write(path.join("stderr.log"), &output.stderr).unwrap();
            fs::write(
                path.join("exit.txt"),
                format!("{:?}\n", output.status.code()),
            )
            .unwrap();
            assert_eq!(output.status.code(), Some(0), "{label}: {output:?}");
            assert!(output.stdout.is_empty(), "{label}: {output:?}");
            assert!(output.stderr.is_empty(), "{label}: {output:?}");
        }
    }
}
