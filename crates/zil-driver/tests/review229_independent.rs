//! Non-author source, namespace and cumulative-budget review of slice 229.
use std::{collections::BTreeMap, fs, path::PathBuf};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{CompilationSession, CompileOptions, CompileReport, ProjectSources, Stage};

fn input(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "review229.zil", text)
}
fn out(label: &str) -> PathBuf {
    let p = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review229-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&p).unwrap();
    p
}
fn check(label: &str, text: &str) -> CompileReport {
    let p = out(label);
    fs::write(p.join("source.zil"), text).unwrap();
    let r = CompilationSession::default().compile_source(
        &input(text),
        &CompileOptions {
            emit_dir: Some(p.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(p.join("report.json"), r.to_json()).unwrap();
    r
}
const TRIGGER: &str = "fn trigger<T>(x:T)->T where T:Copy=>x;";

#[test]
fn bounds_do_not_leak_between_parameters_or_hide_unexecuted_errors() {
    let mut unexpected = Vec::new();
    for (label, text) in [
        (
            "unconstrained-second",
            "fn bad<A,B>(a:A,b:B)->(B,B) where A:Copy=>(b,b);fn main()->i32=>0",
        ),
        (
            "shadowed-global-alias",
            "type T=i32;fn bad<T>(x:T)->T where T:Copy=>x+x;fn main()->i32=>0",
        ),
        (
            "shadowed-nominal",
            "struct T{x:i32}fn bad<T>(x:T)->i32 where T:Copy=>x.x;fn main()->i32=>0",
        ),
        (
            "duplicate-parameter",
            "fn bad<T,T>(x:T)->T where T:Copy=>x;fn main()->i32=>0",
        ),
        (
            "separate-invalid-clause",
            "trait Marker{}fn bad<T>(x:T)->T where T:Copy,T:Marker=>x;fn main()->i32=>0",
        ),
        (
            "separate-invalid-subject",
            "type Other=i32;fn bad<T>(x:T)->T where T:Copy,Other:Copy=>x;fn main()->i32=>0",
        ),
        (
            "reserved-copy-trait",
            "trait Copy{}fn bad<T>(x:T)->T where T:Copy=>x;fn main()->i32=>0",
        ),
        (
            "copy-not-callable",
            "fn bad<T>(x:T)->i32 where T:Copy{let f=()=>x();return 0}fn main()->i32=>0",
        ),
    ] {
        let r = check(label, text);
        if r.status == ResultCategory::Success {
            unexpected.push(format!("{label}: {}", r.to_json()));
            continue;
        }
        assert_eq!(r.phase, Phase::Expand, "{label}: {}", r.to_json());
        assert_eq!(r.verification_runs, 0);
    }
    assert!(unexpected.is_empty(), "{}", unexpected.join("\n"));
}

#[test]
fn record_shadowing_has_a_preserved_inline_control() {
    for (label, text) in [
        (
            "inline-shadowed-nominal",
            "struct T{x:i32}fn bad<T:Copy>(x:T)->i32=>x.x;fn main()->i32=>0",
        ),
        (
            "where-shadowed-nominal",
            "struct T{x:i32}fn bad<T>(x:T)->i32 where T:Copy=>x.x;fn main()->i32=>0",
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
fn nominal_protection_rechecks_later_binders_and_unused_bodies() {
    for (label, text) in [
        (
            "collision-duplicate",
            "struct T{x:i32}fn keep<T,T>(x:T)->T where T:Copy=>x;fn main()->i32=>0",
        ),
        (
            "collision-later-error",
            "struct T{x:i32}fn a_keep<T>(x:T)->T where T:Copy=>x;fn z_bad<U>(x:U)->U where U:Copy=>x+x;fn main()->i32=>0",
        ),
        (
            "collision-aggregate-child",
            "struct T{x:i32}struct Wrap<T>{value:T}fn bad<U>(x:Wrap<U>)->i32 where U:Copy=>x.value.x;fn main()->i32=>0",
        ),
        (
            "collision-pattern",
            "enum T{A,B}fn bad<T>(x:T)->i32 where T:Copy{match x{T.A()=>{return 0},T.B()=>{return 1}}}fn main()->i32=>0",
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
fn unrelated_where_function_does_not_open_methods_impls_or_foreign_templates() {
    for (label, decl) in [
        (
            "generic-method",
            "struct S{x:i32}impl S{fn bad<T>(self:&Self,x:T)->T where T:Copy=>x;}",
        ),
        (
            "concrete-method",
            "struct S{x:i32}impl S{fn bad(self:&Self)->i32 where i32:Copy=>self.x;}",
        ),
        (
            "impl-clause",
            "struct S<T>{x:T}impl<T> S<T> where T:Copy{fn value(self:&Self)->T=>self.x;}",
        ),
        (
            "foreign-clause",
            "extern \"C\" fn bad<T>(x:T)->T where T:Copy;",
        ),
        (
            "trait-clause",
            "trait Marker{fn bad<T>(self:&Self,x:T)->T where T:Copy;}",
        ),
    ] {
        let plain = check(
            &format!("closed-{label}"),
            &format!("{decl}fn main()->i32=>0"),
        );
        let combined = check(
            &format!("trigger-{label}"),
            &format!("{TRIGGER}{decl}fn main()->i32=>0"),
        );
        assert_ne!(
            plain.status,
            ResultCategory::Success,
            "plain {label}: {}",
            plain.to_json()
        );
        assert_ne!(
            combined.status,
            ResultCategory::Success,
            "combined {label}: {}",
            combined.to_json()
        );
        assert_ne!(
            plain.phase,
            Phase::Parse,
            "fixture invalid {label}: {}",
            plain.to_json()
        );
        assert_ne!(
            combined.phase,
            Phase::Parse,
            "fixture invalid {label}: {}",
            combined.to_json()
        );
        assert_eq!(combined.verification_runs, 0);
    }
}

fn project(helper: &str) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: vec![
            input("import helper\nfn main()->i32=>0"),
            SourceUnit::new(FileId(1), ModuleId(1), "helper.zil", helper),
        ],
        paths: BTreeMap::from([(vec!["helper".into()], ModuleId(1))]),
    }
}
#[test]
fn uncalled_imported_helpers_keep_symbolic_scope() {
    for (label, helper) in [
        (
            "imported-second",
            "pub fn bad<A,B>(a:A,b:B)->(B,B) where A:Copy=>(b,b)",
        ),
        (
            "imported-alias",
            "type T=i32;pub fn bad<T>(x:T)->T where T:Copy=>x+x",
        ),
    ] {
        let p = out(label);
        fs::write(p.join("helper.zil"), helper).unwrap();
        let r = CompilationSession::default().compile_project(
            &project(helper),
            &CompileOptions {
                emit_dir: Some(p.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(p.join("report.json"), r.to_json()).unwrap();
        assert_ne!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.phase, Phase::Expand);
        assert_eq!(r.verification_runs, 0);
    }
}

const PLAIN: &str = "type Word=i32;type Alias=Word;struct Pair{x:Alias,y:Alias}fn duplicate<T:Copy>(x:T)->(T,T)=>(x,x);fn keep<U:Copy>(x:U)->U=>duplicate(x).0;fn main()->i32{let p=keep(Pair{x:7,y:9});let q=duplicate((true,p));if q.1.0{return q.0.1.x+q.1.1.y-16}return 1}";
const WHERE: &str = "type Word=i32;type Alias=Word;struct Pair{x:Alias,y:Alias}fn duplicate<T>(x:T)->(T,T) where T:Copy=>(x,x);fn keep<U>(x:U)->U where U:Copy=>duplicate(x).0;fn main()->i32{let p=keep(Pair{x:7,y:9});let q=duplicate((true,p));if q.1.0{return q.0.1.x+q.1.1.y-16}return 1}";

fn minimum(nodes: bool) -> usize {
    let (mut lo, mut hi) = (
        0,
        if nodes {
            Limits::default().max_nodes
        } else {
            Limits::default().max_analysis_steps
        },
    );
    while hi - lo > 1 {
        let n = (lo + hi) / 2;
        let mut limits = Limits::default();
        if nodes {
            limits.max_nodes = n
        } else {
            limits.max_analysis_steps = n
        }
        let r = CompilationSession::default().compile_source(
            &input(WHERE),
            &CompileOptions {
                limits,
                ..CompileOptions::default()
            },
        );
        match r.status {
            ResultCategory::Success => hi = n,
            ResultCategory::ResourceLimit => lo = n,
            _ => panic!("{}", r.to_json()),
        }
    }
    hi
}
#[test]
fn independent_alias_chain_shares_one_budget_through_specialization() {
    let p = out("budget");
    fs::write(p.join("source.zil"), WHERE).unwrap();
    let mut thresholds = Vec::new();
    for nodes in [false, true] {
        let n = minimum(nodes);
        thresholds.push((nodes, n));
        let mut session = CompilationSession::with_capacity(128);
        for (attempt, budget) in [n, n, n - 1, n].into_iter().enumerate() {
            let mut limits = Limits::default();
            if nodes {
                limits.max_nodes = budget
            } else {
                limits.max_analysis_steps = budget
            }
            let r = session.compile_source(
                &input(WHERE),
                &CompileOptions {
                    limits,
                    ..CompileOptions::default()
                },
            );
            fs::write(p.join(format!("{nodes}-{attempt}.json")), r.to_json()).unwrap();
            assert_eq!(
                r.status,
                if attempt == 2 {
                    ResultCategory::ResourceLimit
                } else {
                    ResultCategory::Success
                },
                "{}",
                r.to_json()
            );
            assert_eq!(session.active_query_count(), 0);
            if attempt == 2 {
                assert_eq!(r.verification_runs, 0);
            } else {
                assert_eq!(r.verification_runs, 1);
            }
            if attempt == 1 || attempt == 3 {
                assert!(r.cache_hit, "{}", r.to_json());
            }
        }
    }
    fs::write(p.join("minimum.txt"), format!("{thresholds:?}\n")).unwrap();
}

#[cfg(feature = "llvm-api")]
#[test]
fn independent_inline_where_and_repetition_have_identical_machine_semantics() {
    use std::process::Command;
    use zil_driver::{NativeBackend, Optimization};
    let duplicate = WHERE
        .replace("where T:Copy", "where T:Copy+Copy,T:Copy")
        .replace("where U:Copy", "where U:Copy,U:Copy");
    for optimization in [Optimization::O0, Optimization::O2] {
        let mut llvm = None;
        for (label, text) in [
            ("plain", PLAIN),
            ("where", WHERE),
            ("duplicates", duplicate.as_str()),
        ] {
            let p = out(&format!("native-{optimization:?}-{label}"));
            fs::write(p.join("source.zil"), text).unwrap();
            let r = CompilationSession::default().compile_source(
                &input(text),
                &CompileOptions {
                    stop_after: Stage::Build,
                    backend: NativeBackend::LlvmApi,
                    optimization,
                    output: Some(p.join("program")),
                    emit_dir: Some(p.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(p.join("report.json"), r.to_json()).unwrap();
            assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
            let result = Command::new(&r.artifacts["executable"]).output().unwrap();
            fs::write(p.join("stdout.bin"), &result.stdout).unwrap();
            fs::write(p.join("stderr.bin"), &result.stderr).unwrap();
            fs::write(p.join("exit.txt"), format!("{:?}", result.status.code())).unwrap();
            assert_eq!(result.status.code(), Some(0), "{result:?}");
            assert!(result.stdout.is_empty() && result.stderr.is_empty());
            let body = fs::read_to_string(p.join("phases/llvm.ll"))
                .unwrap()
                .lines()
                .filter(|s| !s.starts_with("; verified-fingerprint: "))
                .collect::<Vec<_>>()
                .join("\n");
            if label == "plain" {
                llvm = Some(body)
            } else {
                assert_eq!(Some(body), llvm, "machine IR mismatch for {label}");
            }
        }
    }
}

#[cfg(feature = "llvm-api")]
#[test]
fn real_nominals_and_generic_wrappers_keep_scoped_native_identity() {
    use std::process::Command;
    use zil_driver::{NativeBackend, Optimization};
    const LOCAL: &str = "struct T{x:i32}struct Wrap<T>{value:T}fn duplicate<T>(x:T)->(T,T) where T:Copy=>(x,x);fn concrete<U>(x:U)->i32 where U:Copy{let pair=duplicate(x);let t=T{x:9};return t.x}fn main()->i32{let a=duplicate(T{x:6});let b=duplicate(Wrap::<i32>{value:7});return a.1.x+b.1.value+concrete(true)-22}";
    const HELPER: &str = "pub struct T{x:i32}pub struct Wrap<T>{value:T}pub fn duplicate<T>(x:T)->(T,T) where T:Copy=>(x,x);pub fn make()->T=>T{x:6};pub fn wrapper()->Wrap<i32>=>Wrap::<i32>{value:7}";
    const ROOT: &str = "import helper\nfn main()->i32{let a=helper.duplicate(helper.make());let b=helper.duplicate(helper.wrapper());return a.1.x+b.1.value-13}";
    for optimization in [Optimization::O0, Optimization::O2] {
        for project_mode in [false, true] {
            let p = out(&format!("nominal-native-{optimization:?}-{project_mode}"));
            let options = CompileOptions {
                stop_after: Stage::Build,
                backend: NativeBackend::LlvmApi,
                optimization,
                output: Some(p.join("program")),
                emit_dir: Some(p.join("phases")),
                ..CompileOptions::default()
            };
            let mut session = CompilationSession::default();
            let report = if project_mode {
                fs::write(p.join("root.zil"), ROOT).unwrap();
                fs::write(p.join("helper.zil"), HELPER).unwrap();
                let mut sources = project(HELPER);
                sources.modules[0] = input(ROOT);
                session.compile_project(&sources, &options)
            } else {
                fs::write(p.join("source.zil"), LOCAL).unwrap();
                session.compile_source(&input(LOCAL), &options)
            };
            fs::write(p.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let execution = Command::new(&report.artifacts["executable"])
                .output()
                .unwrap();
            fs::write(p.join("stdout.bin"), &execution.stdout).unwrap();
            fs::write(p.join("stderr.bin"), &execution.stderr).unwrap();
            fs::write(p.join("exit.txt"), format!("{:?}", execution.status.code())).unwrap();
            assert_eq!(execution.status.code(), Some(0));
            assert!(execution.stdout.is_empty() && execution.stderr.is_empty());
        }
    }
}
