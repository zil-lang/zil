//! Complete ordinary type binding keeps the original failed source bytes.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{FileId, ModuleId, ResultCategory, SourceUnit};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Case(PathBuf, bool);
impl Case {
    fn new(label: &str) -> Self {
        let retained = std::env::var_os("ZIL_TEST_REPORT_DIR");
        let root = retained
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let path = root.join(format!(
            "store202-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn compile(case: &Case, source: &str, stage: Stage, optimization: Optimization) -> CompileReport {
    fs::write(case.0.join("main.zil"), source).unwrap();
    let report = compile_project(
        &ProjectSources {
            root: ModuleId(0),
            modules: vec![SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source)],
            paths: Default::default(),
        },
        &CompileOptions {
            stop_after: stage,
            optimization,
            output: (stage == Stage::Build).then(|| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
    report
}
fn native(label: &str, source: &str, stdout: &str, exit: i32) {
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Stage::Build, optimization);
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        let output = Command::new(&report.artifacts["executable"])
            .output()
            .unwrap();
        fs::write(case.0.join("stdout.log"), &output.stdout).unwrap();
        fs::write(case.0.join("stderr.log"), &output.stderr).unwrap();
        fs::write(case.0.join("native.txt"), format!("optimization={optimization:?}\nexit={:?}\nexpected_exit={exit}\nexpected_stdout={stdout:?}\n", output.status.code())).unwrap();
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, stdout.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty(), "{label}: {output:?}");
    }
}

const PUSH: &str = include_str!("impl202_fixtures/01_original_push.zil");
const FACTORY: &str = include_str!("impl202_fixtures/02_original_factory.zil");

#[test]
fn original_push_preserves_complete_t() {
    native("original-push", PUSH, "1A2BZ", 0);
}
#[test]
fn original_factory_and_reserve_keep_the_same_nominal_t() {
    native("original-factory", FACTORY, "1A2BZ", 0);
}

const IDENTITY: &str = r#"
resource Empty {}
package Item<F> { manages root: Empty; callback: F }
fn identity<T: StoreElement>(value: T) -> T { return value }
fn make() -> impl Fn() -> i32 { return move () => 7 }
fn pack<F: Fn() -> i32>(callback: F) -> Item<F> {
    return Item::<F> { root: Empty {}, callback: callback }
}
fn run<F: Fn() -> i32>(value: Item<F>) -> i32 {
    let result = identity(value)
    return result.callback() - 7
}
fn main() -> i32 { return run(pack(make())) }
"#;

fn typed(label: &str, source: &str) -> zil_ir::TypedModule {
    let case = Case::new(label);
    fs::write(case.0.join("main.zil"), source).unwrap();
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "main.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let core = zil_expand::expand(&unit, &syntax).unwrap();
    fs::write(case.0.join("core.txt"), format!("{core:#?}")).unwrap();
    let module = zil_types::check(
        &core,
        &zil_base::TargetProfile::host_llvm(),
        &zil_base::Limits::default(),
    );
    fs::write(case.0.join("typed.txt"), format!("{module:#?}")).unwrap();
    module.unwrap()
}

#[test]
fn identity_without_store_still_carries_independently_checked_element_qualification() {
    // This input never constructs or borrows a Store. A zero-sized Managed
    // field and an empty closure together have zero actual element stride.
    let module = typed("identity-zero-carrier", IDENTITY);
    assert_eq!(
        module
            .resources
            .iter()
            .filter(|r| matches!(r.payload, zil_ir::Type::OwnedStore { .. }))
            .count(),
        1
    );
    assert!(
        !module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.kind, zil_ir::InstructionKind::OwnedStoreNew { .. }))
    );
    let case = Case::new("identity-zero-reject");
    let report = compile(&case, IDENTITY, Stage::Verify, Optimization::O0);
    assert_eq!(
        report.status,
        ResultCategory::Unsupported,
        "{}",
        report.to_json()
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.phase == zil_base::Phase::Verify
                && d.message.contains("nonzero actual inline layout")),
        "{}",
        report.to_json()
    );
    let nonzero = IDENTITY.replace(
        "return move () => 7",
        "let seed = 7; return move () => seed",
    );
    let module = typed("identity-nonzero-carrier", &nonzero);
    assert_eq!(
        module
            .resources
            .iter()
            .filter(|r| matches!(r.payload, zil_ir::Type::OwnedStore { .. }))
            .count(),
        1
    );
    native("identity-nonzero", &nonzero, "", 0);
}

#[test]
fn ordinary_template_may_call_an_independent_factory_and_callable_helper() {
    let source = format!(
        r#"
fn ping() -> impl Fn() -> i32 {{ return move () => 7 }}
fn invoke<F: Fn() -> i32>(callback: &F) -> i32 {{ return callback() }}
{}"#,
        FACTORY.replace(
            "return store_reserve(items, count)",
            "let cb = ping(); invoke(&cb); return store_reserve(items, count)"
        )
    );
    native("mixed-plan", &source, "1A2BZ", 0);
}

