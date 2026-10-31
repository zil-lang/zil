//! Finite generated declarations remain ordinary untrusted source programs.
use std::{fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, Stage};

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "generated-types228.zil", text)
}
fn directory(label: &str) -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("types228-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&p).unwrap();
    p
}
fn check(label: &str, text: &str) -> CompileReport {
    let p = directory(label);
    fs::write(p.join("source.zil"), text).unwrap();
    let report = CompilationSession::default().compile_source(
        &source(text),
        &CompileOptions {
            emit_dir: Some(p.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(p.join("report.json"), report.to_json()).unwrap();
    report
}
const CASES: &[(&str, &str)] = &[
    (
        "arrays-defaults",
        include_str!("../../../examples/mainline/ct_types228/arrays_defaults.zil"),
    ),
    (
        "arrays-defaults-plain",
        include_str!("../../../examples/mainline/ct_types228/arrays_defaults_plain.zil"),
    ),
    (
        "forward-alias",
        "#for(n in [3]){type A=B;type B=S;pub fn go()->i32{let x=A{x:n};return x.x-3}struct S{x:i32}}fn main()->i32=>go()",
    ),
    (
        "value-shadow",
        "#for(n in [3]){struct Cell{x:i32}pub fn go()->i32{let Cell=7;let x=Cell{x:n};return x.x+Cell-10}}fn main()->i32=>go()",
    ),
    (
        "generic-type-shadow",
        "#for(n in [1]){type T=i32;fn id<T:Copy>(x:T)->T=>x;pub fn go()->i32{if id::<bool>(true){return 0}return 1}}fn main()->i32=>go()",
    ),
    (
        "type-and-iterator",
        "#for(n in [2]){type n=i32;struct S{x:n}pub fn go()->n{let value=S{x:n};return value.x-2}}fn main()->i32=>go()",
    ),
    (
        "nested-shadow",
        "#for(n in [1]){struct S{x:i32}#for(m in [2]){struct S{y:i32}pub fn inner()->i32{let s=S{y:m};return s.y}}pub fn outer()->i32{let s=S{x:n};return s.x}}fn main()->i32=>inner()+outer()-3",
    ),
    (
        "public-type",
        "#for(n in [3]){pub struct S{x:i32}pub type Alias=S}fn main()->i32{let s=Alias{x:3};return s.x-3}",
    ),
    (
        "empty-declarations",
        "#for(n in []){struct S{x:Missing}type A=Missing}fn main()->i32=>0",
    ),
    (
        "array-alias-function-pointer",
        "#for(n in [2]){type Data=[i32;n];struct S{x:Data}fn sum(s:S)->i32=>s.x[0]+s.x[1];pub fn go()->i32{let f:fn(S)->i32=sum;return f(S{x:[3,4]})-7}}fn main()->i32=>go()",
    ),
    (
        "alias-enum",
        "enum E{A(i32),B}#for(n in [3]){type Value=E;pub fn go()->i32{let e=Value.A(n);match e{Value.A(x)=>{return x-3},Value.B()=>{return 1}}}}fn main()->i32=>go()",
    ),
];

#[test]
fn declarations_aliases_defaults_and_namespaces_reach_independent_verify() {
    for (label, text) in CASES {
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
fn duplicate_nominals_visibility_and_unchecked_types_still_reject() {
    for (label, text) in [
        (
            "duplicate-private",
            "#for(n in [1]){struct S{x:i32}struct S{x:i32}}fn main()->i32=>0",
        ),
        (
            "duplicate-alias",
            "#for(n in [1]){type A=i32;type A=bool}fn main()->i32=>0",
        ),
        (
            "duplicate-public",
            "#for(n in [1,2]){pub struct S{x:i32}}fn main()->i32=>0",
        ),
        (
            "public-alias",
            "#for(n in [1,2]){pub type A=i32}fn main()->i32=>0",
        ),
        (
            "private-escape",
            "#for(n in [1]){struct S{x:i32}}fn main()->i32{let x=S{x:1};return x.x}",
        ),
        (
            "alias-private-escape",
            "#for(n in [1]){type A=i32}fn main()->i32{let x:A=0;return x}",
        ),
        (
            "different-iteration-nominals",
            "#for(n in [1,2]){struct S{x:i32}#if(n==1){pub fn make()->S=>S{x:1}}else{pub fn use(x:S)->i32=>x.x}}fn main()->i32=>use(make())",
        ),
        (
            "unused-generic-privilege",
            "#for(n in [1]){type T=i32;fn bad<T>(x:T)->T=>x+x}fn main()->i32=>0",
        ),
        (
            "alias-cycle",
            "#for(n in [1]){type A=B;type B=A}fn main()->i32=>0",
        ),
        (
            "recursive-layout",
            "#for(n in [1]){struct S{x:S}}fn main()->i32=>0",
        ),
        (
            "unused-bad-field",
            "#for(n in [1]){struct S{x:Missing}}fn main()->i32=>0",
        ),
        (
            "unknown-attribute",
            "#for(n in [1]){#[missing]struct S{x:i32}}fn main()->i32=>0",
        ),
        (
            "unknown-field-attribute",
            "#for(n in [1]){struct S{#[missing]x:i32}}fn main()->i32=>0",
        ),
        (
            "unknown-alias-attribute",
            "#for(n in [1]){#[missing]type S=i32}fn main()->i32=>0",
        ),
    ] {
        let report = check(label, text);
        assert_ne!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
    }
}

#[test]
fn declaration_reserved_names_cannot_escape_through_alpha_renaming() {
    for (label, declaration) in [
        ("scalar", "type i32=bool"),
        ("bytes", "struct ByteBuffer{x:i32}"),
        ("store", "type Store=i32"),
        ("copy", "struct Copy{x:i32}"),
        ("operation", "type bytes_view=i32"),
    ] {
        let plain = check(
            &format!("reserved-plain-{label}"),
            &format!("{declaration};fn main()->i32=>0"),
        );
        let generated = check(
            &format!("reserved-generated-{label}"),
            &format!("#for(n in [1]){{{declaration}}}fn main()->i32=>0"),
        );
        assert_ne!(plain.status, ResultCategory::Success, "{}", plain.to_json());
        assert_eq!(
            generated.status,
            plain.status,
            "{label}: {}",
            generated.to_json()
        );
        assert_eq!(
            generated.phase,
            plain.phase,
            "{label}: {}",
            generated.to_json()
        );
        assert!(
            generated
                .diagnostics
                .iter()
                .any(|d| plain.diagnostics.iter().any(|p| p.code == d.code)),
            "{label}: {}",
            generated.to_json()
        );
    }
}

#[test]
fn generated_function_names_follow_the_ordinary_function_rule() {
    for name in [
        "i32",
        "c_null",
        "bytes_len",
        "store_at",
        "ByteBuffer",
        "Store",
        "Copy",
        "bool",
    ] {
        let plain = check(
            &format!("function-name-plain-{name}"),
            &format!("struct S{{x:i32}}fn {name}()->i32=>1;fn main()->i32=>0"),
        );
        let allowed = matches!(name, "Copy" | "bool");
        assert_eq!(
            plain.status == ResultCategory::Success,
            allowed,
            "{name}: {}",
            plain.to_json()
        );
        for (kind, types) in [("pure", ""), ("mixed", "struct S{x:i32}")] {
            let generated = check(
                &format!("function-name-{kind}-{name}"),
                &format!(
                    "#for(n in [1]){{{types}fn {name}()->i32=>n;pub fn run()->i32=>{name}()}}fn main()->i32=>run()-1"
                ),
            );
            assert_eq!(
                generated.status,
                plain.status,
                "{kind}/{name}: {}",
                generated.to_json()
            );
            assert_eq!(
                generated.phase,
                plain.phase,
                "{kind}/{name}: {}",
                generated.to_json()
            );
            if allowed {
                assert_eq!(generated.verification_runs, 1);
            } else {
                assert_eq!(generated.verification_runs, 0);
                assert!(
                    generated
                        .diagnostics
                        .iter()
                        .any(|d| plain.diagnostics.iter().any(|p| p.code == d.code)),
                    "{kind}/{name}: {}",
                    generated.to_json()
                );
            }
        }
    }
}

#[test]
fn unsupported_declaration_kinds_and_configready_remain_closed() {
    for (label, declaration) in [
        ("generic-struct", "struct S<T>{x:T}"),
        ("generic-alias", "type A<T>=T"),
        ("method", "struct S{x:i32;fn value(self:Self)->i32=>self.x}"),
        ("resource", "resource R{x:i32}"),
        ("enum", "enum E{A}"),
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
    }
    let report = check(
        "configready",
        "#for(n in [1]){pub struct Shape{x:i32}}#if(Shape){fn selected()->i32=>0}fn main()->i32=>0",
    );
    assert_eq!(report.phase, Phase::Ct, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-CT-PHASE-DEPENDENCY"),
        "{}",
        report.to_json()
    );
}

const BUDGET: &str = "#for(n in [2,3]){type A=[i32;n];struct S{x:A,tag:i32=n}#if(n==2){pub fn one()->i32=>S{x:[1,2]}.tag}else{pub fn two()->i32=>S{x:[1,2,3]}.tag}}fn main()->i32=>one()+two()-5";

#[test]
fn generated_type_tables_share_request_limits_and_warm_cache_charges() {
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
                &source(BUDGET),
                &CompileOptions {
                    limits,
                    ..CompileOptions::default()
                },
            )
        };
        while hi - lo > 1 {
            let n = (hi + lo) / 2;
            let report = run(n, &mut CompilationSession::default());
            if report.status == ResultCategory::Success {
                hi = n
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
        let p = directory(if nodes { "budget-nodes" } else { "budget-work" });
        fs::write(p.join("source.zil"), BUDGET).unwrap();
        fs::write(p.join("minimum.json"), format!("{{\"minimum\":{hi}}}\n")).unwrap();
        let mut session = CompilationSession::with_capacity(128);
        for (attempt, n) in [hi, hi, hi - 1, hi].into_iter().enumerate() {
            let report = run(n, &mut session);
            fs::write(p.join(format!("attempt-{attempt}.json")), report.to_json()).unwrap();
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
        assert_eq!(fresh.status, ResultCategory::Success, "{}", fresh.to_json());
    }
}

#[cfg(feature = "llvm-api")]
#[test]
fn generated_and_plain_declarations_execute_at_o0_and_o2() {
    use std::process::Command;
    use zil_backend_llvm::Optimization;
    use zil_driver::NativeBackend;
    for optimization in [Optimization::O0, Optimization::O2] {
        for (label, text) in CASES {
            let p = directory(&format!("native-{optimization:?}-{label}"));
            let executable = p.join("program");
            let report = CompilationSession::default().compile_source(
                &source(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend: NativeBackend::LlvmApi,
                    optimization,
                    output: Some(executable.clone()),
                    emit_dir: Some(p.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(p.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{label}: {}",
                report.to_json()
            );
            let result = Command::new(executable).output().unwrap();
            fs::write(p.join("stdout.log"), &result.stdout).unwrap();
            fs::write(p.join("stderr.log"), &result.stderr).unwrap();
            assert_eq!(
                result.status.code(),
                Some(0),
                "{optimization:?}/{label}: {result:?}"
            );
            assert!(
                result.stderr.is_empty(),
                "{optimization:?}/{label}: {result:?}"
            );
        }
    }
}
