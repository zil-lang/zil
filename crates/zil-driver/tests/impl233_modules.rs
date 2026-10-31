//! Public imported nominal traits retain local Self, signature and privacy rules.
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization, ProjectSources,
    Stage, compile_source,
};

fn project(files: &[(&str, &str)]) -> ProjectSources {
    ProjectSources {
        root: ModuleId(0),
        modules: files
            .iter()
            .enumerate()
            .map(|(n, (name, text))| {
                SourceUnit::new(
                    FileId(n as u32),
                    ModuleId(n as u32),
                    format!("{name}.zil"),
                    *text,
                )
            })
            .collect(),
        paths: files
            .iter()
            .enumerate()
            .skip(1)
            .map(|(n, (name, _))| (vec![name.to_string()], ModuleId(n as u32)))
            .collect::<BTreeMap<_, _>>(),
    }
}

fn directory(label: &str) -> PathBuf {
    let dir = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(format!("impl233-{}", std::process::id()))
        .join(label);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn compile(label: &str, files: &[(&str, &str)], mut options: CompileOptions) -> CompileReport {
    let dir = directory(label);
    for (name, text) in files {
        fs::write(dir.join(format!("{name}.zil")), text).unwrap();
    }
    options.emit_dir = Some(dir.join("phases"));
    if matches!(options.stop_after, Stage::Build) {
        options.output = Some(dir.join("program"));
    }
    let mut session = CompilationSession::default();
    let report = session.compile_project(&project(files), &options);
    assert_eq!(session.active_query_count(), 0);
    fs::write(dir.join("report.json"), report.to_json()).unwrap();
    report
}

fn accepted(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}

fn rejected(label: &str, files: &[(&str, &str)], code: &str) {
    let report = compile(label, files, CompileOptions::default());
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
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0);
    assert!(!report.artifacts.contains_key("executable"));
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "{}",
        report.to_json()
    );
}

const MAIN: &str = "import rules;import data;fn read<T>(x:&T)->i32 where T:((rules.Code))=>x.code();fn main()->i32{let x=data.make();return read(&x)-13}";
const RULES: &str = "pub trait Code{fn code(self:&Self)->i32;}";
const DATA: &str = "import rules;pub struct Payload{n:i32}impl rules.Code for Payload{fn code(self:&Self)->i32=>self.n}pub fn make()->Payload=>Payload{n:13}";

#[test]
fn original_three_module_where_and_inline_inputs_reach_independent_verify() {
    let inline = MAIN.replace(
        "read<T>(x:&T)->i32 where T:((rules.Code))",
        "read<T:((rules.Code))>(x:&T)->i32",
    );
    for (label, main) in [
        ("original-where", MAIN),
        ("original-inline", inline.as_str()),
    ] {
        accepted(&compile(
            label,
            &[("main", main), ("rules", RULES), ("data", DATA)],
            CompileOptions::default(),
        ));
    }
    let implicit = DATA.replace("fn code(self:&Self)->i32=>self.n", "fn code()->i32=>.n");
    accepted(&compile(
        "implicit-receiver",
        &[("main", MAIN), ("rules", RULES), ("data", &implicit)],
        CompileOptions::default(),
    ));
}

const OPERATIONS: &str = include_str!("../../../examples/mainline/trait_modules233/operations.zil");
const COUNTERS: &str = include_str!("../../../examples/mainline/trait_modules233/counters.zil");
const USE_COUNTERS: &str = include_str!("../../../examples/mainline/trait_modules233/main.zil");

fn native(label: &str, files: &[(&str, &str)], stdout: &[u8]) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let report = compile(
            &format!("{label}-{optimization:?}"),
            files,
            CompileOptions {
                stop_after: Stage::Build,
                backend: if cfg!(feature = "llvm-api") {
                    NativeBackend::LlvmApi
                } else {
                    NativeBackend::Clang
                },
                optimization,
                ..CompileOptions::default()
            },
        );
        accepted(&report);
        assert!(report.native_codegen.as_ref().unwrap().fallback.is_none());
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout, "{label}: {output:?}");
        assert!(output.stderr.is_empty());
        let dir = directory(&format!("{label}-{optimization:?}"));
        fs::write(dir.join("stdout"), output.stdout).unwrap();
        fs::write(dir.join("stderr"), output.stderr).unwrap();
    }
}

#[test]
fn public_imported_traits_export_checked_shared_mutable_static_and_value_methods() {
    native(
        "counter-consumer",
        &[
            ("main", USE_COUNTERS),
            ("operations", OPERATIONS),
            ("counters", COUNTERS),
        ],
        b"",
    );
    native(
        "original-three-module",
        &[("main", MAIN), ("rules", RULES), ("data", DATA)],
        b"",
    );
}

