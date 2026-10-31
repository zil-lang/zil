//! Source and machine controls for ordinary nominal where-bound normalization.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, Optimization, ProjectSources, Stage,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn source(text: &str) -> SourceUnit {
    SourceUnit::new(FileId(0), ModuleId(0), "where231.zil", text)
}

fn output(label: &str) -> PathBuf {
    let directory = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!(
            "impl231-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&directory).unwrap();
    directory
}

fn check(label: &str, text: &str) -> CompileReport {
    let directory = output(label);
    fs::write(directory.join("source.zil"), text).unwrap();
    let report =
        CompilationSession::default().compile_source(&source(text), &CompileOptions::default());
    fs::write(directory.join("report.json"), report.to_json()).unwrap();
    report
}

fn accepted(label: &str, text: &str) {
    let report = check(label, text);
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{label}: {}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{label}: {}", report.to_json());
}

fn rejected(label: &str, text: &str, reason: &str) {
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
    assert!(!report.artifacts.contains_key("executable"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains(reason)),
        "{label}: {}",
        report.to_json()
    );
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

fn project_check(label: &str, files: &[(&str, &str)]) -> CompileReport {
    let directory = output(label);
    for (name, text) in files {
        fs::write(directory.join(format!("{name}.zil")), text).unwrap();
    }
    let report = zil_driver::compile_project(&project(files), &CompileOptions::default());
    fs::write(directory.join("report.json"), report.to_json()).unwrap();
    report
}

fn native(label: &str, files: &[(&str, &str)]) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let directory = output(label);
        for (name, text) in files {
            fs::write(directory.join(format!("{name}.zil")), text).unwrap();
        }
        let executable = directory.join("program");
        let options = CompileOptions {
            stop_after: Stage::Build,
            optimization,
            output: Some(executable.clone()),
            emit_dir: Some(directory.join("phases")),
            ..CompileOptions::default()
        };
        let report = if files.len() == 1 {
            zil_driver::compile_source(&source(files[0].1), &options)
        } else {
            zil_driver::compile_project(&project(files), &options)
        };
        fs::write(directory.join("report.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let result = Command::new(executable).output().unwrap();
        fs::write(directory.join("stdout"), &result.stdout).unwrap();
        fs::write(directory.join("stderr"), &result.stderr).unwrap();
        fs::write(
            directory.join("exit.txt"),
            format!("{:?}\n", result.status.code()),
        )
        .unwrap();
        assert_eq!(result.status.code(), Some(0), "{label}: {result:?}");
    }
}

const WHERE: &str = r#"
trait Read { fn get(self:&Self)->i32; }
trait Change { fn set(self:&mut Self,n:i32); }
trait Consume { fn finish(self:Self)->i32; }
struct Counter { n:i32 }
impl Read for Counter { fn get()->i32=>.n }
impl Change for Counter { fn set(n:i32){.n=n} }
impl Consume for Counter { fn finish()->i32=>.n }
fn read<T>(x:&T)->i32 where T:Read=>x.get()
fn relay<U>(x:&U)->i32 where U:Read=>read(x)
fn update<T>(x:&mut T,n:i32)->i32 where T:Read+Change{x.set(n);return x.get()}
fn duplicate<T>(x:T)->(T,T) where T:Copy+Read=>(x,x)
fn finish<T>(x:T)->i32 where T:Consume=>x.finish()
fn main()->i32{
    var c=Counter{n:1}
    if update(&mut c,20)!=20{return 1}
    let pair=duplicate(Counter{n:7})
    if read(&pair.0)+read(&pair.1)!=14{return 2}
    return finish(Counter{n:22})+relay(&c)-42
}
"#;

#[test]
fn nominal_where_matches_inline_dispatch_at_o0_and_o2() {
    let inline = WHERE
        .replace("read<T>(x:&T)->i32 where T:Read", "read<T:Read>(x:&T)->i32")
        .replace(
            "relay<U>(x:&U)->i32 where U:Read",
            "relay<U:Read>(x:&U)->i32",
        )
        .replace(
            "update<T>(x:&mut T,n:i32)->i32 where T:Read+Change",
            "update<T:Read+Change>(x:&mut T,n:i32)->i32",
        )
        .replace(
            "duplicate<T>(x:T)->(T,T) where T:Copy+Read",
            "duplicate<T:Copy+Read>(x:T)->(T,T)",
        )
        .replace(
            "finish<T>(x:T)->i32 where T:Consume",
            "finish<T:Consume>(x:T)->i32",
        );
    native("native-where", &[("main", WHERE)]);
    native("native-inline", &[("main", &inline)]);
}

#[test]
fn copy_repetition_and_distinct_nominal_bounds_keep_existing_rules() {
    for (label, declaration) in [
        (
            "inline-and-where",
            "fn read<T:Read>(x:&T)->i32 where T:Change=>x.get()",
        ),
        (
            "copy-before-nominal",
            "fn copy<T:Copy+Read+Copy>(x:T)->(T,T) where T:Copy=>(x,x)",
        ),
        (
            "copy-after-nominal",
            "fn copy<T:Read>(x:T)->(T,T) where T:Copy+Copy,T:Copy=>(x,x)",
        ),
        (
            "independent-subjects",
            "fn pair<A,B>(a:&A,b:&B)->i32 where B:Read,A:Read=>a.get()+b.get()",
        ),
    ] {
        accepted(
            label,
            &format!(
                "trait Read{{fn get(self:&Self)->i32;}}trait Change{{fn set(self:&mut Self,n:i32);}}{declaration};fn main()->i32=>0"
            ),
        );
    }
    for (label, declaration) in [
        (
            "duplicate-inline",
            "fn read<T:Read+Read>(x:&T)->i32=>x.get()",
        ),
        (
            "duplicate-where",
            "fn read<T>(x:&T)->i32 where T:Read+Read=>x.get()",
        ),
        (
            "duplicate-clause",
            "fn read<T>(x:&T)->i32 where T:Read,T:Read=>x.get()",
        ),
        (
            "duplicate-mixed",
            "fn read<T:Read>(x:&T)->i32 where T:Read=>x.get()",
        ),
        (
            "duplicate-grouped",
            "fn read<T:Read>(x:&T)->i32 where T:((Read))=>x.get()",
        ),
    ] {
        rejected(
            label,
            &format!("trait Read{{fn get(self:&Self)->i32;}}{declaration};fn main()->i32=>0"),
            "duplicate nominal trait bound",
        );
    }
}

#[test]
fn unused_definitions_still_require_declared_capabilities() {
    for (label, definition, reason) in [
        (
            "unknown-method",
            "fn bad<T>(x:&T)->i32 where T:Read=>x.missing()",
            "not provided by the declared trait bounds",
        ),
        (
            "wrong-result",
            "fn bad<T>(x:&T)->bool where T:Read=>x.get()",
            "generic body types must agree",
        ),
        (
            "wrong-arity",
            "fn bad<T>(x:&T)->i32 where T:Read=>x.get(true)",
            "argument count differs",
        ),
        (
            "implicit-copy",
            "fn bad<T>(x:T)->(T,T) where T:Read=>(x,x)",
            "used after move",
        ),
        (
            "field-access",
            "fn bad<T>(x:&T)->i32 where T:Read=>x.n",
            "requires a declared bound",
        ),
        (
            "unconstrained-forward",
            "fn read<T>(x:&T)->i32 where T:Read=>x.get();fn bad<U>(x:&U)->i32=>read(x)",
            "does not satisfy nominal trait bound",
        ),
        (
            "nominal-shadow",
            "struct T{n:i32}fn bad<T>(x:&T)->i32 where T:Read=>x.n",
            "requires a declared bound",
        ),
        (
            "shared-write",
            "fn bad<T>(x:&T) where T:Change{x.set(1)}",
            "shared reference cannot grant exclusive access",
        ),
        (
            "borrowed-consume",
            "fn bad<T>(x:&T)->i32 where T:Consume=>x.finish()",
            "cannot move an unknown non-Copy trait receiver",
        ),
    ] {
        rejected(
            label,
            &format!(
                "trait Read{{fn get(self:&Self)->i32;}}trait Change{{fn set(self:&mut Self,n:i32);}}trait Consume{{fn finish(self:Self)->i32;}}{definition};fn main()->i32=>0"
            ),
            reason,
        );
    }
}

#[test]
fn actual_implementation_and_method_signature_are_not_inferred_from_names() {
    rejected(
        "inherent-only",
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl C{fn get(self:&Self)->i32=>.n}fn read<T>(x:&T)->i32 where T:Read=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
        "does not satisfy nominal trait bound",
    );
    let report = check(
        "wrong-impl",
        "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl Read for C{fn get()->bool=>true}fn read<T>(x:&T)->i32 where T:Read=>x.get();fn main()->i32{let c=C{n:7};return read(&c)}",
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "E-TRAIT-SIGNATURE"),
        "{}",
        report.to_json()
    );
    let report = check(
        "active-shared-window",
        "trait Change{fn set(self:&mut Self,n:i32);}struct C{n:i32}impl Change for C{fn set(n:i32){.n=n}}fn update<T>(x:&mut T) where T:Change{x.set(1)}fn main()->i32{var c=C{n:7};let old=&c;update(&mut c);return old.n}",
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("E-BORROW") || d.code.starts_with("E-ADDRESS")),
        "{}",
        report.to_json()
    );
}

