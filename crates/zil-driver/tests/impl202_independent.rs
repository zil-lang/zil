//! Independent complete StoreElement binding review. Raw Core is untrusted;
//! qualification failures are recorded separately from independent Verify.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};
use zil_base::{
    Diagnostic, FileId, Limits, ModuleId, NodeId, Phase, ResultCategory, SourceUnit, TargetProfile,
};
use zil_driver::{
    CompileOptions, CompileReport, Optimization, ProjectSources, Stage, compile_project,
    compile_source,
};
use zil_ir::*;

const ORIGINAL_PUSH: &str = include_str!("impl202_fixtures/01_original_push.zil");
const ORIGINAL_FACTORY: &str = include_str!("impl202_fixtures/02_original_factory.zil");
const SHADOW: &str = include_str!("impl202_fixtures/03_shadow_nested.zil");
const PAIR: &str = include_str!("impl201_fixtures/01_pair_store_mut.zil");
const HELPERS: &str = r#"
fn empty<T:StoreElement>()->Store<T> contract{result(create)}{return store_new()}
fn reserve<T:StoreElement>(items:Store<T>,count:i64)->bool{return store_reserve(items,count)}
fn push<T:StoreElement>(items:Store<T>,value:T)->T!void{return store_push(items,value)}
fn pop<T:StoreElement>(items:Store<T>)->void!T{return store_pop(items)}
"#;
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
            "independent202-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path, retained.is_some())
    }
    fn write(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.1 {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn compile(case: &Case, source: &str, native: Option<Optimization>) -> CompileReport {
    case.write("source.zil", source);
    let report = compile_source(
        &SourceUnit::new(FileId(0), ModuleId(0), "independent202.zil", source),
        &CompileOptions {
            stop_after: if native.is_some() {
                Stage::Build
            } else {
                Stage::Verify
            },
            optimization: native.unwrap_or(Optimization::O0),
            output: native.map(|_| case.0.join("program")),
            emit_dir: Some(case.0.join("phases")),
            ..CompileOptions::default()
        },
    );
    case.write("compile.json", report.to_json());
    report
}
fn native(label: &str, source: &str, expected: &str, exit: i32) {
    for opt in [Optimization::O0, Optimization::O2] {
        let case = Case::new(label);
        let report = compile(&case, source, Some(opt));
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
        case.write("stdout.log", &output.stdout);
        case.write("stderr.log", &output.stderr);
        case.write("native.txt", format!("optimization={opt:?}\nexit={:?}\nexpected_exit={exit}\nexpected_stdout={expected:?}\n", output.status.code()));
        assert_eq!(output.status.code(), Some(exit), "{label}: {output:?}");
        assert_eq!(output.stdout, expected.as_bytes(), "{label}: {output:?}");
        assert!(output.stderr.is_empty());
    }
}
fn reject_source(label: &str, source: &str, phases: &[Phase], code: Option<&str>) {
    let case = Case::new(label);
    let r = compile(&case, source, None);
    assert!(
        matches!(
            r.status,
            ResultCategory::SafeReject | ResultCategory::Unsupported
        ),
        "{}",
        r.to_json()
    );
    assert!(phases.contains(&r.phase), "{}", r.to_json());
    if let Some(code) = code {
        assert!(
            r.diagnostics.iter().any(|d| d.code == code),
            "{}",
            r.to_json()
        );
    }
    assert!(!r.artifacts.contains_key("executable"));
}
fn expanded(label: &str, source: &str) -> CoreModule {
    let case = Case::new(label);
    case.write("source.zil", source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent202.zil", source);
    let syntax = zil_syntax::parse(&unit).into_result();
    case.write("syntax.txt", format!("{syntax:#?}"));
    let core = zil_expand::expand(&unit, &syntax.unwrap());
    case.write("core.txt", format!("{core:#?}"));
    core.unwrap()
}
fn program(core: CoreModule) -> CoreProgram {
    CoreProgram {
        root: core.module_id,
        source_fingerprint: core.source_fingerprint,
        modules: vec![ProgramModule {
            imports: vec![],
            exports: core
                .functions
                .iter()
                .filter(|f| f.name == "main")
                .map(|f| f.id)
                .collect(),
            core,
        }],
    }
}
fn raw_types(label: &str, core: &CoreModule, limits: &Limits, reject: bool) {
    let case = Case::new(label);
    case.write("core.txt", format!("{core:#?}"));
    case.write("limits.txt", format!("{limits:#?}"));
    let single = zil_types::check(core, &TargetProfile::host_llvm(), limits);
    case.write("single.txt", format!("{single:#?}"));
    let multiple =
        zil_types::check_program(&program(core.clone()), &TargetProfile::host_llvm(), limits);
    case.write("program.txt", format!("{multiple:#?}"));
    if reject {
        for ds in [single.unwrap_err(), multiple.unwrap_err()] {
            assert!(
                ds.iter().any(|d| d.phase == Phase::Types
                    && matches!(
                        d.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )),
                "{label}: {ds:#?}"
            );
        }
    } else {
        single.unwrap();
        multiple.unwrap();
    }
}
fn typed_control(label: &str, source: &str) -> (CoreModule, TypedModule, CandidatePlan) {
    let core = expanded(label, source);
    let case = Case::new(&format!("{label}-typed"));
    let typed = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default());
    case.write("typed.txt", format!("{typed:#?}"));
    let typed = typed.unwrap();
    let plan = zil_analyze::analyze(&typed, &Limits::default());
    case.write("analyze.txt", format!("{plan:#?}"));
    let plan = plan.unwrap();
    let wire = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    case.write("candidate.wire", &wire);
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&wire, &Limits::default()).unwrap(),
        (typed.clone(), plan.clone())
    );
    let raw = zil_verify::verify_module(
        typed.clone(),
        plan.clone(),
        &typed.target,
        &Limits::default(),
    );
    let decoded = zil_verify::verify_module_bytes(&wire, &typed.target, &Limits::default());
    case.write("verify.txt", format!("raw={raw:#?}\nwire={decoded:#?}"));
    raw.unwrap();
    decoded.unwrap();
    (core, typed, plan)
}

