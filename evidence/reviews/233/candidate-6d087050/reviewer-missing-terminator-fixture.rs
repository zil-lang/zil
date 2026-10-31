//! Non-author cross-module nominal trait and real method-dispatch probes.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};
use zil_base::{FileId, ModuleId, Phase, ResultCategory, SourceUnit};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, NativeBackend, Optimization, ProjectSources,
    Stage,
};

const ROOT: &str =
    "import rules;import data;fn main()->i32{let value=data.make(42);return rules.read(&value)-42}";
const RULES: &str =
    "pub trait Read{fn value(self:&Self)->i32;}pub fn read<T>(x:&T)->i32 where T:Read=>x.value()";
const DATA: &str = "import rules;pub struct Item{n:i32}impl rules.Read for Item{fn value(self:&Self)->i32=>self.n}pub fn make(n:i32)->Item=>Item{n:n}";
static RUN: OnceLock<String> = OnceLock::new();

fn directory(label: &str) -> PathBuf {
    let path = std::env::var_os("ZIL_TEST_REPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(RUN.get_or_init(|| {
            format!(
                "review233-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )
        }))
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
    let out = directory(label);
    for (name, source) in files {
        fs::write(out.join(format!("{name}.zil")), source).unwrap();
    }
    let report = CompilationSession::default().compile_project(
        &project(files),
        &CompileOptions {
            emit_dir: Some(out.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(out.join("compile.json"), report.to_json()).unwrap();
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

fn reject(label: &str, files: &[(&str, &str)], code: Option<&str>) {
    let report = check(label, files);
    assert!(
        matches!(
            report.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{label}: {}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
    assert_eq!(report.verification_runs, 0, "{label}: {}", report.to_json());
    if let Some(code) = code {
        assert_eq!(report.phase, Phase::Expand, "{label}: {}", report.to_json());
        assert!(
            report.diagnostics.iter().any(|d| d.code == code),
            "{label}: {}",
            report.to_json()
        );
    }
}

fn native(label: &str, files: &[(&str, &str)], stdout: &str) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let out = directory(&format!("{label}-{optimization:?}"));
        for (name, source) in files {
            fs::write(out.join(format!("{name}.zil")), source).unwrap();
        }
        let backend = if cfg!(feature = "llvm-api") {
            NativeBackend::LlvmApi
        } else {
            NativeBackend::Clang
        };
        let report = CompilationSession::default().compile_project(
            &project(files),
            &CompileOptions {
                stop_after: Stage::Build,
                backend,
                optimization,
                emit_dir: Some(out.join("phases")),
                output: Some(out.join("program")),
                ..CompileOptions::default()
            },
        );
        fs::write(out.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{label}: {}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let codegen = report.native_codegen.as_ref().unwrap();
        assert_eq!(codegen.backend, backend);
        assert!(codegen.fallback.is_none());
        let result = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(out.join("stdout"), &result.stdout).unwrap();
        fs::write(out.join("stderr"), &result.stderr).unwrap();
        fs::write(
            out.join("exit.txt"),
            format!("{:?}\n", result.status.code()),
        )
        .unwrap();
        assert_eq!(result.status.code(), Some(0), "{label}: {result:?}");
        assert_eq!(result.stdout, stdout.as_bytes(), "{label}: {result:?}");
        assert!(result.stderr.is_empty(), "{label}: {result:?}");
    }
}

#[test]
fn split_trait_impl_uses_definition_module_and_both_bound_spellings() {
    for (form, rules) in [
        ("where", RULES.to_owned()),
        (
            "inline",
            RULES.replace("read<T>(x:&T)->i32 where T:Read", "read<T:Read>(x:&T)->i32"),
        ),
    ] {
        let files = [("main", ROOT), ("rules", &rules), ("data", DATA)];
        accept(&format!("split-{form}"), &files);
        native(&format!("split-{form}"), &files, "");
        let mut reordered = project(&files);
        reordered.modules.reverse();
        let report =
            CompilationSession::default().compile_project(&reordered, &CompileOptions::default());
        fs::write(
            directory(&format!("reordered-{form}")).join("compile.json"),
            report.to_json(),
        )
        .unwrap();
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
fn two_local_self_types_and_same_spelling_traits_keep_nominal_identity() {
    let root = "import rules;import data;import second;fn main()->i32{let first=data.make(7);let other=second.make(35);return rules.read(&first)+rules.read(&other)-42}";
    native(
        "two-impl-modules",
        &[
            ("main", root),
            ("rules", RULES),
            ("data", DATA),
            ("second", DATA),
        ],
        "",
    );
    let wrong = DATA
        .replace("import rules", "import other")
        .replace("impl rules.Read", "impl other.Read");
    reject(
        "lookalike-trait",
        &[
            ("main", ROOT),
            ("rules", RULES),
            ("data", &wrong),
            ("other", RULES),
        ],
        None,
    );
}

#[test]
fn separate_generic_library_uses_each_actual_method_body() {
    let root = "import facade;import data;import second;fn main()->i32{let first=data.make(7);let other=second.make(35);return facade.read(&first)+facade.read(&other)-142}";
    let facade = "import rules;pub fn read<T>(x:&T)->i32 where T:rules.Read=>x.value()";
    let second = DATA.replace("=>self.n", "=>self.n+100");
    let files = [
        ("main", root),
        ("rules", RULES),
        ("data", DATA),
        ("second", &second),
        ("facade", facade),
    ];
    accept("separate-library-distinct-implementations", &files);
    native("separate-library-distinct-implementations", &files, "");
}

#[test]
fn generated_dependency_cannot_authorize_an_unimported_source_trait() {
    let root = "import facade;import data;fn main()->i32{let value=data.make(42);return facade.read(&value)-42}";
    let facade = "import rules;pub fn read<T>(x:&T)->i32 where T:rules.Read=>x.value() struct Local{n:i32}impl data.Hidden for Local{fn hidden(self:&Self)->i32=>self.n}";
    let data = format!("{DATA} pub trait Hidden{{fn hidden(self:&Self)->i32;}}");
    // The actual read<Item> instance needs a generated dependency on data.
    // That compiler dependency must not legalize the unrelated source impl's
    // missing `import data`, including when that impl is never called.
    reject(
        "generated-import-is-not-source-authorization",
        &[
            ("main", root),
            ("rules", RULES),
            ("data", &data),
            ("facade", facade),
        ],
        None,
    );
    // Adding the actual source import establishes that the only missing
    // precondition above was source-level access, not malformed impl syntax.
    let imported = facade.replacen("import rules;", "import rules;import data;", 1);
    accept(
        "actual-source-import-authorizes-ordinary-trait",
        &[
            ("main", root),
            ("rules", RULES),
            ("data", &data),
            ("facade", &imported),
        ],
    );
}

#[test]
fn uncalled_bad_impls_cannot_escape_signature_method_set_or_collision_checks() {
    let root = "import data;fn main()->i32=>0";
    let rules = "pub trait Read{fn value(self:&Self)->i32;}";
    for (label, data, code) in [
        (
            "receiver",
            "import rules;pub struct Item{n:i32}impl rules.Read for Item{fn value(self:Self)->i32=>self.n}",
            Some("E-TRAIT-SIGNATURE"),
        ),
        (
            "result",
            "import rules;pub struct Item{n:i32}impl rules.Read for Item{fn value(self:&Self)->i64=>1}",
            Some("E-TRAIT-SIGNATURE"),
        ),
        (
            "missing",
            "import rules;pub struct Item{n:i32}impl rules.Read for Item{}",
            Some("E-TRAIT-METHOD-SET"),
        ),
        (
            "duplicate",
            "import rules;pub struct Item{n:i32}impl rules.Read for Item{fn value()->i32=>.n}impl rules.Read for Item{fn value()->i32=>.n}",
            Some("E-TRAIT-DUPLICATE"),
        ),
        (
            "extra-method",
            "import rules;pub struct Item{n:i32}impl rules.Read for Item{fn value()->i32=>.n;fn other()->i32=>1}",
            Some("E-TRAIT-METHOD-SET"),
        ),
        (
            "field-collision",
            "import rules;pub struct Item{value:i32}impl rules.Read for Item{fn value()->i32=>1}",
            None,
        ),
        (
            "inherent-collision",
            "import rules;pub struct Item{n:i32}impl Item{fn value(self:&Self)->i32=>.n}impl rules.Read for Item{fn value()->i32=>.n}",
            None,
        ),
    ] {
        reject(
            label,
            &[("main", root), ("rules", rules), ("data", data)],
            code,
        );
    }
}

#[test]
fn private_missing_unimported_traits_and_foreign_self_stay_closed() {
    for (label, rules, data, code) in [
        (
            "private-trait",
            "trait Read{fn value(self:&Self)->i32;}",
            DATA,
            Some("E-MODULE-PRIVATE"),
        ),
        (
            "missing-trait",
            "pub trait Other{}",
            DATA,
            Some("E-MODULE-MEMBER"),
        ),
        (
            "not-imported",
            "pub trait Read{fn value(self:&Self)->i32;}",
            "pub struct Item{n:i32}impl rules.Read for Item{fn value()->i32=>.n}",
            None,
        ),
        (
            "foreign-self",
            "pub trait Read{fn value(self:&Self)->i32;}pub struct Item{n:i32}",
            "import rules;impl rules.Read for rules.Item{fn value()->i32=>7}",
            None,
        ),
    ] {
        reject(
            label,
            &[
                ("main", "import data;fn main()->i32=>0"),
                ("rules", rules),
                ("data", data),
            ],
            code,
        );
    }
}

#[test]
fn renamed_reserved_traits_are_rejected_without_a_generic_bound() {
    for name in [
        "Copy",
        "StoreElement",
        "Transfer",
        "Share",
        "Send",
        "Sync",
        "Drop",
        "Fn",
        "FnMut",
        "FnOnce",
    ] {
        let rules = format!("pub trait {name}{{}}");
        reject(
            &format!("reserved-declaration-{name}"),
            &[
                ("main", "import rules;fn main()->i32=>0"),
                ("rules", &rules),
            ],
            None,
        );
        let data = format!("import rules;struct Item{{n:i32}}impl rules.{name} for Item{{}}");
        reject(
            &format!("reserved-{name}"),
            &[
                ("main", "import data;fn main()->i32=>0"),
                ("rules", &rules),
                ("data", &data),
            ],
            None,
        );
    }
    accept(
        "near-reserved",
        &[
            ("main", "import data;fn main()->i32=>0"),
            (
                "rules",
                "pub trait Sender{}pub trait Syncer{}pub trait Dropper{}",
            ),
            (
                "data",
                "import rules;struct Item{n:i32}impl rules.Sender for Item{}impl rules.Syncer for Item{}impl rules.Dropper for Item{}",
            ),
        ],
    );
}

#[test]
fn imported_signature_types_preserve_their_real_nominal_owner() {
    let root = "import data;fn main()->i32=>data.check()";
    let rules = "pub struct Stamp{n:i32}pub fn make(n:i32)->Stamp=>Stamp{n:n};pub fn get(x:&Stamp)->i32=>x.n;pub trait Read{fn value(self:&Self,x:Stamp)->i32;}";
    let data = "import rules;pub struct Item{n:i32}impl rules.Read for Item{fn value(x:rules.Stamp)->i32=>.n+rules.get(&x)}pub fn check()->i32{let item=Item{n:7};return item.value(rules.make(35))-42}";
    accept(
        "signature-owner",
        &[("main", root), ("rules", rules), ("data", data)],
    );
    native(
        "signature-owner",
        &[("main", root), ("rules", rules), ("data", data)],
        "",
    );
    let forged = data
        .replace("import rules;", "import rules;import other;")
        .replace("x:rules.Stamp", "x:other.Stamp");
    reject(
        "signature-same-layout-wrong-owner",
        &[
            ("main", "import data;fn main()->i32=>0"),
            ("rules", rules),
            ("data", &forged),
            ("other", "pub struct Stamp{n:i32}"),
        ],
        Some("E-TRAIT-SIGNATURE"),
    );
}

#[test]
fn shared_resource_receiver_runs_real_drop_and_rejects_owned_receiver() {
    let rules = "pub trait Read{fn value(self:&Self)->i32;}";
    let data = "import rules;extern \"C\" fn putchar(code:i32)->i32;resource Counter{n:i32}impl Drop for Counter{fn drop(self:Self){unsafe{putchar(65)}}}impl rules.Read for Counter{fn value()->i32=>.n}pub fn check()->i32{let counter=Counter{n:42};return counter.value()-42}";
    native(
        "resource-cleanup",
        &[
            ("main", "import data;fn main()->i32=>data.check()"),
            ("rules", rules),
            ("data", data),
        ],
        "A",
    );
    let owned = rules.replace("self:&Self", "self:Self");
    reject(
        "owned-resource-receiver",
        &[
            ("main", "import data;fn main()->i32=>0"),
            ("rules", &owned),
            ("data", data),
        ],
        None,
    );
}