#[test]
fn unsupported_subjects_and_non_nominal_bounds_are_not_erased() {
    for (label, text, reason) in [
        (
            "missing-trait",
            "fn keep<T>(x:T)->T where T:Missing=>x;fn main()->i32=>0",
            "not a declared nominal trait",
        ),
        (
            "value-type-bound",
            "struct Plain{n:i32}fn keep<T>(x:T)->T where T:Plain=>x;fn main()->i32=>0",
            "not a declared nominal trait",
        ),
        (
            "alias-bound",
            "trait Read{fn get(self:&Self)->i32;}type Alias=Read;fn keep<T>(x:T)->T where T:Alias=>x;fn main()->i32=>0",
            "not a declared nominal trait",
        ),
        (
            "generic-trait",
            "trait Read<U>{fn get(self:&Self)->U;}fn keep<T>(x:T)->T where T:Read=>x;fn main()->i32=>0",
            "full trait solver",
        ),
        (
            "applied-trait",
            "trait Read<U>{fn get(self:&Self)->U;}fn keep<T>(x:T)->T where T:Read<i32> =>x;fn main()->i32=>0",
            "without type arguments",
        ),
        (
            "unknown-subject",
            "trait Read{}fn keep<T>(x:T)->T where U:Read=>x;fn main()->i32=>0",
            "declared by this function",
        ),
        (
            "projected-subject",
            "trait Read{}fn keep<T>(x:T)->T where T.Item:Read=>x;fn main()->i32=>0",
            "qualified or projected",
        ),
        (
            "bound-shadow",
            "trait Read{}fn keep<Read>(x:Read)->Read where Read:Read=>x;fn main()->i32=>0",
            "trait",
        ),
        (
            "store-bound",
            "fn keep<T>(x:T)->T where T:StoreElement=>x;fn main()->i32=>0",
            "outside the ordinary nominal where profile",
        ),
        (
            "const-binder",
            "trait Read{}fn keep<T,#N:i32>(x:T)->T where T:Read=>x;fn main()->i32=>0",
            "ordinary type parameters",
        ),
    ] {
        rejected(label, text, reason);
    }
}

