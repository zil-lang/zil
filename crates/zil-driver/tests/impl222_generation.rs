//! Real source generation, lexical hygiene, ordinary safety and cache controls.
use std::{collections::BTreeMap, fs, path::PathBuf};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, ProjectSources, Stage};

fn directory(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("generation222-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&path).unwrap();
    path
}
fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "generation222.zil", text)
}
fn check(label: &str, text: &str) -> CompileReport {
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
    report
}
fn pass(label: &str, text: &str) -> CompileReport {
    let report = check(label, text);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
    report
}
fn reject(label: &str, text: &str) -> CompileReport {
    let report = check(label, text);
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    report
}

const CASES: &[(&str, &str)] = &[
    (
        "array-length-closure-parameter-review",
        include_str!(
            "../../../examples/mainline/ct_for222/review_repair/array-closure-parameter.zil"
        ),
    ),
    (
        "array-length-u8",
        "fn main()->i32{#for(n in [u8(2)]){let data:[i32;n]=[4,8];return data[1]-8}}",
    ),
    (
        "array-length-parameter-review",
        include_str!(
            "../../../examples/mainline/ct_for222/review_repair/array-length-parameter.zil"
        ),
    ),
    (
        "array-length-local-review",
        include_str!("../../../examples/mainline/ct_for222/review_repair/array-length-local.zil"),
    ),
    (
        "array-length-result",
        include_str!("../../../examples/mainline/ct_for222/review_repair/array-return-type.zil"),
    ),
    (
        "array-length-closure",
        "fn main()->i32{#for(n in [2]){let f=(a:[i32;n])->[i32;n]=>a;let data=f([4,8]);return data[1]-8}}",
    ),
    (
        "array-length-type-argument",
        include_str!(
            "../../../examples/mainline/ct_for222/review_repair/array-generic-typearg.zil"
        ),
    ),
    (
        "range",
        "fn main()->i32{var sum=0;#for(i in 0..4){let item=i*3;sum+=item}return sum-18}",
    ),
    (
        "bool-array",
        "fn main()->i32{var sum=0;#for(flag in [true,false,true]){#if(flag){sum+=2}else{sum+=1}}return sum-5}",
    ),
    (
        "nested",
        "fn main()->i32{var sum=0;#for(i in 0..3){#for(j in 0..i){sum+=i+j}}return sum-6}",
    ),
    (
        "private-functions",
        "#for(i in [3,5]){fn helper()->i32=>i;#if(i==3){pub fn first()->i32=>helper()}else{pub fn second()->i32=>helper()}}fn main()->i32=>first()+second()-8",
    ),
    (
        "conditional-shadow",
        "fn main()->i32{var sum=0;#for(i in 0..2){#if(i==0){let i=7}else{let i=9}sum+=i}return sum-16}",
    ),
    (
        "outer-shadow",
        "fn main()->i32{let item=10;var sum=0;#for(i in 0..2){let item=item+i;sum+=item}return sum+item-31}",
    ),
    (
        "closure-shadow",
        "fn main()->i32{var sum=0;#for(i in 0..2){let f=(i:i32)=>i+1;sum+=f(4)+i}return sum-11}",
    ),
    (
        "record-shorthand",
        "struct R{n:i32}fn main()->i32{var sum=0;#for(n in [3,5]){let r=R{n};sum+=r.n}return sum-8}",
    ),
    (
        "receiving-continue",
        "fn main()->i32{var sum=0;for j in 0..3{#for(i in 0..2){sum+=1;#if(i==0){if j==0{continue}}}}return sum-5}",
    ),
    (
        "generated-return",
        "fn pick(n:i32)->i32{#for(i in [7,9]){if n==i{return i}}return 0}fn main()->i32=>pick(7)+pick(9)-16",
    ),
    (
        "u8-range",
        "fn main()->i32{var sum=0;#for(i in u8(1)..u8(4)){sum+=i32(i)}return sum-6}",
    ),
    (
        "large-i64",
        "fn big()->i64=>9000000000;fn main()->i32{var sum=i64(0);#for(i in big()..big()+2){sum+=i}return i32(sum-(big()*2+1))}",
    ),
    (
        "negative-range",
        "fn main()->i32{var sum=0;#for(i in -3..2){sum+=i}return sum+5}",
    ),
    (
        "empty-range",
        "fn main()->i32{#for(i in 5..2){let wrong:Missing=unknown}return 0}",
    ),
    (
        "empty-array",
        "fn main()->i32{#for(i in []){let wrong:Missing=unknown}return 0}",
    ),
    (
        "empty-declarations",
        "#for(i in 0..0){fn bad()->Missing=>unavailable}fn main()->i32=>0",
    ),
    (
        "long-array-identity-range",
        "fn main()->i32{var total=0;#for(i in [1,2,3,4,5,6,7,8]){total+=i}return total-36}",
    ),
    (
        "unit-array",
        "fn main()->i32{var total=0;#for(unit in [(),()]){let copy=unit;total+=1}return total-2}",
    ),
    (
        "local-scope",
        "fn main()->i32{let x=2;var sum=0;#for(i in 0..2){let x=3; if true{let x=4;sum+=x}sum+=x}return sum+x-16}",
    ),
];