#[test]
fn both_historical_helper_failures_execute_with_unchanged_source_bytes() {
    assert_eq!(ORIGINAL_PUSH.len(), 1655);
    assert_eq!(ORIGINAL_FACTORY.len(), 1657);
    // Exact SHA256 identities are recorded with the committed fixture files;
    // these are the original helpers01 bytes, not simplified replacements.
    native("original-push", ORIGINAL_PUSH, "1A2BZ", 0);
    native("original-factory", ORIGINAL_FACTORY, "1A2BZ", 0);
}

#[test]
fn caller_binders_nominal_names_and_child_binders_have_lexical_identity() {
    native("shadow-nested", SHADOW, "1A2BZ", 0);
    let inferred = SHADOW.replace("nested_empty::<Item<i32,T>>()", "nested_empty()");
    native("shadow-inferred-result", &inferred, "1A2BZ", 0);
    let no_context = SHADOW.replace(
        "let items: Store<Item<i32,T>> = nested_empty::<Item<i32,T>>()",
        "let items = empty()",
    );
    assert_ne!(no_context, SHADOW);
    reject_source(
        "empty-without-unique-type",
        &no_context,
        &[Phase::Expand, Phase::Types],
        None,
    );
}

fn shadow_without_nominal() -> String {
    SHADOW
        .replace(
            "struct T { value:i32 }\nfn concrete(value:T)->i32{return value.value}\n",
            "",
        )
        .replace("concrete(T{value:0})+", "")
}

#[test]
fn same_named_child_binders_execute_without_an_unrelated_nominal_collision() {
    let source = shadow_without_nominal();
    assert_ne!(source, SHADOW);
    native("nested-no-nominal-collision", &source, "1A2BZ", 0);
    let inferred = source.replace("nested_empty::<Item<i32,T>>()", "nested_empty()");
    assert_ne!(inferred, source);
    native("nested-result-inferred-no-nominal", &inferred, "1A2BZ", 0);
}

fn nominal_pair_source(phantom: bool, reversed: bool) -> String {
    let mut prefix = PAIR[..PAIR.find("fn run<").unwrap()].to_owned();
    if phantom {
        prefix = prefix.replace("; value: T", "");
    }
    let mut body = r#"
struct Left{value:i32}struct Right{value:i32}
fn run<F:FnMut()->i32>(pair:Pair<F>)->void!i32{
let a:Store<Item<Left,F>>=empty()
let b:Store<Item<Right,F>>=empty()
if !reserve(a,1)||!reserve(b,1){return ()}
push(a,Item::<Left,F>{root:Ticket{code:49},callback:pair.first,value:Left{value:10}})
push(b,Item::<Right,F>{root:Ticket{code:50},callback:pair.second,value:Right{value:20}})
var left=pop(a)?
var right=pop(b)?
return left.callback()+right.callback()-32
}
fn main()->void!i32{return run(pair(make(65,10),make(66,20)))}
"#
    .to_owned();
    if phantom {
        body = body
            .replace(",value:Left{value:10}", "")
            .replace(",value:Right{value:20}", "");
    }
    if reversed {
        body = body.replace(
            "let a:Store<Item<Left,F>>=empty()\nlet b:Store<Item<Right,F>>=empty()",
            "let b:Store<Item<Right,F>>=empty()\nlet a:Store<Item<Left,F>>=empty()",
        );
    }
    format!("{HELPERS}{prefix}{body}")
}

#[test]
fn full_nominal_and_phantom_data_arguments_are_not_a_closure_only_instance_key() {
    for phantom in [false, true] {
        for reversed in [false, true] {
            let source = nominal_pair_source(phantom, reversed);
            native(
                &format!("nominal-phantom-{phantom}-reverse-{reversed}"),
                &source,
                "2B1AZ",
                0,
            );
        }
    }
    let wrong = nominal_pair_source(false, false)
        .replace("push(b,Item::<Right,F>", "push(a,Item::<Right,F>");
    reject_source(
        "same-closure-wrong-full-nominal",
        &wrong,
        &[Phase::Expand, Phase::Types],
        None,
    );
}

#[test]
fn ordinary_pop_success_payload_preserves_the_callback_helper_dependency() {
    let helper = "fn invoke<G:FnMut()->i32>(callback:&mut G)->i32{return callback()}\n";
    for phantom in [false, true] {
        let original = nominal_pair_source(phantom, false);
        let source = original.replace(
            "return left.callback()+right.callback()-32",
            "return invoke(&mut left.callback)+invoke(&mut right.callback)-32",
        );
        assert_ne!(source, original);
        native(
            &format!("ordinary-pop-success-helper-phantom-{phantom}"),
            &format!("{helper}{source}"),
            "2B1AZ",
            0,
        );
    }
}

