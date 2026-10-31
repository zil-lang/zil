//! Non-author probes for the exact joint candidate 2c8acce8f01d.
//! These sources are independent of the implementation author's fixtures.
use std::{fs, path::PathBuf, process::Command};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization, ProjectSources,
    Stage,
};

fn output(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("review231-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&path).unwrap();
    path
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

fn check(label: &str, files: &[(&str, &str)]) -> CompileReport {
    let path = output(label);
    for (name, source) in files {
        fs::write(path.join(format!("{name}.zil")), source).unwrap();
    }
    let report = CompilationSession::default().compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(path.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(path.join("report.json"), report.to_json()).unwrap();
    report
}

fn accept(label: &str, files: &[(&str, &str)]) {
    let report = check(label, files);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1);
}

fn reject(label: &str, files: &[(&str, &str)]) {
    let report = check(label, files);
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
    assert!(!report.artifacts.contains_key("executable"));
}

const ORDERED: &str = r#"
trait Code { fn code(self:&Self)->i32; }
trait Mark {}
struct Packet { n:i32 }
impl Code for Packet { fn code()->i32=>.n }
impl Mark for Packet {}
fn read<T>(x:&T)->i32 where T:(((Code)))=>x.code()
fn pair<A,B>(a:A,b:B)->(A,A,B) where B:Mark,A:Copy,A:Code,B:Code=>(a,a,b)
fn main()->i32 {
    let p=pair(Packet{n:9},Packet{n:6})
    return read(&p.0)+read(&p.1)+read(&p.2)-24
}
"#;

fn inline_ordered() -> String {
    let inline = ORDERED
        .replace(
            "read<T>(x:&T)->i32 where T:(((Code)))",
            "read<T:(((Code)))>(x:&T)->i32",
        )
        .replace(
            "pair<A,B>(a:A,b:B)->(A,A,B) where B:Mark,A:Copy,A:Code,B:Code",
            "pair<A:Copy+Code,B:Mark+Code>(a:A,b:B)->(A,A,B)",
        );
    assert!(!inline.contains("where"));
    inline
}

#[test]
fn independent_grouped_bounds_and_cross_subject_clause_order() {
    accept("ordered-where", &[("main", ORDERED)]);
    accept("ordered-inline", &[("main", &inline_ordered())]);
    let copies = ORDERED.replace(
        "where B:Mark,A:Copy,A:Code,B:Code",
        "where A:Copy+Copy,B:Mark,A:Copy+Code,B:Code,A:Copy",
    );
    accept("copy-clause-order", &[("main", &copies)]);
    for (label, old, new) in [
        (
            "repeat-grouped-nominal",
            "where T:(((Code)))",
            "where T:(Code)+((Code))",
        ),
        (
            "repeat-late-nominal",
            "where B:Mark,A:Copy,A:Code,B:Code",
            "where B:Mark,A:Copy,A:Code,B:Code,B:(Mark)",
        ),
        (
            "remove-copy",
            "where B:Mark,A:Copy,A:Code,B:Code",
            "where B:Mark,A:Code,B:Code",
        ),
        (
            "unknown-late-subject",
            "where B:Mark,A:Copy,A:Code,B:Code",
            "where B:Mark,A:Copy,A:Code,B:Code,Missing:Mark",
        ),
    ] {
        reject(label, &[("main", &ORDERED.replace(old, new))]);
    }
}

const ROOT: &str = "import rules;import data;fn read<T>(x:&T)->i32 where T:((rules.Code))=>x.code();fn main()->i32{let x=data.make();return read(&x)-13}";
const RULES: &str = "pub trait Code{fn code(self:&Self)->i32;}pub struct Payload{n:i32}impl Code for Payload{fn code(self:&Self)->i32=>self.n}pub fn make(n:i32)->Payload=>Payload{n:n}";
const DATA: &str = "import rules;pub fn make()->rules.Payload=>rules.make(13)";

#[test]
fn independent_cross_module_impl_matches_existing_inline_boundary() {
    let main = "import rules;import data;fn read<T>(x:&T)->i32 where T:((rules.Code))=>x.code();fn main()->i32{let x=data.make();return read(&x)-13}";
    let rules = "pub trait Code{fn code(self:&Self)->i32;}";
    let data = "import rules;pub struct Payload{n:i32}impl rules.Code for Payload{fn code(self:&Self)->i32=>self.n}pub fn make()->Payload=>Payload{n:13}";
    let inline = main.replace(
        "read<T>(x:&T)->i32 where T:((rules.Code))",
        "read<T:((rules.Code))>(x:&T)->i32",
    );
    assert!(!inline.contains("where"));
    for (form, source) in [("where", main), ("inline", inline.as_str())] {
        let report = check(
            &format!("existing-cross-module-impl-{form}"),
            &[("main", source), ("rules", rules), ("data", data)],
        );
        assert_eq!(
            report.status,
            ResultCategory::Unsupported,
            "{}",
            report.to_json()
        );
        assert_eq!(report.phase, Phase::Types, "{}", report.to_json());
        for message in [
            "trait implementation resolution",
            "trait receiver declaration is not available in this source module",
        ] {
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == "E-UNSUPPORTED"
                        && diagnostic.message == message),
                "{}",
                report.to_json()
            );
        }
        assert_eq!(report.verification_runs, 0);
        assert!(!report.artifacts.contains_key("executable"));
    }
}