#[test]
fn ordered_ranges_arrays_nested_selection_and_hygiene_reach_verify() {
    for (label, text) in CASES {
        let report = pass(label, text);
        let trace = fs::read_to_string(&report.artifacts["selection"]).unwrap();
        assert!(trace.contains("\"generations\""), "{label}: {trace}");
    }
}

#[test]
fn generation_boundaries_and_runtime_shadowing_fail_closed() {
    for (label, text) in [
        (
            "record-callee-iterator-shadow",
            "struct Cell{x:i32}fn main()->i32{#for(Cell in [8]){let c=Cell{x:3};return c.x+Cell-11}}",
        ),
        (
            "array-length-runtime-shadow",
            "fn main()->i32{#for(n in [2]){let n=2;let data:[i32;n]=[4,8];return data[1]-8}}",
        ),
        (
            "array-length-negative-iterator",
            "fn main()->i32{#for(n in [-1]){let data:[i32;n]=[4,8];return data[1]-8}}",
        ),
        (
            "type-name-is-not-value-binding",
            "#for(T in [2]){pub fn bad(x:T)->i32=>0}fn main()->i32=>0",
        ),
        (
            "runtime-bound",
            "fn main()->i32{let n=3;#for(i in 0..n){}return 0}",
        ),
        (
            "runtime-shadow-in-condition",
            "fn main()->i32{#for(i in 0..2){let i=7;#if(i==7){return 0}}return 1}",
        ),
        (
            "iterator-write",
            "fn main()->i32{#for(i in 0..2){i+=1}return 0}",
        ),
        (
            "private-local-does-not-escape",
            "fn main()->i32{#for(i in 0..1){let private=3}return private}",
        ),
        (
            "private-function-does-not-escape",
            "#for(i in 0..1){fn private()->i32=>i}fn main()->i32=>private()",
        ),
        (
            "duplicate-public",
            "#for(i in 0..2){pub fn same()->i32=>i}fn main()->i32=>0",
        ),
        (
            "duplicate-private-one-iteration",
            "#for(i in 0..1){fn same()->i32=>i;fn same()->i32=>i}fn main()->i32=>0",
        ),
        (
            "duplicate-local",
            "fn main()->i32{#for(i in 0..1){let x=i;let x=i}return 0}",
        ),
        (
            "noninteger-range",
            "fn main()->i32{#for(i in false..true){}return 0}",
        ),
        (
            "heterogeneous-array",
            "fn main()->i32{#for(i in [true,3]){}return 0}",
        ),
        (
            "nested-array",
            "fn main()->i32{#for(i in [[1,2],[3,4]]){}return 0}",
        ),
        (
            "unimplemented-emit",
            "fn main()->i32{#for(i in 0..1){#emit i}return 0}",
        ),
        (
            "generated-type",
            "#for(i in 0..1){struct S{x:i32}}fn main()->i32=>0",
        ),
        (
            "conditional-import",
            "#for(i in 0..1){import missing}fn main()->i32=>0",
        ),
        (
            "syntax-even-empty",
            "fn main()->i32{#for(i in 0..0){let =}return 0}",
        ),
        (
            "ordinary-unreachable-boundary",
            "fn pick()->i32{#for(i in [7,9]){return i}return 0}fn main()->i32=>pick()",
        ),
    ] {
        reject(label, text);
    }
    let generated = reject(
        "phase-dependency",
        "#for(i in 0..1){pub fn count()->i32=>3}fn main()->i32{#for(j in 0..count()){}return 0}",
    );
    assert_eq!(generated.phase, Phase::Ct);
    assert!(
        generated
            .diagnostics
            .iter()
            .any(|d| d.code == "E-CT-PHASE-DEPENDENCY"),
        "{}",
        generated.to_json()
    );
    let huge = reject(
        "bounded-before-output",
        "fn main()->i32{#for(i in 0..1000000000){let n=i}return 0}",
    );
    assert_eq!(
        huge.status,
        ResultCategory::ResourceLimit,
        "{}",
        huge.to_json()
    );
}

