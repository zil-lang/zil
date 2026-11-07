//! Component preparation helpers compose with self-contained returned handlers.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{FileId, Limits, ModuleId, ResultCategory, SourceUnit, json_string};
use zil_driver::{
    CompilationSession, CompileOptions, CompileReport, Optimization, ProjectSources, Stage,
    compile_project,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(name: &str) -> Self {
        let keep = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = keep
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "defaults134-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path, keep.is_some())
    }
    fn write(&self, name: &str, data: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), data).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
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
            .collect::<BTreeMap<_, _>>(),
    }
}
fn compile(
    input: &ProjectSources,
    case: &Case,
    optimization: Option<Optimization>,
) -> CompileReport {
    let native_main = input
        .modules
        .iter()
        .find(|unit| unit.module_id == input.root)
        .unwrap()
        .text
        .contains("fn main(");
    for unit in &input.modules {
        case.write(&format!("module{}.zil", unit.module_id.0), &unit.text);
    }
    let report = compile_project(
        input,
        &CompileOptions {
            stop_after: if optimization.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: optimization.unwrap_or(Optimization::O0),
            entry: if native_main {
                None
            } else {
                Some("value".into())
            },
            output: optimization.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}
fn success(report: &CompileReport) {
    assert_eq!(
        report.status,
        ResultCategory::Success,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 1, "{}", report.to_json());
}
fn native(input: &ProjectSources, label: &str, expected: &str, code: i32) {
    for opt in [Optimization::O0, Optimization::O2] {
        let case = Case::new(&format!("{label}-{opt:?}"));
        let report = compile(input, &case, Some(opt));
        success(&report);
        let path = &report.artifacts["executable"];
        case.write(
            "native-command.json",
            format!("[{}]\n", json_string(&path.to_string_lossy())),
        );
        let output = Command::new(path).output().unwrap();
        case.write("native.stdout", &output.stdout);
        case.write("native.stderr", &output.stderr);
        case.write("native.exit", format!("{:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(code), "{label}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

const IO: &str = "extern \"C\" fn putchar(n:i32)->i32; pub fn emit(n:i32){unsafe{putchar(n)}}";
#[test]
fn defining_scope_and_private_helper_not_callers_same_named_function() {
    native(
        &project(&[
            (
                "main",
                "import config\nfn seed()->i64{return 100}\nfn value()->i64{let a=config.Config{};return a.x+a.y}",
            ),
            (
                "config",
                "fn seed()->i64{return 20}\npub struct Config{pub x:i64=seed();pub y:i64=22;}",
            ),
        ]),
        "scope",
        "42\n",
        0,
    );
}
#[test]
fn explicit_fields_then_defaults_in_declaration_order_and_update_does_not_repeat() {
    native(
        &project(&[
            (
                "main",
                "import config\nimport io\nfn explicit()->i64{io.emit(69);return 2}\nfn value()->i64{let a=config.Config{c:explicit()};let b=config.Config{...a,c:3};return b.a+b.b+b.c}",
            ),
            (
                "config",
                "import io\nfn first()->i64{io.emit(65);return 20}\nfn second()->i64{io.emit(66);return 19}\npub struct Config{pub a:i64=first();pub b:i64=second();pub c:i64=100;}",
            ),
            ("io", IO),
        ]),
        "order",
        "EAB42\n",
        0,
    );
}
#[test]
fn multiple_callers_use_owning_module_default() {
    native(
        &project(&[
            (
                "main",
                "import config\nimport consumer\nfn value()->i64{let a=config.Cell{x:20};return a.n+consumer.get()}",
            ),
            ("config", "pub struct Cell{pub x:i64;pub n:i64=21;}"),
            (
                "consumer",
                "import config\npub fn get()->i64{let a=config.Cell{x:1};return a.n}",
            ),
        ]),
        "generic",
        "42\n",
        0,
    );
}
#[test]
fn inferred_nominal_context_and_repeated_construction() {
    native(
        &project(&[
            (
                "main",
                "import config\nfn value()->i64{let a:config.Config=.{};let b:config.Config=.{};return a.x+b.x}",
            ),
            ("config", "pub struct Config{pub x:i64=21;}"),
        ]),
        "inferred",
        "42\n",
        0,
    );
}
#[test]
fn field_privacy_and_private_helper_stay_rejected() {
    for (main, library) in [
        (
            "import config\nfn value()->i64{let a=config.Config{};return 42}",
            "pub struct Config{x:i64=42;}",
        ),
        (
            "import config\nfn value()->i64{return config.seed()}",
            "fn seed()->i64{return 42}\npub struct Config{pub x:i64=seed();}",
        ),
    ] {
        let input = project(&[("main", main), ("config", library)]);
        let case = Case::new("private");
        let report = compile(&input, &case, None);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(!report.artifacts.contains_key("executable"));
    }
}
#[test]
fn invalid_unused_public_default_cannot_hide_behind_explicit_field() {
    let input = project(&[
        (
            "main",
            "import config\nfn value()->i64{let a=config.Config{x:42};return a.x}",
        ),
        ("config", "pub struct Config{pub x:i64=missing();}"),
    ]);
    let case = Case::new("invalid");
    let report = compile(&input, &case, None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}

#[test]
fn self_static_method_default_stays_in_definition_scope() {
    native(
        &project(&[
            (
                "main",
                "import config\nfn value()->i64{let a=config.Config{};return a.x}",
            ),
            (
                "config",
                "pub struct Config{pub x:i64=Self.seed();}\nimpl Config{fn seed()->i64{return 42}}",
            ),
        ]),
        "self",
        "42\n",
        0,
    );
}
#[test]
fn generic_defaults_still_require_symbolic_checking() {
    let input = project(&[
        (
            "main",
            "import config\nfn value()->i64{let a=config.Cell::<i64>{};return a.x}",
        ),
        ("config", "pub struct Cell<T>{pub x:T=42;}"),
    ]);
    let case = Case::new("generic-boundary");
    let report = compile(&input, &case, None);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(report.diagnostics[0].message.contains("symbolic default"));
}

#[test]
fn self_value_binders_inside_defaults_are_not_nominal_namespaces() {
    native(
        &project(&[
            (
                "main",
                "import config\nfn value()->i64{let a=config.Config{};return a.x}",
            ),
            (
                "config",
                "struct Pair{v:i64}pub struct Config{pub x:i64=((Self:Pair)=>Self.v)(Pair{v:42});}",
            ),
        ]),
        "self-shadow",
        "42\n",
        0,
    );
}
#[test]
fn defaults_cannot_read_unfinished_receiver() {
    let input = project(&[
        (
            "main",
            "import config\nfn value()->i64{let a=config.Config{};return a.x}",
        ),
        ("config", "pub struct Config{pub x:i64=.y;pub y:i64=1;}"),
    ]);
    let case = Case::new("unfinished");
    let report = compile(&input, &case, None);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert!(!report.artifacts.contains_key("executable"));
}
#[test]
fn module_order_and_equal_field_names_do_not_alias_helpers() {
    let mut input = project(&[
        (
            "main",
            "import first\nimport second\nfn value()->i64{let a=first.Cell{};let b=second.Cell{};return a.x+b.x}",
        ),
        ("first", "pub struct Cell{pub x:i64=20;}"),
        ("second", "pub struct Cell{pub x:i64=22;}"),
    ]);
    native(&input, "order-forward", "42\n", 0);
    input.modules.reverse();
    native(&input, "order-reverse", "42\n", 0);
}
#[test]
fn cached_private_default_edit_and_budget_failure_do_not_reuse_old_results() {
    let case = Case::new("session");
    case.write(
        "main.zil",
        "import config\nfn value()->i64{let a=config.Config{};return a.x}",
    );
    let path = case.0.join("main.zil");
    let mut session = CompilationSession::with_capacity(128);
    for (i, value) in [42, 42, 43, 42].into_iter().enumerate() {
        case.write(
            "config.zil",
            format!("fn seed()->i64{{return {value}}}pub struct Config{{pub x:i64=seed();}}"),
        );
        let options = CompileOptions {
            stop_after: Stage::Build,
            entry: Some("value".into()),
            output: Some(case.0.join(format!("run{i}"))),
            ..CompileOptions::default()
        };
        let report = session.compile_path(&path, &options);
        case.write(&format!("report{i}.json"), report.to_json());
        success(&report);
        assert_eq!(
            Command::new(&report.artifacts["executable"])
                .output()
                .unwrap()
                .stdout,
            format!("{value}\n").as_bytes()
        );
        assert_eq!(session.active_query_count(), 0);
        let low = CompileOptions {
            limits: Limits {
                max_analysis_steps: 1,
                ..Limits::default()
            },
            ..options.clone()
        };
        let rejected = session.compile_path(&path, &low);
        assert_eq!(
            rejected.status,
            ResultCategory::ResourceLimit,
            "{}",
            rejected.to_json()
        );
        assert_eq!(session.active_query_count(), 0);
        assert!(!rejected.artifacts.contains_key("executable"));
    }
}