#[test]
fn actual_desc_structural_writes_survive_nested_helper_parameter_remapping() {
    // Isolate actual parameter-remapped writes from the separate nominal-name
    // collision test; both must pass, but one early failure must not hide the
    // other independent borrow-checking obligation.
    let control = shadow_without_nominal();
    native("desc-write-before-window", &control, "1A2BZ", 0);
    for (label, changed) in [
        (
            "nested-reserve",
            "let first = store_at_mut(items, 0)\nshifted(0,items,4)",
        ),
        (
            "noop-reserve",
            "let first = store_at_mut(items, 0)\nshifted(0,items,2)",
        ),
        (
            "failed-reserve",
            "let first = store_at_mut(items, 0)\nshifted(0,items,-1)",
        ),
    ] {
        let source = control.replace("let first = store_at_mut(items, 0)", changed);
        reject_source(label, &source, &[Phase::Verify], Some("E-BORROW-CONFLICT"));
    }
}

#[test]
fn ordinary_templates_can_plan_independent_factory_and_callable_dependencies() {
    let prefix = r#"
fn signal()->impl Fn()->i32{return ()=>0}
fn observe<G:Fn()->i32>(callback:G)->i32{return callback()}
fn measured<U:StoreElement>(items:Store<U>)->i64{
let callback=signal()
let zero=observe(callback)
return store_len(items)+i64(zero)
}
"#;
    let source = format!(
        "{prefix}{}",
        ORIGINAL_PUSH.replace(
            "let first = store_at_mut(items, 0)",
            "if measured(items)!=2{return 9}\nlet first = store_at_mut(items, 0)"
        )
    );
    native("ordinary-factory-callable-plan", &source, "1A2BZ", 0);
}

#[test]
fn identical_template_names_in_two_modules_keep_their_definition_identity() {
    let library = r#"
pub fn empty<T:StoreElement>()->Store<T> contract{result(create)}{return store_new()}
fn reserve<T:StoreElement>(items:Store<T>,count:i64)->bool{return store_reserve(items,count)}
pub fn shifted<T:StoreElement>(tag:i32,items:Store<T>,count:i64)->bool{tag;return reserve::<T>(items,count)}
pub fn push<T:StoreElement>(items:Store<T>,value:T)->T!void{return store_push(items,value)}
pub fn count<T:StoreElement>(items:Store<T>)->i64{return store_len(items)}
"#;
    let other = "pub fn count<U:StoreElement>(items:Store<U>)->i64{return store_len(items)+1}";
    let main=format!("import left\nimport right\n{}",PAIR
        .replace("= store_new()","= left.empty()")
        .replace("store_reserve(items, 2)","left.shifted(0,items,2)")
        .replace("store_push(items,","left.push(items,")
        .replace("let first = store_at_mut(items, 0)","if left.count(items)!=2||right.count(items)!=3{return 6}\nlet first = store_at_mut(items, 0)"));
    for opt in [Optimization::O0, Optimization::O2] {
        let case = Case::new("two-module-same-template-name");
        let files = [("main", main.as_str()), ("left", library), ("right", other)];
        for (name, source) in files {
            case.write(&format!("{name}.zil"), source);
        }
        let project = ProjectSources {
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
        };
        let r = compile_project(
            &project,
            &CompileOptions {
                stop_after: Stage::Build,
                optimization: opt,
                output: Some(case.0.join("program")),
                emit_dir: Some(case.0.join("phases")),
                ..CompileOptions::default()
            },
        );
        case.write("compile.json", r.to_json());
        assert_eq!(r.status, ResultCategory::Success, "{}", r.to_json());
        assert_eq!(r.verification_runs, 1);
        let output = Command::new(&r.artifacts["executable"]).output().unwrap();
        case.write("stdout.log", &output.stdout);
        case.write("stderr.log", &output.stderr);
        case.write(
            "native.txt",
            format!(
                "optimization={opt:?}\nexit={:?}\nexpected_exit=0\nexpected_stdout=1A2BZ\n",
                output.status.code()
            ),
        );
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"1A2BZ");
        assert!(output.stderr.is_empty());
    }
}

const IDENTITY_ONLY: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Empty{}
impl Drop for Empty{fn drop(self:Self){unsafe{putchar(65)}}}
package State<F>{manages empty:Empty;callback:F}
fn make()->impl Fn()->i32{return ()=>7}
fn pack<F:Fn()->i32>(callback:F)->State<F>{return State::<F>{empty:Empty{},callback:callback}}
fn identity<T:StoreElement>(value:T)->T{return value}
fn run<F:Fn()->i32>(state:State<F>)->i32{let retained=identity(state);return retained.callback()-7}
fn main()->i32{return run(pack(make()))}
"#;

