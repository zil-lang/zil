//! Independent generated-type scope, nominal and machine controls.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, ProjectSources, Stage};

fn directory(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review228-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&path).unwrap();
    path
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review228.zil", text)
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

const NESTED: &str = "#for(n in [4]){type Outer=Payload;struct Payload{x:i32}#for(m in [5]){type Return=Outer;pub fn make()->Return=>Return{x:n+m}}pub fn read(x:Outer)->i32=>x.x}fn main()->i32=>read(make())-9";
const ACCEPTED: &[(&str, &str)] = &[
    ("nested-outer-forward-alias", NESTED),
    (
        "value-parameter-and-constructor",
        "#for(n in [3]){struct Packet{item:i32}type Alias=Packet;fn make(Alias:i32)->Alias=>Alias{item:Alias};pub fn run()->i32{let Packet=make(n);return Packet.item-3}}fn main()->i32=>run()",
    ),
    (
        "pattern-type-value-shadow",
        "enum E{A(i32),B}#for(n in [7]){type Choice=E;pub fn run()->i32{let value=Choice.A(n);let Choice=1;match value{Choice.A(x)=>{return x+Choice-8},Choice.B()=>{return 1}}}}fn main()->i32=>run()",
    ),
    (
        "function-alias-types",
        "#for(n in [2]){type Cell=A;struct A{v:i32}type Callback=fn(Cell)->Cell;fn step(x:Cell)->Cell=>Cell{v:x.v+n};pub fn run()->i32{let f:Callback=step;return f(Cell{v:4}).v-6}}fn main()->i32=>run()",
    ),
    (
        "generic-versus-nominal",
        "#for(n in [8]){struct T{v:i32}fn id<T:Copy>(x:T)->T=>x;pub fn run()->i32{let value=id(T{v:n});return value.v-8}}fn main()->i32=>run()",
    ),
    (
        "selection-per-iteration-shape",
        "#for(n in [1,2]){struct Event{value:[i32;n]}#if(n==1){pub fn one()->i32{let p=Event{value:[4]};return p.value[0]}}else{pub fn two()->i32{let p=Event{value:[4,5]};return p.value[0]+p.value[1]}}}fn main()->i32=>one()+two()-13",
    ),
    (
        "ordinary-function-names-remain-allowed",
        "#for(n in [3]){type Alias=i32;fn Copy()->i32=>n;fn bool()->Alias=>Copy();pub fn run()->Alias=>bool()-3}fn main()->i32=>run()",
    ),
];