#[test]
fn independent_module_identity_alias_privacy_and_binder_boundaries() {
    accept(
        "qualified-group",
        &[("main", ROOT), ("rules", RULES), ("data", DATA)],
    );
    let lookalike = "pub trait Code{fn code(self:&Self)->i32;}pub struct Payload{n:i32}impl Code for Payload{fn code(self:&Self)->i32=>self.n}pub fn make()->Payload=>Payload{n:13}";
    reject(
        "same-method-wrong-trait",
        &[("main", ROOT), ("rules", RULES), ("data", lookalike)],
    );
    for (label, main, rules) in [
        (
            "private-group",
            "import rules;fn keep<T>(x:T)->T where T:((rules.Secret))=>x;fn main()->i32=>0",
            "trait Secret{}",
        ),
        (
            "qualified-alias",
            "import rules;fn keep<T>(x:T)->T where T:((rules.Alias))=>x;fn main()->i32=>0",
            "pub trait Mark{}pub type Alias=Mark",
        ),
        (
            "module-binder-shadow",
            "import rules;fn keep<rules,T>(x:T)->T where T:rules.Mark=>x;fn main()->i32=>0",
            "pub trait Mark{}",
        ),
        (
            "projected-subject",
            "import rules;fn keep<T>(x:T)->T where T.Member:rules.Mark=>x;fn main()->i32=>0",
            "pub trait Mark{}",
        ),
        (
            "ordinary-type-bound",
            "import rules;fn keep<T>(x:T)->T where T:rules.Mark=>x;fn main()->i32=>0",
            "pub struct Mark{n:i32}",
        ),
    ] {
        reject(label, &[("main", main), ("rules", rules)]);
    }
    reject(
        "alias-binder-privilege",
        &[(
            "main",
            "trait Mark{}struct Record{n:i32}type Alias=Record;fn bad<Alias>(x:Alias)->i32 where Alias:Mark=>x.n;fn main()->i32=>0",
        )],
    );
}

#[test]
fn independent_reserved_capabilities_and_similar_ordinary_trait_names() {
    for name in [
        "Transfer", "Share", "Send", "Sync", "Drop", "Fn", "FnMut", "FnOnce",
    ] {
        let declaration = format!("pub trait {name}{{}}");
        for (form, header) in [
            (
                "where",
                format!("fn keep<T>(x:T)->T where T:((caps.{name}))=>x"),
            ),
            ("inline", format!("fn keep<T:((caps.{name}))>(x:T)->T=>x")),
        ] {
            reject(
                &format!("reserved-{name}-{form}"),
                &[
                    ("main", &format!("import caps;{header};fn main()->i32=>0")),
                    ("caps", &declaration),
                ],
            );
        }
    }
    for name in ["User_Send", "Send_User", "FnOnce_Extra", "Module0_Send"] {
        let declaration = format!("pub trait {name}{{}}");
        let main =
            format!("import caps;fn keep<T>(x:T)->T where T:((caps.{name}))=>x;fn main()->i32=>0");
        accept(
            &format!("ordinary-{name}"),
            &[("main", &main), ("caps", &declaration)],
        );
    }
}

#[test]
fn independent_raw_ast_prefixes_only_apply_the_reserved_name_exclusion() {
    use zil_syntax::ast::{ItemKind, TypeKind};
    let text = "trait Tag{}fn keep<T>(x:T)->T where T:Tag=>x;fn main()->i32=>0";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw-review231.zil", text);
    let original = zil_syntax::parse(&source).into_result().unwrap();
    for (index, (name, blocked)) in [
        ("$module17_Send", true),
        ("$module0_Sync", true),
        ("$module43_FnOnce", true),
        ("$modulex_Send", false),
        ("$module_Send", false),
        ("$module17_User_Send", false),
        ("$module17_Send_User", false),
    ]
    .into_iter()
    .enumerate()
    {
        let mut syntax = original.clone();
        for item in &mut syntax.items {
            match &mut item.kind {
                ItemKind::Trait(declaration) => declaration.name = name.into(),
                ItemKind::Function(function) if !function.where_clause.is_empty() => {
                    function.where_clause[0].bounds[0].kind = TypeKind::Named {
                        path: vec![name.into()],
                        args: vec![],
                    };
                }
                _ => {}
            }
        }
        let result = zil_expand::expand_with_map_limits(&source, &syntax, &Limits::default());
        let path = output(&format!("raw-prefix-{index}"));
        fs::write(path.join("source.zil"), text).unwrap();
        fs::write(path.join("syntax.txt"), format!("{syntax:#?}")).unwrap();
        fs::write(path.join("result.txt"), format!("{result:#?}")).unwrap();
        assert_eq!(result.is_err(), blocked, "{name}: {result:?}");
        if let Err(errors) = result {
            assert!(
                errors.iter().any(|e| e
                    .message
                    .contains("cannot manufacture built-in capabilities")),
                "{name}: {errors:?}"
            );
        }
    }
}