const PROJECT: &[(&str, &str)] = &[
    (
        "main",
        include_str!("../../../examples/mainline/where_traits231/main.zil"),
    ),
    (
        "model",
        include_str!("../../../examples/mainline/where_traits231/model.zil"),
    ),
    (
        "helper",
        include_str!("../../../examples/mainline/where_traits231/helper.zil"),
    ),
];

#[test]
fn qualified_public_where_bounds_keep_module_identity_and_native_behavior() {
    native("native-project", PROJECT);
    let files = [
        (
            "main",
            "import left;import right;fn main()->i32{let a=left.make(7);let b=right.make(35);return left.read(&a)+right.read(&b)-42}",
        ),
        (
            "left",
            "pub trait Read{fn get(self:&Self)->i32;}pub struct C{n:i32}impl Read for C{fn get()->i32=>.n}pub fn make(n:i32)->C=>C{n:n};pub fn read<T>(x:&T)->i32 where T:Read=>x.get()",
        ),
        (
            "right",
            "pub trait Read{fn get(self:&Self)->i32;}pub struct C{n:i32}impl Read for C{fn get()->i32=>.n}pub fn make(n:i32)->C=>C{n:n};pub fn read<T>(x:&T)->i32 where T:Read=>x.get()",
        ),
    ];
    native("native-same-spelling", &files);
}