#[test]
fn independent_nested_and_namespace_sources_reach_verify() {
    let mut failed = Vec::new();
    for (label, text) in ACCEPTED {
        let report = check(label, text);
        if report.status != ResultCategory::Success {
            failed.push(format!("{label}: {}", report.to_json()));
            continue;
        }
        assert_eq!(report.verification_runs, 1);
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn private_identity_scope_and_ordinary_capability_rejections_survive() {
    let cases = [
        (
            "same-spelling-type-value-declarations",
            "#for(n in [1]){type Item=i32;fn Item()->i32=>n}fn main()->i32=>0",
        ),
        (
            "existing-public-duplicate",
            "struct Item{x:i32}#for(n in [1]){pub type Item=i32}fn main()->i32=>0",
        ),
        (
            "nominal-alias-across-iterations",
            "#for(n in [1,2]){struct Payload{x:i32}type Wrap=Payload;#if(n==1){pub fn make()->Wrap=>Wrap{x:n}}else{pub fn read(x:Wrap)->i32=>x.x}}fn main()->i32=>read(make())",
        ),
        (
            "nested-private-not-visible",
            "#for(n in [1]){#for(m in [2]){type Private=i32}pub fn run()->Private=>n}fn main()->i32=>run()",
        ),
        (
            "generated-nominal-cannot-authorize-generic-field",
            "#for(n in [1]){struct T{x:i32}fn bad<T:Copy>(x:T)->i32=>x.x}fn main()->i32=>0",
        ),
        (
            "generated-alias-cannot-authorize-generic-call",
            "#for(n in [1]){type F=fn()->i32;fn bad<F>(f:F)->i32=>f()}fn main()->i32=>0",
        ),
        (
            "alias-recursive-layout",
            "#for(n in [1]){type Link=Node;struct Node{next:Link}}fn main()->i32=>0",
        ),
        (
            "hidden-invalid-array",
            "#for(n in [1]){type Bad=[Missing;n]}fn main()->i32=>0",
        ),
        (
            "borrow-is-not-copy",
            "#for(n in [1]){struct Ref{value:&i32}fn copy<T:Copy>(x:T)->(T,T)=>(x,x);pub fn run()->i32{let n=1;let p=copy(Ref{value:&n});return *p.0.value}}fn main()->i32=>run()",
        ),
        (
            "reserved-operation-alias",
            "#for(n in [1]){type bytes_len=bool}fn main()->i32=>0",
        ),
        (
            "reserved-operation-function",
            "#for(n in [1]){struct S{x:i32}fn bytes_len()->i32=>n;pub fn run()->i32=>bytes_len()}fn main()->i32=>run()-1",
        ),
        (
            "reserved-copy-alias",
            "#for(n in [1]){type Copy=i32}fn main()->i32=>0",
        ),
        (
            "reserved-scalar-struct",
            "#for(n in [1]){struct i32{x:bool}}fn main()->i32=>0",
        ),
        (
            "const-generic-remains-closed",
            "#for(n in [1]){type T=i32;fn value<T,#N:i32>(x:T)->T=>x}fn main()->i32=>0",
        ),
        (
            "generated-layout-not-configready",
            "fn tag()->i32{let p=Model{x:0};return p.x}#for(n in [1]){pub struct Model{x:i32}}#if(tag()==0){fn yes()->i32=>0}fn main()->i32=>0",
        ),
        (
            "generated-import-not-configready",
            "#for(n in [1]){type A=i32;import hidden}fn main()->i32=>0",
        ),
        (
            "bounded-alias-remains-closed",
            "#for(n in [1]){type A<T:Copy> = T}fn main()->i32=>0",
        ),
    ];
    let mut accepted = Vec::new();
    for (label, text) in cases {
        let report = check(label, text);
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
        assert!(
            !matches!(report.phase, Phase::Lex | Phase::Parse),
            "test must reach the semantic rejection: {label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0, "{label}: {}", report.to_json());
        for diagnostic in &report.diagnostics {
            if let Some(span) = diagnostic.span {
                assert!(span.is_valid_for(&source(text)));
            }
        }
        assert!(!report.artifacts.contains_key("executable"));
    }
    assert!(accepted.is_empty(), "unexpectedly accepted {accepted:?}");
}

#[test]
fn function_reserved_names_have_plain_and_old_generation_controls() {
    let mut accepted = Vec::new();
    for (label, text) in [
        (
            "plain-reserved-function",
            "struct S{x:i32}fn bytes_len()->i32=>1;fn main()->i32=>bytes_len()-1",
        ),
        (
            "old-pure-generation-reserved-function",
            "#for(n in [1]){fn bytes_len()->i32=>n;pub fn run()->i32=>bytes_len()}fn main()->i32=>run()-1",
        ),
        (
            "mixed-generation-reserved-function",
            "#for(n in [1]){struct S{x:i32}fn bytes_len()->i32=>n;pub fn run()->i32=>bytes_len()}fn main()->i32=>run()-1",
        ),
    ] {
        let report = check(label, text);
        if report.status == ResultCategory::Success {
            accepted.push(label);
            continue;
        }
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
        assert_eq!(report.verification_runs, 0);
    }
    assert!(
        accepted.is_empty(),
        "reserved function names escaped: {accepted:?}"
    );
}

fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
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
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, (name, _))| (vec![name.to_string()], ModuleId(index as u32)))
            .collect(),
    }
}
const QUALIFIED: &[(&str, &str)] = &[
    (
        "main",
        "import model\n#for(n in [1]){type model=i32;struct Local{x:model.Item}pub fn run()->i32{let a=model.Item{value:13};let z:fn()->model.Item=model.input;return a.value+z().value-26}}fn main()->i32=>run()",
    ),
    (
        "model",
        "pub struct Item{pub value:i32}pub fn input()->Item=>Item{value:13}",
    ),
];
const DISTINCT: &[(&str, &str)] = &[
    (
        "main",
        "import a\nimport b\nfn main()->i32=>a.read(a.make())+b.read(b.make())-23",
    ),
    (
        "a",
        "#for(n in [11]){pub struct Packet{pub value:i32}pub type Result=Packet;pub fn make()->Result=>Result{value:n};pub fn read(x:Result)->i32=>x.value}",
    ),
    (
        "b",
        "#for(n in [12]){pub struct Packet{pub value:i32}pub type Result=Packet;pub fn make()->Result=>Result{value:n};pub fn read(x:Result)->i32=>x.value}",
    ),
];
fn project_check(label: &str, files: &[(&str, &str)], options: CompileOptions) -> CompileReport {
    let p = directory(label);
    for (name, text) in files {
        fs::write(p.join(format!("{name}.zil")), text).unwrap();
    }
    let report = zil_driver::compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(p.join("phases")),
            ..options
        },
    );
    fs::write(p.join("report.json"), report.to_json()).unwrap();
    report
}
#[test]
fn module_qualifiers_and_exported_nominal_identities_remain_distinct() {
    for (label, files) in [
        ("qualified-type-shadow", QUALIFIED),
        ("distinct-modules", DISTINCT),
    ] {
        let report = project_check(label, files, CompileOptions::default());
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
    let mixed = [
        (
            "main",
            "import a\nimport b\nfn main()->i32=>a.read(b.make())",
        ),
        DISTINCT[1],
        DISTINCT[2],
    ];
    let report = project_check("mixed-module-nominals", &mixed, CompileOptions::default());
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    let private = [
        (
            "main",
            "import hidden\nfn main()->i32{let p=hidden.Packet{secret:1};return 0}",
        ),
        ("hidden", "#for(n in [1]){pub struct Packet{secret:i32}}"),
    ];
    let report = project_check(
        "private-generated-field",
        &private,
        CompileOptions::default(),
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
}

#[test]
fn nested_types_share_fresh_and_warm_budget_exhaustion() {
    for nodes in [false, true] {
        let run = |n, session: &mut CompilationSession| {
            let mut limits = Limits::default();
            if nodes {
                limits.max_nodes = n;
            } else {
                limits.max_analysis_steps = n;
            }
            session.compile_source(
                &source(NESTED),
                &CompileOptions {
                    limits,
                    ..CompileOptions::default()
                },
            )
        };
        let (mut low, mut high) = (
            0,
            if nodes {
                Limits::default().max_nodes
            } else {
                Limits::default().max_analysis_steps
            },
        );
        while high - low > 1 {
            let mid = (low + high) / 2;
            let report = run(mid, &mut CompilationSession::default());
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid,
                _ => panic!("{}", report.to_json()),
            }
        }
        let out = directory(if nodes { "budget-nodes" } else { "budget-work" });
        fs::write(out.join("source.zil"), NESTED).unwrap();
        fs::write(out.join("threshold.txt"), format!("{high}\n")).unwrap();
        let mut session = CompilationSession::with_capacity(128);
        for (index, n) in [high, high, high - 1, high].into_iter().enumerate() {
            let report = run(n, &mut session);
            fs::write(out.join(format!("attempt-{index}.json")), report.to_json()).unwrap();
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
                assert_eq!(report.verification_runs, 0);
            } else {
                assert_eq!(report.verification_runs, 1);
            }
            if index == 1 || index == 3 {
                assert!(report.cache_hit);
            }
        }
    }
}

