//! Factory-local callables compose with one independently traced external callback.
use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use zil_base::{
    FileId, Limits, ModuleId, Phase, ResultCategory, SourceUnit, TargetProfile, json_string,
};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
};
use zil_ir::{CallableExit, CallableKind};

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
            "localfactory127-{name}-{}-{}",
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

fn core(text: &str) -> zil_ir::CoreModule {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "localfactory127.zil", text);
    let ast = zil_syntax::parse(&source).into_result().unwrap();
    zil_expand::expand(&source, &ast).unwrap()
}
fn typed(text: &str) -> zil_ir::TypedModule {
    zil_types::check(&core(text), &TargetProfile::default(), &Limits::default()).unwrap()
}
fn trace(text: &str, kind: CallableKind, calls: &[usize]) {
    let module = typed(text);
    let contract = module
        .functions
        .iter()
        .find_map(|f| f.callable_contract.as_ref())
        .unwrap();
    assert_eq!(contract.kind, kind);
    assert_eq!(
        contract
            .traces
            .iter()
            .map(|t| t.calls.len())
            .collect::<Vec<_>>(),
        calls
    );
    assert!(
        contract
            .traces
            .iter()
            .all(|t| t.exit == CallableExit::Return && t.calls.iter().all(|k| *k == kind))
    );
}
const SHARED: &str = "fn make(seed:i64)->impl Fn(i64)->i64{return move (n:i64)=>seed+n}fn run<F:Fn(i64)->i64>(f:&F)->i64{let local=make(2);return f(local(3))}fn value()->i64{let f=make(37);return run(&f)}";
const ONCE_PREFIX: &str = "resource Token{n:i64}fn consume(give t:Token)->i64{return t.n}fn make(seed:i64)->impl FnOnce()->i64{let t=Token{n:seed};return move ()=>consume(give t)}";
const MUT_PREFIX: &str = "fn make(seed:i64)->impl FnMut(i64)->i64{var n=seed;return move (delta:i64)=>{n+=delta;return n}}";
#[test]
fn same_factory_same_hidden_identity_has_one_parameter_invocation() {
    trace(SHARED, CallableKind::Fn, &[1]);
    native(&project(&[("main", SHARED)]), "same-factory", "42\n", 0);
}
#[test]
fn different_local_kind_does_not_change_parameter_bound() {
    let source = format!(
        "{MUT_PREFIX}fn run<F:Fn(i64)->i64>(f:&F)->i64{{var local=make(2);let first=local(3);return f(first)+local(4)}}fn value()->i64{{let base:i64=30;let f=(n:i64)=>base+n;return run(&f)}}"
    );
    trace(&source, CallableKind::Fn, &[1]);
    native(&project(&[("main", &source)]), "different-kind", "44\n", 0);
}
#[test]
fn two_owned_same_factory_once_instances_each_consume_once() {
    let source = format!(
        "{ONCE_PREFIX}fn run<F:FnOnce()->i64>(f:F)->i64{{let local=make(2);return local()+f()}}fn value()->i64{{let f=make(40);return run(f)}}"
    );
    trace(&source, CallableKind::FnOnce, &[1]);
    native(&project(&[("main", &source)]), "two-once", "42\n", 0);
}
#[test]
fn same_mutable_factory_instances_keep_independent_state() {
    let source = format!(
        "{MUT_PREFIX}fn run<F:FnMut(i64)->i64>(f:&mut F)->i64{{var local=make(20);return f(1)+local(2)+f(3)+local(4)}}fn value()->i64{{var f=make(10);let result=run(&mut f);return result+f(0)}}"
    );
    trace(&source, CallableKind::FnMut, &[2]);
    native(&project(&[("main", &source)]), "two-mut", "87\n", 0);
}
#[test]
fn parameter_alias_retains_parameter_origin() {
    let source = SHARED.replace(
        "let local=make(2);return f(local(3))",
        "let alias=f;let local=make(2);return alias(local(3))",
    );
    trace(&source, CallableKind::Fn, &[1]);
    native(&project(&[("main", &source)]), "parameter-alias", "42\n", 0);
}
#[test]
fn four_modules_clean_called_unused_and_error_environments() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/mainline/local_factory_helper");
    let content: Vec<_> = ["main", "combine", "factory", "io"]
        .into_iter()
        .map(|name| {
            (
                name,
                fs::read_to_string(root.join(format!("{name}.zil"))).unwrap(),
            )
        })
        .collect();
    let normal_files: Vec<_> = content
        .iter()
        .map(|(name, text)| (*name, text.as_str()))
        .collect();
    let case = Case::new("consumer-interface");
    let report = compile(&project(&normal_files), &case, None);
    success(&report);
    let interface = report
        .scalar_interfaces
        .iter()
        .find(|i| i.module == ModuleId(1))
        .unwrap();
    let contracts = interface.callable_contracts.as_ref().unwrap();
    assert_eq!(contracts.len(), 1);
    assert_eq!(
        contracts[0]
            .contract
            .traces
            .iter()
            .map(|t| t.calls.len())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    case.write("interface.txt", format!("{interface:#?}"));
    for (label, main, expected, code) in [
        ("normal", content[0].1.clone(), "PCBAST", 0),
        (
            "local-error",
            content[0]
                .1
                .replace("combine.run(&f,0)", "combine.run(&f,2)"),
            "CBAT",
            1,
        ),
        (
            "parameter-error",
            content[0].1.replace("return n+1", "return true"),
            "PCBAT",
            1,
        ),
    ] {
        let files: Vec<_> = content
            .iter()
            .map(|(name, text)| {
                (
                    *name,
                    if *name == "main" {
                        main.as_str()
                    } else {
                        text.as_str()
                    },
                )
            })
            .collect();
        native(&project(&files), label, expected, code);
    }
}
#[test]
fn unused_templates_still_check_opaque_consumption_and_mutability() {
    let cases = [
        (
            "repeat-once",
            format!(
                "{ONCE_PREFIX}fn run<F:Fn()->i64>(f:&F)->i64{{let local=make(2);return local()+local()+f()}}fn main()->i32{{return 0}}"
            ),
            "consumed",
        ),
        (
            "moved-once",
            format!(
                "{ONCE_PREFIX}fn run<F:Fn()->i64>(f:&F)->i64{{let local=make(2);let alias=local;return local()+f()}}fn main()->i32{{return 0}}"
            ),
            "consumed",
        ),
        (
            "moved-alias-twice",
            format!(
                "{ONCE_PREFIX}fn run<F:Fn()->i64>(f:&F)->i64{{let local=make(2);let alias=local;return alias()+alias()+f()}}fn main()->i32{{return 0}}"
            ),
            "consumed",
        ),
        (
            "immutable-mut",
            format!(
                "{MUT_PREFIX}fn run<F:Fn()->i64>(f:&F)->i64{{let local=make(2);return local(1)+f()}}fn main()->i32{{return 0}}"
            ),
            "mutable binding",
        ),
        (
            "local-address",
            SHARED.replace(
                "return f(local(3))",
                "let address=&local;return f(address(3))",
            ),
            "direct opaque factory result",
        ),
        (
            "wrong-argument",
            SHARED.replace("local(3)", "local(true)"),
            "types must agree",
        ),
    ];
    for (label, text, message) in cases {
        let case = Case::new(label);
        let report = compile(&project(&[("main", &text)]), &case, None);
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
                .any(|d| d.phase == Phase::Expand && d.message.contains(message)),
            "{}",
            report.to_json()
        );
    }
}
#[test]
fn local_invocations_pay_each_independent_phase_budget() {
    let source = SourceUnit::new(FileId(0), ModuleId(0), "budget127.zil", SHARED);
    let syntax = zil_syntax::parse(&source).into_result().unwrap();
    let input = core(SHARED);
    let defaults = Limits::default();
    let target = TargetProfile::default();
    let module = zil_types::check(&input, &target, &defaults).unwrap();
    let plan = zil_analyze::analyze(&module, &defaults).unwrap();
    let case = Case::new("budgets");
    case.write("source.zil", SHARED);
    let mut records = Vec::new();
    for phase in [Phase::Expand, Phase::Types, Phase::Analyze, Phase::Verify] {
        let succeeds = |steps| {
            let limits = Limits {
                max_analysis_steps: steps,
                ..defaults.clone()
            };
            match phase {
                Phase::Expand => zil_expand::expand_with_map_limits(&source, &syntax, &limits)
                    .map(|_| ())
                    .map_err(|e| e[0].category),
                Phase::Types => zil_types::check(&input, &target, &limits)
                    .map(|_| ())
                    .map_err(|e| e[0].category),
                Phase::Analyze => zil_analyze::analyze(&module, &limits)
                    .map(|_| ())
                    .map_err(|e| e[0].category),
                Phase::Verify => {
                    zil_verify::verify_module(module.clone(), plan.clone(), &target, &limits)
                        .map(|_| ())
                        .map_err(|e| e[0].category)
                }
                _ => unreachable!(),
            }
        };
        let (mut low, mut high) = (0, defaults.max_analysis_steps);
        while low < high {
            let mid = low + (high - low) / 2;
            if succeeds(mid).is_ok() {
                high = mid
            } else {
                low = mid + 1
            }
        }
        assert!(low > 0);
        assert!(succeeds(low).is_ok());
        assert_eq!(succeeds(low - 1), Err(ResultCategory::ResourceLimit));
        assert!(succeeds(low).is_ok());
        records.push(format!("{{\"phase\":\"{phase:?}\",\"N\":{low},\"N_minus_1\":{},\"at_N\":\"Pass\",\"at_N_minus_1\":\"ResourceLimit\",\"retry\":\"Pass\"}}", low - 1));
    }
    case.write("boundaries.json", format!("[{}]\n", records.join(",")));
}