#[test]
fn private_missing_and_wrong_module_traits_cannot_satisfy_where_bounds() {
    for (label, bound, model, code) in [
        (
            "private",
            "model.Secret",
            "trait Secret{fn get(self:&Self)->i32;}",
            "E-MODULE-PRIVATE",
        ),
        (
            "missing-member",
            "model.Missing",
            "pub trait Read{fn get(self:&Self)->i32;}",
            "E-MODULE-MEMBER",
        ),
    ] {
        let main = format!(
            "import model;fn read<T>(x:&T)->i32 where T:{bound}=>x.get();fn main()->i32=>0"
        );
        let report = project_check(label, &[("main", &main), ("model", model)]);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{label}: {}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
    }
    let report = project_check(
        "wrong-module",
        &[
            (
                "main",
                "import left;import right;fn main()->i32{let c=right.make();return left.read(&c)}",
            ),
            (
                "left",
                "pub trait Read{fn get(self:&Self)->i32;}pub fn read<T>(x:&T)->i32 where T:Read=>x.get()",
            ),
            (
                "right",
                "pub trait Read{fn get(self:&Self)->i32;}pub struct C{n:i32}impl Read for C{fn get()->i32=>.n}pub fn make()->C=>C{n:7}",
            ),
        ],
    );
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("does not satisfy nominal trait bound")),
        "{}",
        report.to_json()
    );
    let report = project_check(
        "project-subject-shadow",
        &[
            ("main", "import model;fn main()->i32=>0"),
            (
                "model",
                "pub trait Read{fn get(self:&Self)->i32;}pub struct T{n:i32}pub fn bad<T>(x:&T)->i32 where T:Read=>x.n",
            ),
        ],
    );
    assert_ne!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.phase, Phase::Expand);
    assert_eq!(report.verification_runs, 0);
}

const BUDGET: &str = "type Number=i32;trait Read{fn get(self:&Self)->i32;}struct C{n:Number}impl Read for C{fn get()->i32=>.n}fn read<T>(x:&T)->i32 where T:Read=>x.get();fn main()->i32{let c=C{n:7};return read(&c)-7}";

#[test]
fn alias_in_trait_signature_retains_the_existing_inline_boundary() {
    let original = "type Number=i32;trait Read{fn get(self:&Self)->Number;}struct C{n:Number}impl Read for C{fn get()->Number=>.n}fn read<T>(x:&T)->Number where T:Read=>x.get();fn main()->i32{let c=C{n:7};return read(&c)-7}";
    let inline = original.replace(
        "read<T>(x:&T)->Number where T:Read",
        "read<T:Read>(x:&T)->Number",
    );
    for (label, text) in [
        ("original-alias-signature", original),
        ("inline-alias-signature", inline.as_str()),
    ] {
        rejected(label, text, "generic body types must agree");
    }
}