fn no_store_qualification(label: &str, source: &str) -> (CoreModule, TypedModule, CandidatePlan) {
    let core = expanded(label, source);
    let case = Case::new(&format!("{label}-qualification"));
    let typed = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default());
    case.write("typed.txt", format!("{typed:#?}"));
    let typed = typed.unwrap();
    let identity=typed.functions.iter().find(|f| f.parameters.len()==1 && matches!(f.return_type,Type::Package{..}) && f.locals[f.parameters[0].0 as usize].ty==f.return_type).expect("actual concrete identity parameter/result, without inferring it from its generated name");
    assert!(identity.callable_contract.is_none());
    let actual = &identity.return_type;
    let Type::Package { name, .. } = actual else {
        unreachable!()
    };
    assert!(
        typed
            .resources
            .iter()
            .any(|r| r.name == format!("@builtin:OwnedStore<{name}>")
                && matches!(&r.payload,Type::OwnedStore{element} if element.as_ref()==actual)),
        "exact full T must survive in its canonical qualification definition"
    );
    assert!(
        !typed
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(
                i.kind,
                InstructionKind::OwnedStoreNew { .. }
                    | InstructionKind::OwnedStoreReserve { .. }
                    | InstructionKind::OwnedStorePush { .. }
                    | InstructionKind::OwnedStorePop { .. }
                    | InstructionKind::OwnedStoreLen { .. }
                    | InstructionKind::OwnedStoreCapacity { .. }
                    | InstructionKind::OwnedStoreBorrow { .. }
                    | InstructionKind::OwnedStoreBorrowMut { .. }
            )),
        "metadata cannot silently construct a Store value or use a runtime Store operation"
    );
    let plan = zil_analyze::analyze(&typed, &Limits::default());
    case.write("analyze.txt", format!("{plan:#?}"));
    (core, typed, plan.unwrap())
}

#[test]
fn a_no_store_identity_still_carries_independent_actual_element_qualification() {
    let direct = IDENTITY_ONLY.replace("let retained=identity(state)", "let retained=state");
    native("zero-layout-without-store-bound-control", &direct, "A", 0);
    reject_source(
        "zero-layout-with-store-bound",
        IDENTITY_ONLY,
        &[Phase::Verify],
        Some("E-UNSUPPORTED"),
    );
    let (_, typed, plan) =
        no_store_qualification("zero-layout-bound-no-store-operations", IDENTITY_ONLY);
    let case = Case::new("no-store-zero-layout-raw-wire");
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    case.write("candidate.wire", &bytes);
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (typed.clone(), plan.clone())
    );
    let raw = zil_verify::verify_module(typed.clone(), plan, &typed.target, &Limits::default());
    let wire = zil_verify::verify_module_bytes(&bytes, &typed.target, &Limits::default());
    case.write("verify.txt", format!("raw={raw:#?}\nwire={wire:#?}"));
    for result in [raw, wire] {
        let errors = result.unwrap_err();
        assert!(
            errors.iter().any(|d| d.phase == Phase::Verify
                && d.category == ResultCategory::Unsupported
                && d.message.contains("nonzero actual inline layout")),
            "{errors:#?}"
        );
    }
    let nonzero = IDENTITY_ONLY.replace(
        "fn make()->impl Fn()->i32{return ()=>7}",
        "fn make()->impl Fn()->i32{let n=7;return move()=>n}",
    );
    native("nonzero-layout-bound-no-store-operations", &nonzero, "A", 0);
    let (_, typed, plan) = no_store_qualification("nonzero-layout-bound", &nonzero);
    let case = Case::new("no-store-nonzero-layout-raw-wire");
    let bytes = zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    case.write("candidate.wire", &bytes);
    let raw = zil_verify::verify_module(typed.clone(), plan, &typed.target, &Limits::default());
    let wire = zil_verify::verify_module_bytes(&bytes, &typed.target, &Limits::default());
    case.write("verify.txt", format!("raw={raw:#?}\nwire={wire:#?}"));
    raw.unwrap();
    wire.unwrap();
    // We do not call removal of an unused constraint an unsafe IR bypass:
    // Verify does not prove arbitrary raw IR equivalent to the source program.
}

