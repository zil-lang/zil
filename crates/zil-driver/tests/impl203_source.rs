//! The four independent preflight sources retain their original bytes and oracles.
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
            "state203-{label}-{}-{}",
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

const SAME: &str =
    include_str!("../../zil-expand/tests/impl203_fixtures/01_same_factory_distinct_modes.zil");
const REPEATED: &str = include_str!("../../zil-expand/tests/impl203_fixtures/02_once_repeated.zil");
const REVERSED: &str =
    include_str!("../../zil-expand/tests/impl203_fixtures/03_reversed_declared_modes.zil");
const MARKERS: &str =
    include_str!("../../zil-expand/tests/impl203_fixtures/04_mode_cleanup_markers.zil");

#[test]
fn original_same_factory_distinct_modes() {
    native("original01", SAME, "BZA", 0);
}
#[test]
fn original_mode_cleanup_markers() {
    native("original04", MARKERS, "MPBQZA", 0);
}
fn reject_consumed(label: &str, source: &str) {
    let case = Case::new(label);
    let report = compile(&case, source, Stage::Verify, Optimization::O0);
    assert_eq!(
        report.status,
        ResultCategory::SafeReject,
        "{}",
        report.to_json()
    );
    // The unchanged source is rejected by its declaration before IR exists.
    // Separate raw Core tests below bypass this source declaration check.
    assert_eq!(report.verification_runs, 0, "{}", report.to_json());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.phase == zil_base::Phase::Expand
                && diagnostic.code == "E-GENERIC-ARGUMENT"
                && diagnostic.message.contains("environment was moved")),
        "{}",
        report.to_json()
    );
}
#[test]
fn original_once_repeated() {
    reject_consumed("original02", REPEATED);
}
#[test]
fn original_reversed_declaration() {
    reject_consumed("original03", REVERSED);
}

const STORE_HELPERS: &str = r#"
fn identity<T:StoreElement>(value:T)->T{return value}
fn empty<T:StoreElement>()->Store<T> contract{result(create)}{return store_new()}
fn reserve<T:StoreElement>(items:Store<T>,count:i64)->bool{return store_reserve(items,count)}
fn push<T:StoreElement>(items:Store<T>,value:T)->T!void{return store_push(items,value)}
fn pop<T:StoreElement>(items:Store<T>)->void!T{return store_pop(items)}
"#;

fn mutable_store(different_roots: bool) -> String {
    let source = format!(
        "{STORE_HELPERS}{}",
        r#"
extern "C" fn putchar(value:i32)->i32;
resource Ticket{code:i32}
impl Drop for Ticket{fn drop(self:Self){unsafe{putchar(.code)}}}
package State<T,F,G>{manages marker:Ticket;first:F;second:G;tag:T}
fn make(label:i32,value:i32)->impl FnMut()->i32{
    let ticket=Ticket{code:label}
    let bytes=bytes_new()
    var count=value
    return move()=>{ticket.code;bytes_len(bytes);count+=1;return count}
}
fn other(label:i32,value:i32)->impl Fn()->i32{
    let ticket=Ticket{code:label}
    return move()=>{ticket.code;return value+1}
}
fn pack<T:Copy,F:FnMut()->i32,G:FnOnce()->i32>(first:F,second:G,tag:T)->State<T,F,G>{
    return State::<T,F,G>{marker:Ticket{code:90},first:first,second:second,tag:tag}
}
fn run<T:Copy,F:FnMut()->i32,G:FnOnce()->i32>(state:State<T,F,G>)->void!(i32,T){
    let items:Store<State<T,F,G>>=empty::<State<T,F,G>>()
    if !reserve(items,1){return ()}
    push(items,identity::<State<T,F,G>>(state))
    let view=store_at_mut(items,0)
    let first=view.first()
    unsafe{putchar(77)}
    let again=store_at_mut(items,0).first()
    unsafe{putchar(80)}
    let next:State<T,F,G>=pop(items)?
    let second=next.second()
    unsafe{putchar(81)}
    return (first+again+second-44,next.tag)
}
fn main()->void!i32{
    let result=run(pack(make(65,10),make(66,20),17))?
    if result.1!=17{return 1}
    return result.0
}
"#
    );
    if different_roots {
        source.replace("make(66,20)", "other(66,20)")
    } else {
        source
    }
}

#[test]
fn mutable_windows_and_owned_once_keep_both_declared_modes() {
    for different_roots in [false, true] {
        native(
            &format!("mutable-store-distinct-{different_roots}"),
            &mutable_store(different_roots),
            "MPBQZA",
            0,
        );
    }
}