#[test]
fn resource_implementations_keep_shared_heads_and_original_drop_responsibility() {
    let data = "import rules;extern \"C\" fn putchar(n:i32)->i32;resource Ticket{n:i32}impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.n)}}}impl rules.Code for Ticket{fn code()->i32=>.n}pub fn probe()->i32{let ticket=Ticket{n:65};return ticket.code()-65}";
    native(
        "resource-consumer",
        &[
            ("main", "import data;fn main()->i32=>data.probe()"),
            ("rules", RULES),
            ("data", data),
        ],
        b"A",
    );
    for (label, signature) in [
        ("resource-mutable", "fn code(self:&mut Self)->i32;"),
        ("resource-value", "fn code(self:Self)->i32;"),
    ] {
        rejected(
            label,
            &[
                ("main", "import data;fn main()->i32=>0"),
                ("rules", &format!("pub trait Code{{{signature}}}")),
                ("data", data),
            ],
            "E-UNSUPPORTED",
        );
    }
}

#[test]
fn static_trait_declarations_cannot_hide_reserved_capabilities_after_module_renaming() {
    for name in [
        "Transfer", "Share", "Send", "Sync", "Drop", "Fn", "FnMut", "FnOnce",
    ] {
        rejected(
            &format!("reserved-declaration-{name}"),
            &[
                ("main", "import model;fn main()->i32=>0"),
                ("model", &format!("pub trait {name}{{}}")),
            ],
            "E-TRAIT-RESERVED",
        );
        if !name.starts_with("Fn") {
            rejected(
                &format!("reserved-impl-{name}"),
                &[
                    ("main", "import model;fn main()->i32=>0"),
                    (
                        "model",
                        &format!(
                            "pub trait {name}{{}}pub struct Payload{{n:i32}}impl {name} for Payload{{}}"
                        ),
                    ),
                ],
                "E-TRAIT-RESERVED",
            );
        }
    }
    for name in ["User_Send", "Send_User", "FnOnce_Extra", "Module0_Send"] {
        accepted(&compile(
            &format!("near-name-{name}"),
            &[
                ("main", "import model;fn main()->i32=>0"),
                (
                    "model",
                    &format!(
                        "pub trait {name}{{}}pub struct Payload{{n:i32}}impl {name} for Payload{{}}"
                    ),
                ),
            ],
            CompileOptions::default(),
        ));
    }
}

#[test]
fn imported_impls_check_uncalled_method_sets_signatures_and_collisions() {
    for (label, implementation, code) in [
        ("missing", "", "E-TRAIT-METHOD-SET"),
        (
            "extra",
            "fn code()->i32=>.n;fn extra(){}",
            "E-TRAIT-METHOD-SET",
        ),
        (
            "duplicate",
            "fn code()->i32=>.n;fn code()->i32=>.n",
            "E-TRAIT-METHOD-SET",
        ),
        (
            "mutable-mismatch",
            "fn code(self:&mut Self)->i32=>.n",
            "E-TRAIT-SIGNATURE",
        ),
        (
            "value-mismatch",
            "fn code(self:Self)->i32=>.n",
            "E-TRAIT-SIGNATURE",
        ),
        ("result-mismatch", "fn code()->i64=>1", "E-TRAIT-SIGNATURE"),
        (
            "argument-mismatch",
            "fn code(n:i32)->i32=>n",
            "E-TRAIT-SIGNATURE",
        ),
    ] {
        let data = format!(
            "import rules;pub struct Payload{{n:i32}}impl rules.Code for Payload{{{implementation}}}"
        );
        rejected(
            label,
            &[
                ("main", "import data;fn main()->i32=>0"),
                ("rules", RULES),
                ("data", &data),
            ],
            code,
        );
    }
    for (label, body, code) in [
        (
            "overlap",
            "impl rules.Code for Payload{fn code()->i32=>.n}impl rules.Code for Payload{fn code()->i32=>.n}",
            "E-TRAIT-DUPLICATE",
        ),
        (
            "inherent-collision",
            "impl rules.Code for Payload{fn code()->i32=>.n}impl Payload{fn code(self:&Self)->i32=>.n}",
            "E-UNSUPPORTED",
        ),
    ] {
        let data = format!("import rules;pub struct Payload{{n:i32}}{body}");
        rejected(
            label,
            &[
                ("main", "import data;fn main()->i32=>0"),
                ("rules", RULES),
                ("data", &data),
            ],
            code,
        );
    }
    rejected(
        "field-collision",
        &[
            ("main", "import data;fn main()->i32=>0"),
            ("rules", RULES),
            (
                "data",
                "import rules;pub struct Payload{code:i32}impl rules.Code for Payload{fn code()->i32=>.code}",
            ),
        ],
        "E-RECEIVER-MEMBER-CONFLICT",
    );
}