#[cfg(feature = "llvm-api")]
#[test]
fn independent_sources_and_modules_execute_at_both_optimization_levels() {
    use std::process::Command;
    use zil_driver::{NativeBackend, Optimization};
    for optimization in [Optimization::O0, Optimization::O2] {
        let cases = ACCEPTED
            .iter()
            .map(|(label, text)| (*label, vec![("main", *text)]))
            .chain([
                ("qualified", QUALIFIED.to_vec()),
                ("distinct", DISTINCT.to_vec()),
            ]);
        for (label, files) in cases {
            let label = format!("native-{label}-{optimization:?}");
            let out = directory(&label);
            let report = project_check(
                &label,
                &files,
                CompileOptions {
                    stop_after: Stage::Build,
                    backend: NativeBackend::LlvmApi,
                    optimization,
                    output: Some(out.join("program")),
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
            let result = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(out.join("stdout.bin"), &result.stdout).unwrap();
            fs::write(out.join("stderr.bin"), &result.stderr).unwrap();
            fs::write(
                out.join("exit.txt"),
                format!("{:?}\n", result.status.code()),
            )
            .unwrap();
            assert_eq!(result.status.code(), Some(0), "{label}: {result:?}");
            assert!(result.stdout.is_empty());
            assert!(result.stderr.is_empty());
        }
    }
}