#[test]
fn store_windows_reject_borrowed_once_and_shared_mutable() {
    for different_roots in [false, true] {
        let source = mutable_store(different_roots);
        for (label, source, message) in [
            (
                "borrowed-once",
                source.replace(
                    "let first=view.first()",
                    "view.second();let first=view.first()",
                ),
                "FnOnce requires an owned Store element",
            ),
            (
                "shared-mut",
                source.replace("store_at_mut", "store_at"),
                "mutable callback field",
            ),
        ] {
            let case = Case::new(&format!("{label}-distinct-{different_roots}"));
            let report = compile(&case, &source, Stage::Verify, Optimization::O0);
            assert_eq!(
                report.status,
                ResultCategory::SafeReject,
                "{}",
                report.to_json()
            );
            assert_eq!(report.verification_runs, 0, "{}", report.to_json());
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.phase == zil_base::Phase::Expand
                        && diagnostic.message.contains(message)),
                "{}",
                report.to_json()
            );
        }
    }
}

#[test]
fn mutually_exclusive_once_paths_are_checked_independently() {
    for selected in [false, true] {
        let source = MARKERS.replace(
            "let second = state.second()",
            &format!(
                "if {selected} {{ let second = state.second(); unsafe {{ putchar(81) }}; return (first + again + second - 40, tag) }}\n    let second = state.second()"
            ),
        );
        native(&format!("exclusive-once-{selected}"), &source, "MPBQZA", 0);
    }
}

fn expanded(source: &str) -> zil_ir::CoreModule {
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "raw203.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    zil_expand::expand(&unit, &syntax).unwrap()
}

fn raw_program(core: zil_ir::CoreModule) -> zil_ir::CoreProgram {
    zil_ir::CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![zil_ir::ProgramModule {
            imports: vec![],
            exports: vec![],
            core,
        }],
    }
}