#[test]
fn privacy_local_self_and_nominal_identity_remain_required() {
    rejected(
        "private-trait",
        &[
            ("main", "import data;fn main()->i32=>0"),
            ("rules", "trait Code{fn code(self:&Self)->i32;}"),
            ("data", DATA),
        ],
        "E-MODULE-PRIVATE",
    );
    rejected(
        "foreign-self",
        &[
            ("main", "import implementation;fn main()->i32=>0"),
            ("rules", RULES),
            ("data", "pub struct Payload{pub n:i32}"),
            (
                "implementation",
                "import rules;import data;impl rules.Code for data.Payload{fn code()->i32=>.n}",
            ),
        ],
        "E-UNSUPPORTED",
    );
    let local_trait = "pub trait Code{fn code(self:&Self)->i32;}pub struct Payload{n:i32}impl Code for Payload{fn code()->i32=>.n}pub fn make()->Payload=>Payload{n:13}";
    rejected(
        "same-spelling-wrong-trait",
        &[("main", MAIN), ("rules", RULES), ("data", local_trait)],
        "E-GENERIC-ARGUMENT",
    );
    let private_self = DATA.replace("pub struct Payload", "struct Payload");
    rejected(
        "private-self-method-export",
        &[
            (
                "main",
                "import data;fn main()->i32{let x=data.make();return x.code()-13}",
            ),
            ("rules", RULES),
            ("data", &private_self),
        ],
        "E-UNKNOWN-FIELD",
    );
}

#[test]
fn original_single_module_and_drop_work_budgets_remain_exact() {
    for (text, minimum) in [
        ("fn main()->i32=>0", 242),
        (
            "resource R{n:i32}impl Drop for R{fn drop(self:Self){}}fn main()->i32{let r=R{n:1};return 0}",
            1568,
        ),
        (
            "trait Code{fn code(self:&Self)->i32;}struct Payload{n:i32}impl Code for Payload{fn code()->i32=>.n}fn main()->i32{let x=Payload{n:13};return x.code()-13}",
            1432,
        ),
    ] {
        let unit = SourceUnit::new(FileId(0), ModuleId(0), "main.zil", text);
        for (steps, expected) in [
            (minimum, ResultCategory::Success),
            (minimum - 1, ResultCategory::ResourceLimit),
            (minimum, ResultCategory::Success),
        ] {
            let mut options = CompileOptions::default();
            options.limits.max_analysis_steps = steps;
            assert_eq!(compile_source(&unit, &options).status, expected);
        }
    }
    let files = [
        (
            "main",
            "import model;fn main()->i32{let x=model.make();return x.code()-13}",
        ),
        (
            "model",
            "pub trait Code{fn code(self:&Self)->i32;}pub struct Payload{n:i32}impl Code for Payload{fn code()->i32=>.n}pub fn make()->Payload=>Payload{n:13}",
        ),
    ];
    for (steps, expected) in [
        (1412, ResultCategory::Success),
        (1411, ResultCategory::ResourceLimit),
        (1412, ResultCategory::Success),
    ] {
        let mut options = CompileOptions::default();
        options.limits.max_analysis_steps = steps;
        assert_eq!(
            CompilationSession::default()
                .compile_project(&project(&files), &options)
                .status,
            expected
        );
    }
}

#[test]
fn new_imported_trait_work_has_exact_cold_warm_and_fresh_boundaries() {
    let input = project(&[
        ("main", USE_COUNTERS),
        ("operations", OPERATIONS),
        ("counters", COUNTERS),
    ]);
    let (mut low, mut high) = (1, 1_000_000);
    while low < high {
        let middle = low + (high - low) / 2;
        let mut options = CompileOptions::default();
        options.limits.max_analysis_steps = middle;
        let report = CompilationSession::default().compile_project(&input, &options);
        match report.status {
            ResultCategory::Success => high = middle,
            ResultCategory::ResourceLimit => low = middle + 1,
            _ => panic!("{}", report.to_json()),
        }
    }
    let mut session = CompilationSession::default();
    for (steps, expected) in [
        (low, ResultCategory::Success),
        (low - 1, ResultCategory::ResourceLimit),
        (low, ResultCategory::Success),
    ] {
        let mut options = CompileOptions::default();
        options.limits.max_analysis_steps = steps;
        let report = session.compile_project(&input, &options);
        assert_eq!(report.status, expected, "{}", report.to_json());
        assert_eq!(session.active_query_count(), 0);
        assert_eq!(
            CompilationSession::default()
                .compile_project(&input, &options)
                .status,
            expected
        );
    }
    fs::write(
        directory("budget").join("result.json"),
        format!("{{\"minimum_analysis_steps\":{low},\"cold_warm_fresh\":\"Pass\"}}\n"),
    )
    .unwrap();
}