#[test]
fn generated_operations_still_enter_the_ordinary_safety_gate() {
    let duplicate = reject(
        "double-owner",
        "fn consume(give b:ByteBuffer)->i32{return 0}fn main()->i32{let b=bytes_new();#for(i in 0..2){consume(give b)}return 0}",
    );
    assert_ne!(duplicate.phase, Phase::Ct, "{}", duplicate.to_json());
    assert_eq!(duplicate.phase, Phase::Analyze, "{}", duplicate.to_json());
    assert!(
        duplicate
            .diagnostics
            .iter()
            .any(|d| d.code == "E-UNINITIALIZED")
    );
    let dangling = reject(
        "live-window-consumption",
        "fn consume(give b:ByteBuffer)->i32{return 0}fn main()->i32{let b=bytes_new();bytes_reserve(b,1);bytes_push(b,u8(65));#for(i in 0..1){let view=bytes_view(b);consume(give b);let n=i32(view[0]);if n==65{return n-65}}return 0}",
    );
    assert!(
        matches!(dangling.phase, Phase::Analyze | Phase::Verify),
        "{}",
        dangling.to_json()
    );
}

#[test]
fn cross_module_closed_dependencies_and_visibility_are_preserved() {
    let project = ProjectSources {
        root: ModuleId(0),
        modules: vec![
            source(
                "import cfg\nfn main()->i32{var n=0;#for(i in 0..(cfg).count()){n+=i}return n-6}",
            ),
            SourceUnit::new(
                FileId(1),
                ModuleId(1),
                "cfg.zil",
                "fn leaf()->i32=>4;pub fn count()->i32=>leaf()",
            ),
        ],
        paths: BTreeMap::from([(vec!["cfg".into()], ModuleId(1))]),
    };
    let out = directory("project");
    let report = CompilationSession::default().compile_project(
        &project,
        &CompileOptions {
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
    let trace = fs::read_to_string(&report.artifacts["selection"]).unwrap();
    assert!(trace.contains("\"function\":\"count\""));
    assert!(trace.contains("\"function\":\"leaf\""));
}

const BUDGET: &str = "fn count(n:i32)->i32{var i=0;while i<n{i+=1}return i}fn main()->i32{var n=0;#for(i in 0..count(3)){#if(i<2){n+=i}}let later=#(count(4));return n+later-5}";

fn threshold(stage: Stage, nodes: bool) -> usize {
    let (mut low, mut high) = (0, if nodes { 50_000 } else { 5_000 });
    let mut session = CompilationSession::with_capacity(256);
    while high - low > 1 {
        let mid = (low + high) / 2;
        let mut options = CompileOptions {
            stop_after: stage,
            ..CompileOptions::default()
        };
        if nodes {
            options.limits.max_nodes = mid;
        } else {
            options.limits.max_ct_steps = mid as u64;
        }
        let report = session.compile_source(&source(BUDGET), &options);
        if report.status == ResultCategory::Success {
            high = mid;
        } else {
            assert_eq!(
                report.status,
                ResultCategory::ResourceLimit,
                "{}",
                report.to_json()
            );
            low = mid;
        }
        assert_eq!(session.active_query_count(), 0);
    }
    high
}

#[test]
fn all_generations_conditions_and_later_queries_share_limits_and_cache_charges() {
    let core_steps = threshold(Stage::Core, false);
    let steps = threshold(Stage::Verify, false);
    let nodes = threshold(Stage::Verify, true);
    assert!(steps > core_steps);
    let out = directory("budget");
    fs::write(out.join("source.zil"), BUDGET).unwrap();
    fs::write(
        out.join("thresholds.json"),
        format!("{{\"core_steps\":{core_steps},\"full_steps\":{steps},\"full_nodes\":{nodes}}}"),
    )
    .unwrap();
    for (label, exact, node_budget) in [("steps", steps, false), ("nodes", nodes, true)] {
        let mut session = CompilationSession::with_capacity(128);
        for (attempt, budget) in [exact, exact - 1, exact, exact].into_iter().enumerate() {
            let mut options = CompileOptions {
                emit_dir: Some(out.join(format!("{label}-{attempt}"))),
                ..CompileOptions::default()
            };
            if node_budget {
                options.limits.max_nodes = budget;
            } else {
                options.limits.max_ct_steps = budget as u64;
            }
            let report = session.compile_source(&source(BUDGET), &options);
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

#[cfg(feature = "llvm-api")]
#[test]
fn generated_values_hygiene_and_receiving_control_flow_execute_at_o0_and_o2() {
    use std::process::Command;
    use zil_backend_llvm::Optimization;
    use zil_driver::NativeBackend;
    for optimization in [Optimization::O0, Optimization::O2] {
        for (label, text) in CASES {
            let out = directory(&format!("native-{optimization:?}-{label}"));
            let executable = out.join("program");
            let report = CompilationSession::default().compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend: NativeBackend::LlvmApi,
                    optimization,
                    output: Some(executable.clone()),
                    emit_dir: Some(out.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(out.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{label}: {}",
                report.to_json()
            );
            let result = Command::new(executable).output().unwrap();
            fs::write(out.join("stdout.log"), &result.stdout).unwrap();
            fs::write(out.join("stderr.log"), &result.stderr).unwrap();
            assert_eq!(
                result.status.code(),
                Some(0),
                "{optimization:?}/{label}: {result:?}"
            );
        }
    }
}