fn raw_types(label: &str, core: &zil_ir::CoreModule, expected_code: Option<&str>) {
    let case = Case::new(label);
    fs::write(case.0.join("core.txt"), format!("{core:#?}")).unwrap();
    let limits = zil_base::Limits::default();
    let target = zil_base::TargetProfile::host_llvm();
    let single = zil_types::check(core, &target, &limits);
    let program = zil_types::check_program(&raw_program(core.clone()), &target, &limits)
        .map(|checked| checked.module);
    fs::write(
        case.0.join("types.txt"),
        format!("single={single:#?}\nprogram={program:#?}"),
    )
    .unwrap();
    for result in [single, program] {
        if let Some(code) = expected_code {
            let diagnostics = result.unwrap_err();
            assert!(
                diagnostics.iter().any(|d| d.code == code),
                "{diagnostics:#?}"
            );
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn raw_unused_declaration_flow_keeps_separate_return_paths() {
    use zil_ir::{CoreExprKind, CoreStatementKind};
    let core = expanded(
        r#"
resource Ticket{code:i32}
package State<F,G>{manages marker:Ticket;first:F;second:G}
fn inspect<F:Fn()->i32,G:FnOnce()->i32>(state:State<F,G>,flag:bool)->i32{
    if flag{return state.second()}
    return state.second()
}

fn main()->i32{return 0}
"#,
    );
    raw_types("unused-paths-control", &core, None);
    let index = core
        .functions
        .iter()
        .position(|f| !f.state_parameters.is_empty())
        .unwrap();
    let mut repeated = core.clone();
    let function = &mut repeated.functions[index];
    let mut first = function.body.statements.last().unwrap().clone();
    let CoreStatementKind::Return(Some(value)) = first.kind else {
        panic!("once return")
    };
    first.kind = CoreStatementKind::Expression(value);
    function
        .body
        .statements
        .insert(function.body.statements.len() - 1, first);
    raw_types("unused-paths-repeated", &repeated, Some("E-STATE-ONCE"));
    let mut collision = core;
    let function = &mut collision.functions[index];
    let CoreStatementKind::Return(Some(last)) =
        &mut function.body.statements.last_mut().unwrap().kind
    else {
        panic!("return")
    };
    let CoreExprKind::Call { callee, .. } = &mut last.kind else {
        panic!("call")
    };
    let CoreExprKind::Member { object, .. } = &mut callee.kind else {
        panic!("field")
    };
    // A single NodeId must not simultaneously mean the Package and its field.
    callee.id = object.id;
    raw_types(
        "unused-conflicting-node-origin",
        &collision,
        Some("E-STATE-ORIGIN"),
    );
}

#[test]
fn error_side_complete_state_keeps_its_owning_payload() {
    native(
        "error-side",
        include_str!("../../zil-expand/tests/impl203_fixtures/05_independent_error_side.zil"),
        "ZAB",
        0,
    );
}

#[test]
fn five_module_consumer_infers_complete_value_types() {
    use std::io::Write;
    use std::process::Stdio;
    let entries = [
        (
            "main",
            include_str!("../../../examples/mainline/state_bindings203/main.zil"),
        ),
        (
            "factory",
            include_str!("../../../examples/mainline/state_bindings203/factory.zil"),
        ),
        (
            "state",
            include_str!("../../../examples/mainline/state_bindings203/state.zil"),
        ),
        (
            "portable",
            include_str!("../../../examples/mainline/state_bindings203/portable.zil"),
        ),
        (
            "store_helpers",
            include_str!("../../../examples/mainline/state_bindings203/store_helpers.zil"),
        ),
    ];
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("five-module-inferred-values");
        let sources = ProjectSources {
            root: ModuleId(0),
            modules: entries
                .iter()
                .enumerate()
                .map(|(i, (name, source))| {
                    fs::write(case.0.join(format!("{name}.zil")), source).unwrap();
                    SourceUnit::new(
                        FileId(i as u32),
                        ModuleId(i as u32),
                        format!("{name}.zil"),
                        *source,
                    )
                })
                .collect(),
            paths: entries
                .iter()
                .enumerate()
                .map(|(i, (name, _))| (vec![(*name).into()], ModuleId(i as u32)))
                .collect(),
        };
        let report = compile_project(
            &sources,
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
        for (input, expected) in [
            ("0", "PDdQC2c1ab"),
            ("1", "ACCAPDdQC2c1ab"),
            ("6", "2cd1ab"),
        ] {
            let mut child = Command::new(&report.artifacts["executable"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
            let output = child.wait_with_output().unwrap();
            fs::write(
                case.0.join(format!("input{input}-stdout.log")),
                &output.stdout,
            )
            .unwrap();
            fs::write(
                case.0.join(format!("input{input}-stderr.log")),
                &output.stderr,
            )
            .unwrap();
            fs::write(
                case.0.join(format!("input{input}-result.txt")),
                format!(
                    "exit={:?}\nexpected_stdout={expected}\n",
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

fn types_boundary<T: PartialEq + std::fmt::Debug>(
    mut run: impl FnMut(usize) -> Result<T, Vec<zil_base::Diagnostic>>,
    maximum: usize,
) -> (usize, String) {
    let expected = run(maximum).unwrap();
    let (mut low, mut high) = (0, maximum);
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        match run(mid) {
            Ok(actual) => {
                assert_eq!(actual, expected);
                high = mid;
            }
            Err(errors) => {
                assert!(
                    errors.iter().all(|d| {
                        d.phase == zil_base::Phase::Types
                            && d.category == ResultCategory::ResourceLimit
                    }),
                    "{errors:#?}"
                );
                low = mid;
            }
        }
    }
    assert_eq!(run(high).unwrap(), expected);
    let below = run(high - 1).unwrap_err();
    assert!(
        below.iter().all(|d| {
            d.phase == zil_base::Phase::Types && d.category == ResultCategory::ResourceLimit
        }),
        "{below:#?}"
    );
    assert_eq!(run(high).unwrap(), expected);
    (high, format!("{below:#?}"))
}

#[test]
fn dual_state_and_value_inferred_instances_have_exact_types_boundaries() {
    use zil_base::{Limits, TargetProfile};
    let mixed = mutable_store(true);
    let bare = mixed.replace(
        "run(pack(make(65,10),other(66,20),17))?",
        "run(identity(pack(make(65,10),other(66,20),17)))?",
    );
    assert_ne!(bare, mixed);
    for (label, source) in [
        ("same-declared-modes", MARKERS),
        ("different-mutable-store", mixed.as_str()),
        ("bare-complete-value", bare.as_str()),
    ] {
        let case = Case::new(&format!("budget-{label}"));
        fs::write(case.0.join("source.zil"), source).unwrap();
        let core = expanded(source);
        fs::write(case.0.join("core.txt"), format!("{core:#?}")).unwrap();
        let target = TargetProfile::host_llvm();
        let defaults = Limits::default();
        let check = |limits: &Limits| zil_types::check(&core, &target, limits);
        let typed = check(&defaults).unwrap();
        let plan = zil_analyze::analyze(&typed, &defaults).unwrap();
        let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &defaults).unwrap();
        let decoded = zil_ir::wire::decode_module_candidate(&wire, &defaults).unwrap();
        assert_eq!(decoded, (typed.clone(), plan.clone()));
        zil_verify::verify_module(typed, plan, &target, &defaults).unwrap();
        zil_verify::verify_module_bytes(&wire, &target, &defaults).unwrap();
        let (work, work_below) = types_boundary(
            |n| {
                check(&Limits {
                    max_analysis_steps: n,
                    ..defaults.clone()
                })
            },
            defaults.max_analysis_steps,
        );
        let (locals, locals_below) = types_boundary(
            |n| {
                check(&Limits {
                    max_locals: n,
                    ..defaults.clone()
                })
            },
            defaults.max_locals,
        );
        let (functions, functions_below) = types_boundary(
            |n| {
                check(&Limits {
                    max_functions: n,
                    ..defaults.clone()
                })
            },
            defaults.max_functions,
        );
        fs::write(case.0.join("boundary.json"), format!("{{\"types_work\":{work},\"types_locals\":{locals},\"types_functions\":{functions},\"checks\":\"default == N == fresh N; N-1 Types ResourceLimit; raw and Wire Verify\"}}\n")).unwrap();
        fs::write(case.0.join("below.txt"), format!("work N-1\n{work_below}\nlocals N-1\n{locals_below}\nfunctions N-1\n{functions_below}")).unwrap();
        eprintln!("{label}: types_work={work}, locals={locals}, functions={functions}");
    }
}