const GENERATED: &str = "trait Read{fn value(self:&Self)->i32;}struct Cell{n:i32}impl Read for Cell{fn value()->i32=>.n}#for(offset in [3]){enum Tag{Value(i32)}pub fn get<T>(x:&T)->i32 where T:((Read)){let tag=Tag.Value(x.value()+offset);match tag{Tag.Value(v)=>{return v}}}}fn main()->i32{let x=Cell{n:7};return get(&x)-10}";

#[test]
fn independent_native_grouped_module_and_generated_enum_consumers() {
    let inline = inline_ordered();
    let cases = [
        ("where", vec![("main", ORDERED)]),
        ("inline", vec![("main", inline.as_str())]),
        (
            "project",
            vec![("main", ROOT), ("rules", RULES), ("data", DATA)],
        ),
        ("generated", vec![("main", GENERATED)]),
    ];
    for optimization in [Optimization::O0, Optimization::O2] {
        for (label, files) in &cases {
            let path = output(&format!("native-{optimization:?}-{label}"));
            for (name, source) in files {
                fs::write(path.join(format!("{name}.zil")), source).unwrap();
            }
            let executable = path.join("program");
            let backend = if cfg!(feature = "llvm-api") {
                NativeBackend::LlvmApi
            } else {
                NativeBackend::Clang
            };
            let report = CompilationSession::default().compile_project(
                &project(files),
                &CompileOptions {
                    stop_after: Stage::Build,
                    optimization,
                    backend,
                    output: Some(executable.clone()),
                    emit_dir: Some(path.join("phases")),
                    ..CompileOptions::default()
                },
            );
            fs::write(path.join("report.json"), report.to_json()).unwrap();
            assert_eq!(
                report.status,
                ResultCategory::Success,
                "{label}/{optimization:?}: {}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 1);
            let native = report.native_codegen.as_ref().unwrap();
            assert_eq!(native.backend, backend);
            assert!(native.fallback.is_none());
            if backend == NativeBackend::LlvmApi {
                assert!(!native.object_reports.is_empty());
                assert!(
                    native
                        .object_reports
                        .iter()
                        .all(|r| r.contains("\"input_kind\":\"builder\"")
                            && r.contains("\"verify_before_after\":true"))
                );
            }
            let result = Command::new(executable).output().unwrap();
            fs::write(path.join("stdout"), &result.stdout).unwrap();
            fs::write(path.join("stderr"), &result.stderr).unwrap();
            fs::write(
                path.join("exit.txt"),
                format!("{:?}\n", result.status.code()),
            )
            .unwrap();
            assert_eq!(
                result.status.code(),
                Some(0),
                "{label}/{optimization:?}: {result:?}"
            );
            assert!(
                result.stdout.is_empty() && result.stderr.is_empty(),
                "{result:?}"
            );
        }
    }
}

#[test]
fn independent_module_where_budget_boundary_survives_cache_replay() {
    let input = project(&[("main", ROOT), ("rules", RULES), ("data", DATA)]);
    for nodes in [true, false] {
        let defaults = Limits::default();
        let (mut low, mut high) = (
            0,
            if nodes {
                defaults.max_nodes
            } else {
                defaults.max_analysis_steps
            },
        );
        let run = |n, session: &mut CompilationSession| {
            let mut limits = Limits::default();
            if nodes {
                limits.max_nodes = n;
            } else {
                limits.max_analysis_steps = n;
            }
            session.compile_project(
                &input,
                &CompileOptions {
                    limits,
                    ..CompileOptions::default()
                },
            )
        };
        let ceiling = run(high, &mut CompilationSession::default());
        assert_eq!(
            ceiling.status,
            ResultCategory::Success,
            "{}",
            ceiling.to_json()
        );
        while high - low > 1 {
            let mid = low + (high - low) / 2;
            let report = run(mid, &mut CompilationSession::default());
            match report.status {
                ResultCategory::Success => high = mid,
                ResultCategory::ResourceLimit => low = mid,
                _ => panic!("{}", report.to_json()),
            }
        }
        let path = output(if nodes { "module-nodes" } else { "module-work" });
        fs::write(
            path.join("minimum.json"),
            format!("{{\"minimum\":{high}}}\n"),
        )
        .unwrap();
        for source in &input.modules {
            fs::write(path.join(&source.path), &source.text).unwrap();
        }
        let mut session = CompilationSession::with_capacity(128);
        for (attempt, n) in [high, high, high - 1, high].into_iter().enumerate() {
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
        let fresh = run(high, &mut CompilationSession::default());
        fs::write(path.join("fresh.json"), fresh.to_json()).unwrap();
        assert_eq!(fresh.status, ResultCategory::Success, "{}", fresh.to_json());
    }
}