#[test]
fn qualification_only_instances_charge_and_replay_their_complete_definition() {
    let source = IDENTITY_ONLY.replace(
        "fn make()->impl Fn()->i32{return ()=>7}",
        "fn make()->impl Fn()->i32{let n=7;return move()=>n}",
    );
    let (core, typed, plan) = no_store_qualification("qualification-budget-control", &source);
    let types = boundary(|n| {
        zil_types::check(
            &core,
            &typed.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|actual| assert_eq!(actual, typed))
    });
    let analyze = boundary(|n| {
        zil_analyze::analyze(
            &typed,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|actual| assert_eq!(actual, plan))
    });
    let verify = boundary(|n| {
        zil_verify::verify_module(
            typed.clone(),
            plan.clone(),
            &typed.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    Case::new("qualification-only-exact-budgets").write("boundaries.json",format!("{{\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"checks\":\"N, N-1 ResourceLimit, fresh N; Types and Analyze output exactly equals default\",\"runtime_store_operations\":0}}\n"));
}

#[test]
fn unused_declarations_do_not_gain_fields_copy_capture_or_give_from_the_bound() {
    for (label, declaration) in [
        (
            "unused-field",
            "fn invalid<T:StoreElement>(value:T)->i32{return value.value}",
        ),
        (
            "unused-callback",
            "fn invalid<T:StoreElement>(value:T)->i32{return value.callback()}",
        ),
        (
            "unused-give",
            "fn invalid<T:StoreElement>(items:Store<T>)->Store<T> contract{result(give)}{return give items}",
        ),
        (
            "unused-double-take",
            "fn invalid<T:StoreElement>(items:Store<T>,value:T){store_push(items,value);store_push(items,value)}",
        ),
    ] {
        reject_source(
            label,
            &format!("{declaration}\n{ORIGINAL_PUSH}"),
            &[Phase::Expand, Phase::Types],
            None,
        );
    }
}

fn visit_block(block: &mut CoreBlock, action: &mut impl FnMut(&mut CoreExpr) -> bool) {
    use CoreStatementKind as S;
    for statement in &mut block.statements {
        match &mut statement.kind {
            S::Binding {
                initializer: Some(v),
                ..
            }
            | S::Expression(v)
            | S::Return(Some(v)) => visit_expr(v, action),
            S::Assign { target, value } => {
                visit_expr(target, action);
                visit_expr(value, action);
            }
            S::If {
                condition,
                then_body,
                else_body,
            } => {
                visit_expr(condition, action);
                visit_block(then_body, action);
                if let Some(b) = else_body {
                    visit_block(b, action);
                }
            }
            S::IfLet {
                value,
                then_body,
                else_body,
                ..
            } => {
                visit_expr(value, action);
                visit_block(then_body, action);
                if let Some(b) = else_body {
                    visit_block(b, action);
                }
            }
            S::While { condition, body } => {
                visit_expr(condition, action);
                visit_block(body, action);
            }
            S::WhileLet { value, body, .. } => {
                visit_expr(value, action);
                visit_block(body, action);
            }
            S::ForRange {
                start, end, body, ..
            } => {
                visit_expr(start, action);
                visit_expr(end, action);
                visit_block(body, action);
            }
            S::Match { value, arms } => {
                visit_expr(value, action);
                for arm in arms {
                    visit_block(&mut arm.body, action);
                }
            }
            S::Block(b) | S::Unsafe(b) => visit_block(b, action),
            _ => {}
        }
    }
}
fn visit_expr(expr: &mut CoreExpr, action: &mut impl FnMut(&mut CoreExpr) -> bool) {
    if action(expr) {
        return;
    }
    use CoreExprKind as E;
    match &mut expr.kind {
        E::Ascribed { value, .. }
        | E::IntegerConvert { value, .. }
        | E::CompileTime(value)
        | E::Try(value)
        | E::Unary { operand: value, .. } => visit_expr(value, action),
        E::Binary { left, right, .. } => {
            visit_expr(left, action);
            visit_expr(right, action);
        }
        E::Call { callee, arguments } => {
            visit_expr(callee, action);
            for a in arguments {
                visit_expr(a, action);
            }
        }
        E::TypeApply { callee, .. } => visit_expr(callee, action),
        E::Tuple(xs) | E::Array(xs) => {
            for x in xs {
                visit_expr(x, action);
            }
        }
        E::Index { object, index } => {
            visit_expr(object, action);
            visit_expr(index, action);
        }
        E::Slice { object, start, end } => {
            visit_expr(object, action);
            for x in [start, end].into_iter().flatten() {
                visit_expr(x, action);
            }
        }
        E::Member { object, .. } => visit_expr(object, action),
        E::Record { fields, .. } => {
            for (_, v) in fields {
                visit_expr(v, action);
            }
        }
        E::RecordUpdate { base, fields, .. } => {
            visit_expr(base, action);
            for (_, v) in fields {
                visit_expr(v, action);
            }
        }
        E::Sequence { bindings, value } => {
            for (_, v) in bindings {
                visit_expr(v, action);
            }
            visit_expr(value, action);
        }
        E::Closure { body, .. } => visit_block(body, action),
        E::If {
            condition,
            then_value,
            else_value,
        } => {
            visit_expr(condition, action);
            visit_expr(then_value, action);
            visit_expr(else_value, action);
        }
        _ => {}
    }
}
fn first_application(core: &mut CoreModule, mutate: impl FnOnce(&mut CoreExpr)) {
    let mut mutate = Some(mutate);
    for f in &mut core.functions {
        visit_block(&mut f.body, &mut |expr| {
            if matches!(expr.kind, CoreExprKind::TypeApply { .. })
                && let Some(mutate) = mutate.take()
            {
                mutate(expr);
                return true;
            }
            false
        });
    }
    assert!(mutate.is_none(), "source must actually retain a TypeApply");
}

#[test]
fn raw_template_headers_and_lexical_free_binders_are_checked_in_both_entries() {
    let source = format!("fn unrelated(value:i32)->i32{{return 0}}\n{ORIGINAL_PUSH}");
    let core = expanded("metadata-control", &source);
    raw_types("metadata-valid", &core, &Limits::default(), false);
    let index = core
        .functions
        .iter()
        .position(|f| !f.type_parameters.is_empty())
        .expect("retained ordinary binder");
    let callable = core
        .functions
        .iter()
        .find(|f| !f.callable_parameters.is_empty())
        .unwrap()
        .callable_parameters
        .clone();
    let opaque = core
        .functions
        .iter()
        .find_map(|f| f.callable_return.clone())
        .unwrap();
    let foreign = core
        .functions
        .iter()
        .find_map(|f| f.foreign.clone())
        .unwrap();
    for attack in 0..8 {
        let mut changed = core.clone();
        match attack {
            0 => changed.functions[index].type_parameters[0].index = 1,
            1 => {
                let p = changed.functions[index].type_parameters[0].clone();
                changed.functions[index].type_parameters.push(p);
            }
            2 => changed.functions[index].type_parameters[0].name.clear(),
            3 => changed.functions[index].callable_parameters = callable.clone(),
            4 => changed.functions[index].callable_return = Some(opaque.clone()),
            5 => changed.functions[index].foreign = Some(foreign.clone()),
            6 => {
                let mut duplicate = changed.functions[index].clone();
                duplicate.name.push_str("_different_name_same_id");
                changed.functions.push(duplicate);
            }
            7 => {
                changed.functions[index].receiver = Some(ReceiverShape {
                    ty: Type::I32,
                    mode: PassingMode::Copy,
                    implicit: true,
                })
            }
            _ => unreachable!(),
        }
        raw_types(
            &format!("header-attack-{attack}"),
            &changed,
            &Limits::default(),
            true,
        );
    }
    for free in ["T", "F"] {
        let mut changed = core.clone();
        let unrelated = changed
            .functions
            .iter_mut()
            .find(|f| f.name == "unrelated")
            .unwrap();
        unrelated.parameters[0].ty = Type::Named(free.into());
        raw_types(
            &format!("free-binder-{free}-outside-definition"),
            &changed,
            &Limits::default(),
            true,
        );
    }
}

#[test]
fn unused_raw_template_metadata_cannot_borrow_another_binder_or_target() {
    let mut core = expanded("unused-raw-template", ORIGINAL_PUSH);
    let mut unused = core
        .functions
        .iter()
        .find(|f| !f.type_parameters.is_empty())
        .unwrap()
        .clone();
    unused.id = NodeId(core.functions.iter().map(|f| f.id.0).max().unwrap() + 1);
    unused.name = "unused_ordinary_template".into();
    let unused_index = core.functions.len();
    core.functions.push(unused);
    let mut identity = core.functions[unused_index].clone();
    identity.id = NodeId(identity.id.0 + 1);
    identity.name = "unused_identity_template".into();
    let binder = identity.type_parameters[0].name.clone();
    identity.parameters = vec![CoreParameter {
        name: "value".into(),
        ty: Type::Named(binder.clone()),
        mode: PassingMode::Infer,
        span: identity.span,
    }];
    identity.return_type = Type::Named(binder);
    identity.result_mode = PassingMode::Infer;
    identity.body.statements.truncate(1);
    identity.body.statements[0].kind = CoreStatementKind::Return(Some(CoreExpr {
        id: NodeId(920003),
        span: identity.span,
        kind: CoreExprKind::Name("value".into()),
    }));
    let identity_index = core.functions.len();
    core.functions.push(identity);
    raw_types(
        "unused-raw-template-valid",
        &core,
        &Limits::default(),
        false,
    );
    let mut accepted = Vec::new();
    for attack in [
        "foreign-binder-in-unused-signature",
        "wrong-target-in-unused-body",
        "foreign-binder-in-unused-identity",
        "foreign-target-in-unused-body",
        "unearned-field-in-unused-identity",
        "unearned-call-in-unused-identity",
    ] {
        let mut changed = core.clone();
        let index = if attack.ends_with("-unused-identity") {
            identity_index
        } else {
            unused_index
        };
        let unused = &mut changed.functions[index];
        if attack.starts_with("foreign-binder-") {
            // Every occurrence of the original T now belongs to another
            // definition; this otherwise unused template only declares U.
            assert_ne!(unused.type_parameters[0].name, "UnusedU");
            unused.type_parameters[0].name = "UnusedU".into();
        } else if attack.starts_with("unearned-") {
            let value = CoreExpr {
                id: NodeId(920004),
                span: unused.span,
                kind: CoreExprKind::Name("value".into()),
            };
            let kind = if attack == "unearned-field-in-unused-identity" {
                CoreExprKind::Member {
                    object: Box::new(value),
                    member: "secret".into(),
                }
            } else {
                CoreExprKind::Call {
                    callee: Box::new(value),
                    arguments: vec![],
                }
            };
            unused.body.statements[0].kind = CoreStatementKind::Return(Some(CoreExpr {
                id: NodeId(920005),
                span: unused.span,
                kind,
            }));
        } else {
            let mut mutated = 0;
            visit_block(&mut unused.body, &mut |expr| {
                let CoreExprKind::Call { callee, .. } = &mut expr.kind else {
                    return false;
                };
                if mutated == 0 {
                    let target = CoreExpr {
                        id: NodeId(920001),
                        span: callee.span,
                        kind: CoreExprKind::Name(
                            if attack == "foreign-target-in-unused-body" {
                                "putchar"
                            } else {
                                "make"
                            }
                            .into(),
                        ),
                    };
                    callee.kind = CoreExprKind::TypeApply {
                        callee: Box::new(target),
                        type_arguments: vec![Type::Named(unused.type_parameters[0].name.clone())],
                    };
                    mutated += 1;
                    true
                } else {
                    false
                }
            });
            assert_eq!(mutated, 1);
        }
        let case = Case::new(attack);
        case.write("core.txt", format!("{changed:#?}"));
        let single = zil_types::check(&changed, &TargetProfile::host_llvm(), &Limits::default());
        let multiple = zil_types::check_program(
            &program(changed),
            &TargetProfile::host_llvm(),
            &Limits::default(),
        );
        case.write("single.txt", format!("{single:#?}"));
        case.write("program.txt", format!("{multiple:#?}"));
        for (entry, result) in [
            ("single", single.map(|_| ())),
            ("program", multiple.map(|_| ())),
        ] {
            match result {
                Ok(_) => accepted.push(format!("{attack}/{entry}")),
                Err(ds) => assert!(
                    ds.iter().any(|d| d.phase == Phase::Types
                        && matches!(
                            d.category,
                            ResultCategory::SafeReject | ResultCategory::Unsupported
                        )),
                    "{attack}/{entry}: {ds:#?}"
                ),
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "unused raw metadata accepted: {accepted:?}"
    );
}

#[test]
fn raw_type_application_constraints_cannot_be_dropped_or_become_runtime_values() {
    let core = expanded("application-control", ORIGINAL_PUSH);
    raw_types("application-valid", &core, &Limits::default(), false);
    for attack in 0..9 {
        let mut changed = core.clone();
        first_application(&mut changed, |expr| {
            let CoreExprKind::TypeApply {
                callee,
                type_arguments,
            } = &mut expr.kind
            else {
                unreachable!()
            };
            match attack {
                0 => type_arguments.clear(),
                1 => type_arguments.push(type_arguments[0].clone()),
                2 => type_arguments[0] = Type::Named("UnboundOutsideEveryScope".into()),
                3 => type_arguments[0] = Type::Desc(Box::new(type_arguments[0].clone())),
                4 => callee.kind = CoreExprKind::Name("putchar".into()),
                5 => callee.kind = CoreExprKind::Name("make".into()),
                6 => {
                    let inner = callee.clone();
                    callee.kind = CoreExprKind::TypeApply {
                        callee: inner,
                        type_arguments: type_arguments.clone(),
                    };
                }
                7 => expr.kind = callee.kind.clone(),
                8 => callee.kind = CoreExprKind::Name("run".into()),
                _ => unreachable!(),
            }
        });
        raw_types(
            &format!("application-attack-{attack}"),
            &changed,
            &Limits::default(),
            true,
        );
    }
    let mut changed = core.clone();
    let mut done = false;
    for f in &mut changed.functions {
        visit_block(&mut f.body, &mut |expr| {
            if !done
                && let CoreExprKind::Call { callee, .. } = &expr.kind
                && matches!(callee.kind, CoreExprKind::TypeApply { .. })
            {
                expr.kind = callee.kind.clone();
                done = true;
                return true;
            }
            false
        });
    }
    assert!(done);
    raw_types(
        "type-application-as-bare-value",
        &changed,
        &Limits::default(),
        true,
    );
}

#[test]
fn a_raw_recursive_template_cannot_reuse_an_unfinished_function_identity() {
    let core = expanded("unfinished-instance-control", ORIGINAL_PUSH);
    raw_types(
        "unfinished-instance-valid",
        &core,
        &Limits::default(),
        false,
    );
    let mut changed = core.clone();
    let mut changed_calls = 0;
    for f in &mut changed.functions {
        if f.type_parameters.len() != 1 {
            continue;
        }
        let target = f.name.clone();
        let argument = Type::Named(f.type_parameters[0].name.clone());
        visit_block(&mut f.body, &mut |expr| {
            let CoreExprKind::Call { callee, .. } = &mut expr.kind else {
                return false;
            };
            if !matches!(&callee.kind, CoreExprKind::Name(name) if name == "store_push") {
                return false;
            }
            // Existing value arguments, result and parameter modes still
            // match. Only replace the real builtin edge with this exact
            // template's own type-constrained edge, preserving caller T.
            let original = callee.clone();
            callee.kind = CoreExprKind::TypeApply {
                callee: Box::new(CoreExpr {
                    id: NodeId(910_001),
                    span: original.span,
                    kind: CoreExprKind::Name(target.clone()),
                }),
                type_arguments: vec![argument.clone()],
            };
            changed_calls += 1;
            true
        });
    }
    assert_eq!(changed_calls, 1);
    raw_types(
        "unfinished-self-instance",
        &changed,
        &Limits::default(),
        true,
    );
}

#[test]
fn raw_actual_type_constraints_cannot_select_another_same_environment_data_family() {
    let source = nominal_pair_source(true, false);
    let core = expanded("phantom-type-constraint-control", &source);
    raw_types(
        "phantom-type-constraint-valid",
        &core,
        &Limits::default(),
        false,
    );
    let constraints = applications(&core);
    let first = constraints.first().unwrap().1[0].clone();
    let other = constraints
        .iter()
        .find_map(|(_, ts)| (ts[0] != first).then(|| ts[0].clone()))
        .expect("two full phantom data families, not just one ClosureId");
    let mut changed = core.clone();
    first_application(&mut changed, |expr| {
        let CoreExprKind::TypeApply { type_arguments, .. } = &mut expr.kind else {
            unreachable!()
        };
        assert_eq!(type_arguments[0], first);
        type_arguments[0] = other;
    });
    raw_types(
        "phantom-factory-result-selects-other-family",
        &changed,
        &Limits::default(),
        true,
    );
}

#[test]
fn raw_new_type_trees_are_bounded_before_type_copy_in_single_program_and_ct() {
    let core = expanded("raw-depth-control", ORIGINAL_PUSH);
    let limits = Limits {
        max_call_depth: 32,
        ..Limits::default()
    };
    raw_types("raw-depth-valid", &core, &limits, false);
    let mut changed = core.clone();
    first_application(&mut changed, |expr| {
        let CoreExprKind::TypeApply { type_arguments, .. } = &mut expr.kind else {
            unreachable!()
        };
        let mut deep = Type::I32;
        for _ in 0..64 {
            deep = Type::Array {
                element: Box::new(deep),
                length: 1,
            };
        }
        type_arguments[0] = deep;
    });
    let case = Case::new("raw-type-tree-depth");
    case.write("core.txt", format!("{changed:#?}"));
    let single = zil_types::check(&changed, &TargetProfile::host_llvm(), &limits);
    case.write("single.txt", format!("{single:#?}"));
    let multi = zil_types::check_program(
        &program(changed.clone()),
        &TargetProfile::host_llvm(),
        &limits,
    );
    case.write("program.txt", format!("{multi:#?}"));
    let ct = zil_ct::evaluate_source_constants_with_usage(
        &changed,
        &TargetProfile::host_llvm(),
        &limits,
    );
    case.write("ct.txt", format!("{ct:#?}"));
    for ds in [single.unwrap_err(), multi.unwrap_err(), ct.unwrap_err()] {
        assert!(
            ds.iter()
                .any(|d| d.category == ResultCategory::ResourceLimit),
            "{ds:#?}"
        );
    }
}

fn applications(core: &CoreModule) -> Vec<(NodeId, Vec<Type>)> {
    let mut copy = core.clone();
    let mut found = vec![];
    for f in &mut copy.functions {
        visit_block(&mut f.body, &mut |e| {
            if let CoreExprKind::TypeApply { type_arguments, .. } = &e.kind {
                found.push((e.id, type_arguments.clone()));
            }
            false
        });
    }
    found
}

#[test]
fn runtime_ct_normalization_retains_full_type_constraints_and_stale_wire_is_rejected() {
    let (core, typed, _) = typed_control("ct-normalization-control", ORIGINAL_PUSH);
    let case = Case::new("runtime-ct-normalization");
    let evaluated = zil_ct::evaluate_source_constants_with_usage(
        &core,
        &TargetProfile::host_llvm(),
        &Limits::default(),
    );
    case.write("ct.txt", format!("{evaluated:#?}"));
    let (normalized, _) = evaluated.unwrap();
    assert_eq!(applications(&core), applications(&normalized));
    assert!(!applications(&normalized).is_empty());
    let before = core
        .functions
        .iter()
        .map(|f| (f.id, f.type_parameters.clone()))
        .collect::<Vec<_>>();
    let after = normalized
        .functions
        .iter()
        .map(|f| (f.id, f.type_parameters.clone()))
        .collect::<Vec<_>>();
    assert_eq!(before, after);
    let plan = zil_analyze::analyze(&typed, &Limits::default()).unwrap();
    let mut bytes =
        zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 26);
    bytes[8..12].copy_from_slice(&25u32.to_le_bytes());
    case.write("stale25.wire", &bytes);
    let errors =
        zil_verify::verify_module_bytes(&bytes, &typed.target, &Limits::default()).unwrap_err();
    case.write("stale25.txt", format!("{errors:#?}"));
    assert!(errors.iter().any(|d| d.code == "E-WIRE-VERSION"));
}

fn boundary(mut run: impl FnMut(usize) -> Result<(), Vec<Diagnostic>>) -> usize {
    let (mut lo, mut hi) = (0, Limits::default().max_analysis_steps);
    run(hi).unwrap();
    while lo + 1 < hi {
        let n = lo + (hi - lo) / 2;
        match run(n) {
            Ok(()) => hi = n,
            Err(ds) => {
                assert!(
                    ds.iter()
                        .any(|d| d.category == ResultCategory::ResourceLimit),
                    "{ds:#?}"
                );
                lo = n;
            }
        }
    }
    run(hi).unwrap();
    assert!(
        run(hi - 1)
            .unwrap_err()
            .iter()
            .any(|d| d.category == ResultCategory::ResourceLimit)
    );
    run(hi).unwrap();
    hi
}

#[test]
fn complete_type_instantiation_and_independent_checks_have_exact_fresh_work_bounds() {
    let source = nominal_pair_source(true, true);
    let (core, typed, plan) = typed_control("full-type-budget-control", &source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent202.zil", &source);
    let syntax = zil_syntax::parse(&unit).into_result().unwrap();
    let expand = boundary(|n| {
        zil_expand::expand_with_map_limits(
            &unit,
            &syntax,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let types = boundary(|n| {
        zil_types::check(
            &core,
            &typed.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let analyze = boundary(|n| {
        zil_analyze::analyze(
            &typed,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    let verify = boundary(|n| {
        zil_verify::verify_module(
            typed.clone(),
            plan.clone(),
            &typed.target,
            &Limits {
                max_analysis_steps: n,
                ..Limits::default()
            },
        )
        .map(|_| ())
    });
    Case::new("exact-full-type-budgets").write("boundaries.json",format!("{{\"expand\":{expand},\"types\":{types},\"analyze\":{analyze},\"verify\":{verify},\"checks\":\"N, N-1 ResourceLimit, fresh N\",\"defaults_unchanged\":true}}\n"));
}