#[test]
fn source_inference_keeps_a_type_application_in_a_factory_using_caller() {
    let source = format!(
        r#"
fn ping() -> impl Fn() -> i32 {{ return move () => 7 }}
fn invoke<F: Fn() -> i32>(callback: &F) -> i32 {{ return callback() }}
{}"#,
        FACTORY.replace(
            "fn run<F: FnMut() -> i32>(pair: Pair<F>) -> i32 {",
            "fn run<F: FnMut() -> i32>(pair: Pair<F>) -> i32 { let cb = ping(); invoke(&cb);"
        )
    );
    native("typeapply-source-inference", &source, "1A2BZ", 0);
}

#[test]
fn ordinary_binders_forward_alpha_renamed_complete_type_arguments() {
    let source = FACTORY.replace("fn reserve<T: StoreElement>(items: Store<T>, count: i64) -> bool { return store_reserve(items, count) }", r#"
fn child<T: StoreElement>(items: Store<T>, count: i64) -> bool { return store_reserve(items, count) }
fn reserve<U: StoreElement>(items: Store<U>, count: i64) -> bool { return child::<U>(items, count) }
"#);
    native("ordinary-alpha-forward", &source, "1A2BZ", 0);
}

#[test]
fn pop_template_success_retains_its_owned_callback_for_a_nested_helper() {
    let prefix = &FACTORY[..FACTORY.find("fn run<").unwrap()];
    let source = format!(
        r#"{prefix}
fn invoke<F: FnMut() -> i32>(callback: &mut F) -> i32 {{ return callback() }}
fn run<F: FnMut() -> i32>(pair: Pair<F>) -> void!i32 {{
    let items: Store<Item<i32,F>> = empty()
    if !reserve(items,2) {{ return () }}
    push(items, Item::<i32,F>{{root:Ticket{{code:49}},callback:pair.first,value:10}})
    push(items, Item::<i32,F>{{root:Ticket{{code:50}},callback:pair.second,value:20}})
    var last = pop(items)?
    return invoke(&mut last.callback) - 21
}}
fn main() -> void!i32 {{ return run(pair(make(65,10),make(66,20))) }}
"#
    );
    native("pop-then-nested-helper", &source, "2B1AZ", 0);
    let annotated = source.replace(
        "var last = pop(items)?",
        "let pending: void!Item<i32,F> = pop(items); var last = pending?",
    );
    native("pop-annotated-then-nested-helper", &annotated, "2B1AZ", 0);
    let wrong_side = annotated.replace("void!Item<i32,F>", "Item<i32,F>!void");
    let case = Case::new("pop-wrong-annotated-side");
    let report = compile(&case, &wrong_side, Stage::Verify, Optimization::O0);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    assert_eq!(report.verification_runs, 0, "{}", report.to_json());
}

#[test]
fn five_module_consumer_preserves_pop_success_and_push_error_responsibility() {
    use std::{io::Write, process::Stdio};
    let sources = [
        (
            "main",
            include_str!("../../../examples/mainline/store_template202/main.zil"),
        ),
        (
            "factory",
            include_str!("../../../examples/mainline/store_template202/factory.zil"),
        ),
        (
            "state",
            include_str!("../../../examples/mainline/store_template202/state.zil"),
        ),
        (
            "portable",
            include_str!("../../../examples/mainline/store_template202/portable.zil"),
        ),
        (
            "store_helpers",
            include_str!("../../../examples/mainline/store_template202/store_helpers.zil"),
        ),
    ];
    let project = ProjectSources {
        root: ModuleId(0),
        modules: sources
            .iter()
            .enumerate()
            .map(|(i, (name, source))| {
                SourceUnit::new(
                    FileId(i as u32),
                    ModuleId(i as u32),
                    format!("{name}.zil"),
                    *source,
                )
            })
            .collect(),
        paths: sources
            .iter()
            .enumerate()
            .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
            .collect(),
    };
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("five-module");
        for (name, source) in &sources {
            fs::write(case.0.join(format!("{name}.zil")), source).unwrap();
        }
        let report = compile_project(
            &project,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization,
                output: Some(case.0.join("program")),
                emit_dir: Some(case.0.join("phases")),
                ..CompileOptions::default()
            },
        );
        fs::write(case.0.join("compile.json"), report.to_json()).unwrap();
        assert_eq!(
            report.status,
            ResultCategory::Success,
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 1);
        for (input, expected) in [(b'0', "2b1aZ"), (b'1', "ABBAB2b1aZ"), (b'6', "2b1aZ")] {
            let mut child = Command::new(&report.artifacts["executable"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(&[input]).unwrap();
            let output = child.wait_with_output().unwrap();
            fs::write(
                case.0.join(format!("input-{input}-stdout.log")),
                &output.stdout,
            )
            .unwrap();
            fs::write(
                case.0.join(format!("input-{input}-stderr.log")),
                &output.stderr,
            )
            .unwrap();
            fs::write(
                case.0.join(format!("input-{input}-native.txt")),
                format!(
                    "optimization={optimization:?}\nexit={:?}\nexpected_stdout={expected:?}\n",
                    output.status.code()
                ),
            )
            .unwrap();
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert_eq!(output.stdout, expected.as_bytes(), "{output:?}");
            assert!(output.stderr.is_empty(), "{output:?}");
        }
    }
}