#[test]
fn reserved_capability_names_do_not_become_nominal_bounds_after_module_resolution() {
    for name in [
        "Send", "Sync", "Transfer", "Share", "Drop", "Fn", "FnMut", "FnOnce",
    ] {
        let model = format!("pub trait {name}{{}}");
        for (kind, main) in [
            (
                "where",
                format!(
                    "import model;fn keep<T>(x:T)->T where T:model.{name}=>x;fn main()->i32=>0"
                ),
            ),
            (
                "inline",
                format!("import model;fn keep<T:model.{name}>(x:T)->T=>x;fn main()->i32=>0"),
            ),
        ] {
            let report = project_check(
                &format!("reserved-{kind}-{name}"),
                &[("main", &main), ("model", &model)],
            );
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert_eq!(report.phase, Phase::Expand);
            assert_eq!(report.verification_runs, 0);
            assert!(
                report.diagnostics.iter().any(|d| d
                    .message
                    .contains("cannot manufacture built-in capabilities")),
                "{}",
                report.to_json()
            );
        }
    }
    for main in [
        "import model;fn keep<T>(x:T)->T where T:model.User_Send=>x;fn main()->i32=>0",
        "import model;fn keep<T:model.User_Send>(x:T)->T=>x;fn main()->i32=>0",
    ] {
        let report = project_check(
            "near-reserved",
            &[("main", main), ("model", "pub trait User_Send{}")],
        );
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn nominal_grouping_matches_inline_at_source_and_project_entries() {
    for group in ["Read", "(Read)", "((Read))"] {
        let prefix =
            "trait Read{fn get(self:&Self)->i32;}struct C{n:i32}impl Read for C{fn get()->i32=>.n}";
        let suffix = "fn main()->i32{let c=C{n:7};return read(&c)-7}";
        let inline = format!("{prefix}fn read<T:{group}>(x:&T)->i32=>x.get();{suffix}");
        let where_form = format!("{prefix}fn read<T>(x:&T)->i32 where T:{group}=>x.get();{suffix}");
        accepted("grouped-inline", &inline);
        accepted("grouped-where", &where_form);
    }
    let model = "pub trait Read{fn get(self:&Self)->i32;}pub struct C{n:i32}impl Read for C{fn get()->i32=>.n}pub fn make()->C=>C{n:7}";
    for main in [
        "import model;fn read<T:(model.Read)>(x:&T)->i32=>x.get();fn main()->i32{let c=model.make();return read(&c)-7}",
        "import model;fn read<T>(x:&T)->i32 where T:(model.Read)=>x.get();fn main()->i32{let c=model.make();return read(&c)-7}",
    ] {
        let report = project_check("grouped-project", &[("main", main), ("model", model)]);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
    }
}

#[test]
fn raw_ast_respects_the_exact_module_prefix_boundary() {
    use zil_syntax::ast::{GenericParamKind, ItemKind, TypeKind};
    let source = source("trait Marker{}fn keep<T:Marker>(x:T)->T=>x;fn main()->i32=>0");
    for (name, reserved) in [
        ("$module0_Send", true),
        ("$module42_FnOnce", true),
        ("$module_Send", false),
        ("$modulex_Send", false),
        ("$module1x_Send", false),
        ("$module7_User_Send", false),
        ("User_Send", false),
    ] {
        let mut syntax = zil_syntax::parse(&source).into_result().unwrap();
        for item in &mut syntax.items {
            match &mut item.kind {
                ItemKind::Trait(declaration) => declaration.name = name.to_string(),
                ItemKind::Function(function) if function.name == "keep" => {
                    let GenericParamKind::Type(bounds) = &mut function.generics[0].kind else {
                        unreachable!()
                    };
                    let TypeKind::Named { path, .. } = &mut bounds[0].kind else {
                        unreachable!()
                    };
                    path[0] = name.to_string();
                }
                _ => {}
            }
        }
        let result = zil_expand::expand_with_map_limits(&source, &syntax, &Limits::default());
        if reserved {
            let errors = result.unwrap_err();
            assert!(
                errors.iter().any(|error| error
                    .message
                    .contains("cannot manufacture built-in capabilities")),
                "{name}: {errors:?}"
            );
        } else {
            assert!(result.is_ok(), "{name}: {result:?}");
        }
    }
}

fn minimum(nodes: bool) -> usize {
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
        let n = low + (high - low) / 2;
        let mut options = CompileOptions {
            stop_after: Stage::Core,
            ..CompileOptions::default()
        };
        if nodes {
            options.limits.max_nodes = n;
        } else {
            options.limits.max_analysis_steps = n;
        }
        let report = CompilationSession::default().compile_source(&source(BUDGET), &options);
        match report.status {
            ResultCategory::Success => high = n,
            ResultCategory::ResourceLimit => low = n,
            _ => panic!("{}", report.to_json()),
        }
    }
    high
}

#[test]
fn nominal_where_and_alias_work_is_bounded_on_fresh_and_cached_requests() {
    let directory = output("budgets");
    fs::write(directory.join("source.zil"), BUDGET).unwrap();
    let mut costs = String::from("dimension,minimum\n");
    for nodes in [false, true] {
        let exact = minimum(nodes);
        costs.push_str(&format!("{nodes},{exact}\n"));
        let mut session = CompilationSession::with_capacity(64);
        for (step, n) in [exact, exact, exact - 1, exact].into_iter().enumerate() {
            let mut options = CompileOptions {
                stop_after: Stage::Core,
                ..CompileOptions::default()
            };
            if nodes {
                options.limits.max_nodes = n;
            } else {
                options.limits.max_analysis_steps = n;
            }
            let report = session.compile_source(&source(BUDGET), &options);
            fs::write(
                directory.join(format!("{nodes}-{step}.json")),
                report.to_json(),
            )
            .unwrap();
            assert_eq!(
                report.status,
                if step == 2 {
                    ResultCategory::ResourceLimit
                } else {
                    ResultCategory::Success
                },
                "{}",
                report.to_json()
            );
            assert_eq!(session.active_query_count(), 0);
            if step == 1 || step == 3 {
                assert!(report.cache_hit, "{}", report.to_json());
            }
        }
        let mut options = CompileOptions {
            stop_after: Stage::Core,
            ..CompileOptions::default()
        };
        if nodes {
            options.limits.max_nodes = exact;
        } else {
            options.limits.max_analysis_steps = exact;
        }
        let fresh = CompilationSession::default().compile_source(&source(BUDGET), &options);
        assert_eq!(fresh.status, ResultCategory::Success, "{}", fresh.to_json());
    }
    fs::write(directory.join("costs.csv"), costs).unwrap();
}

#[test]
fn module_bound_resolution_obeys_exact_fresh_and_cached_limits() {
    let directory = output("module-budgets");
    for (name, text) in PROJECT {
        fs::write(directory.join(format!("{name}.zil")), text).unwrap();
    }
    let input = project(PROJECT);
    let defaults = Limits::default();
    let mut costs = String::from("dimension,minimum\n");
    for nodes in [false, true] {
        let (mut low, mut high) = (
            0,
            if nodes {
                defaults.max_nodes
            } else {
                defaults.max_analysis_steps
            },
        );
        while high - low > 1 {
            let n = low + (high - low) / 2;
            let mut options = CompileOptions {
                stop_after: Stage::Core,
                ..CompileOptions::default()
            };
            if nodes {
                options.limits.max_nodes = n;
            } else {
                options.limits.max_analysis_steps = n;
            }
            let report = CompilationSession::default().compile_project(&input, &options);
            match report.status {
                ResultCategory::Success => high = n,
                ResultCategory::ResourceLimit => low = n,
                _ => panic!("{}", report.to_json()),
            }
        }
        costs.push_str(&format!("{nodes},{high}\n"));
        let mut session = CompilationSession::with_capacity(64);
        for (step, n) in [high, high, high - 1, high].into_iter().enumerate() {
            let mut options = CompileOptions {
                stop_after: Stage::Core,
                ..CompileOptions::default()
            };
            if nodes {
                options.limits.max_nodes = n;
            } else {
                options.limits.max_analysis_steps = n;
            }
            let report = session.compile_project(&input, &options);
            fs::write(
                directory.join(format!("{nodes}-{step}.json")),
                report.to_json(),
            )
            .unwrap();
            assert_eq!(
                report.status,
                if step == 2 {
                    ResultCategory::ResourceLimit
                } else {
                    ResultCategory::Success
                },
                "{}",
                report.to_json()
            );
            assert_eq!(session.active_query_count(), 0);
            if step == 1 || step == 3 {
                assert!(report.cache_hit, "{}", report.to_json());
            }
        }
        let mut options = CompileOptions {
            stop_after: Stage::Core,
            ..CompileOptions::default()
        };
        if nodes {
            options.limits.max_nodes = high;
        } else {
            options.limits.max_analysis_steps = high;
        }
        let report = CompilationSession::default().compile_project(&input, &options);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        fs::write(
            directory.join(format!("{nodes}-fresh.json")),
            report.to_json(),
        )
        .unwrap();
    }
    fs::write(directory.join("costs.csv"), costs).unwrap();
}

#[test]
fn direct_expand_api_checks_nominal_where_without_driver_normalization() {
    let text = "trait Read{fn get(self:&Self)->i32;}fn read<T>(x:&T)->i32 where T:Read=>x.get();fn main()->i32=>0";
    let source = source(text);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let result = zil_expand::expand_with_map_limits(&source, &syntax, &Limits::default());
    assert!(result.is_ok(), "{result:?}");
    let bad = "trait Read{fn get(self:&Self)->i32;}fn read<T>(x:T)->(T,T) where T:Read=>(x,x);fn main()->i32=>0";
    let source = SourceUnit::new(FileId(0), ModuleId(0), "raw231.zil", bad);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let errors =
        zil_expand::expand_with_map_limits(&source, &syntax, &Limits::default()).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("used after move")),
        "{errors:?}"
    );
}
