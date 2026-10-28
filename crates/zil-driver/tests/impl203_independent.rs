//! Independent State binder and two-root Store review. Source declared modes
//! and raw concrete safety are deliberately reported as different obligations.
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

const ORIGINAL: &str = include_str!("impl203_fixtures/01_same_factory_distinct_modes.zil");
const ORIGINAL_REPEAT: &str = include_str!("impl203_fixtures/02_once_repeated.zil");
const ORIGINAL_REVERSED: &str = include_str!("impl203_fixtures/03_reversed_declared_modes.zil");
const ORIGINAL_MARKERS: &str = include_str!("impl203_fixtures/04_mode_cleanup_markers.zil");
const HELPERS: &str = r#"
fn identity<T:StoreElement>(value:T)->T{return value}
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
            "independent203-{label}-{}-{}",
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
        &SourceUnit::new(FileId(0), ModuleId(0), "independent203.zil", source),
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
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent203.zil", source);
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
fn original_positive_inputs_keep_exact_bytes_and_declared_cleanup_timing() {
    assert_eq!(ORIGINAL.len(), 958);
    assert_eq!(ORIGINAL_MARKERS.len(), 1039);
    // Capture all four original inputs before asserting an admission result,
    // so a closed scaffold cannot erase the other original observations.
    let reports = [
        ORIGINAL,
        ORIGINAL_REPEAT,
        ORIGINAL_REVERSED,
        ORIGINAL_MARKERS,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, source)| {
        compile(
            &Case::new(&format!("original-preflight-{}", i + 1)),
            source,
            None,
        )
    })
    .collect::<Vec<_>>();
    for i in [0, 3] {
        assert_eq!(
            reports[i].status,
            ResultCategory::Success,
            "{}",
            reports[i].to_json()
        );
    }
    for i in [1, 2] {
        assert!(
            matches!(
                reports[i].status,
                ResultCategory::SafeReject | ResultCategory::Unsupported
            ),
            "{}",
            reports[i].to_json()
        );
    }
    native("original-same-factory", ORIGINAL, "BZA", 0);
    native("original-mode-markers", ORIGINAL_MARKERS, "MPBQZA", 0);
}

#[test]
fn original_repeated_and_reversed_once_calls_are_rejected_after_the_positive_control() {
    native("same-factory-before-negative-controls", ORIGINAL, "BZA", 0);
    assert_eq!(ORIGINAL_REPEAT.len(), 992);
    assert_eq!(ORIGINAL_REVERSED.len(), 958);
    for (label, source) in [
        ("original-repeated-once", ORIGINAL_REPEAT),
        ("original-reversed-bounds", ORIGINAL_REVERSED),
    ] {
        reject_source(
            label,
            source,
            &[Phase::Expand, Phase::Types, Phase::Analyze, Phase::Verify],
            None,
        );
    }
}

#[test]
fn full_type_identity_annotations_and_local_moves_preserve_weak_bound_modes() {
    let moved = ORIGINAL_MARKERS.replace(
        "    let tag = state.tag",
        "    let intermediate = identity::<State<T,F,G>>(state)\n    let next: State<T,F,G> = intermediate\n    let tag = next.tag",
    );
    let moved = moved
        .replace("state.first()", "next.first()")
        .replace("state.second()", "next.second()");
    let source = format!("{HELPERS}{moved}");
    native("typeapply-identity-annotation-name", &source, "MPBQZA", 0);
    let repeat = source.replace(
        "let second = next.second()",
        "let second = next.second()\n    next.second()",
    );
    assert_ne!(repeat, source);
    reject_source(
        "identity-repeated-once",
        &repeat,
        &[Phase::Expand, Phase::Types, Phase::Analyze, Phase::Verify],
        None,
    );
}

#[test]
fn field_take_and_direct_helper_arguments_do_not_strengthen_actual_fn() {
    let taken = ORIGINAL_MARKERS.replace(
        "let second = state.second()",
        "let second_fn = state.second\n    let second = second_fn()",
    );
    native("taken-once-field", &taken, "MPBQZA", 0);
    let repeated = taken.replace(
        "let second = second_fn()",
        "let second = second_fn()\n    second_fn()",
    );
    reject_source(
        "taken-once-field-repeated",
        &repeated,
        &[Phase::Expand, Phase::Types, Phase::Analyze, Phase::Verify],
        None,
    );
    let own = format!(
        "fn consume<H:FnOnce()->i32>(value:H)->i32{{return value()}}\n{}",
        taken.replace(
            "let second = second_fn()",
            "let second = consume(second_fn)"
        )
    );
    native("taken-once-to-own-helper", &own, "MPBQZA", 0);
    let borrowed = format!(
        "fn observe<H:Fn()->i32>(value:&H)->i32{{return value()}}\n{}",
        taken.replace(
            "let second = second_fn()",
            "let second = observe(&second_fn)"
        )
    );
    reject_source(
        "taken-once-cannot-be-strengthened-at-helper",
        &borrowed,
        &[Phase::Expand, Phase::Types],
        None,
    );
}

fn store_source(different_roots: bool) -> String {
    let prefix = &ORIGINAL[..ORIGINAL.find("fn run<").unwrap()];
    let mut source = format!(
        "{HELPERS}{prefix}{}",
        r#"
fn other(label:i32,value:i32)->impl Fn()->i32{
    let ticket=Ticket{code:label}
    let bytes=bytes_new()
    return move()=>{ticket.code;bytes_len(bytes);return value}
}
fn run<T:Copy,F:Fn()->i32,G:FnOnce()->i32>(state:State<T,F,G>)->void!(i32,T){
    let items:Store<State<T,F,G>>=empty::<State<T,F,G>>()
    if !reserve(items,1){return ()}
    push(items,identity::<State<T,F,G>>(state))
    let view=store_at(items,0)
    let first=view.first()
    unsafe{putchar(77)}
    let again=store_at(items,0).first()
    unsafe{putchar(80)}
    let next:State<T,F,G>=pop(items)?
    let second=next.second()
    unsafe{putchar(81)}
    return (first+again+second-40,next.tag)
}
fn main()->void!i32{
    let result=run(pack(make(65,10),make(66,20),17))?
    if result.1!=17{return 1}
    return result.0
}
"#
    );
    if different_roots {
        source = source.replace("make(66,20)", "other(66,20)");
    }
    source
}

#[test]
fn store_and_success_union_preserve_mode_for_same_and_different_actual_types() {
    for distinct in [false, true] {
        let source = store_source(distinct);
        native(
            &format!("store-identity-pop-distinct-{distinct}"),
            &source,
            "MPBQZA",
            0,
        );
        let borrow_once = source.replace(
            "let first=view.first()",
            "view.second()\n    let first=view.first()",
        );
        reject_source(
            &format!("borrowed-once-distinct-{distinct}"),
            &borrow_once,
            &[Phase::Types],
            Some("E-CLOSURE-MODE"),
        );
        let twice = source.replace(
            "let second=next.second()",
            "let second=next.second()\n    next.second()",
        );
        reject_source(
            &format!("popped-once-repeated-distinct-{distinct}"),
            &twice,
            &[Phase::Expand, Phase::Types, Phase::Analyze, Phase::Verify],
            None,
        );
    }
}

#[test]
fn declaration_order_and_field_order_are_independent_even_when_ids_are_equal() {
    let reversed_fields = ORIGINAL_MARKERS.replace(
        "manages marker: Ticket; first: F; second: G; tag: T",
        "second: G; tag: T; manages marker: Ticket; first: F",
    );
    // G is already consumed before Q. Only Z then A remain at shell cleanup.
    native("same-id-reversed-fields", &reversed_fields, "MPBQZA", 0);
    let reversed_declarations = reversed_fields.replace(
        "fn run<T: Copy, F: Fn() -> i32, G: FnOnce() -> i32>",
        "fn run<T: Copy, G: FnOnce() -> i32, F: Fn() -> i32>",
    );
    assert_ne!(reversed_declarations, reversed_fields);
    native(
        "same-id-reversed-bindings-and-fields",
        &reversed_declarations,
        "MPBQZA",
        0,
    );
}

fn unused_state_core() -> (CoreModule, usize) {
    let core = expanded(
        "raw-state-header-source",
        r#"
resource Ticket{code:i32}
package State<F>{manages marker:Ticket;first:F;second:F}
fn inspect<F:Fn()->i32>(state:State<F>)->i32{return 0}
fn main()->i32{return 0}
"#,
    );
    // Expand retains nominal aggregate references. Resolve the legacy control
    // through its real public stage before introducing the new raw table.
    let mut resolved =
        zil_types::resolve_program_shapes(&program(core), &Limits::default()).unwrap();
    let mut core = resolved.modules.remove(0).core;
    let index = core
        .functions
        .iter()
        .position(|f| !f.callable_parameters.is_empty())
        .unwrap();
    let function = &mut core.functions[index];
    assert_eq!(function.callable_parameters.len(), 1);
    let previous = function.callable_parameters.remove(0);
    assert_eq!(previous.index, 0);
    let Type::Package { fields, .. } = &mut function.parameters[0].ty else {
        panic!("owning symbolic shell")
    };
    assert_eq!(fields.len(), 3);
    assert!(matches!(fields[1].1, Type::Named(_)));
    fields[1].1 = Type::Named("ReviewF".into());
    fields[2].1 = Type::Named("ReviewG".into());
    function.state_parameters = vec![CoreStateParameter {
        index: 0,
        bindings: vec![
            CoreStateBinding {
                generic_index: 0,
                generic_name: "ReviewF".into(),
                kind: CallableKind::Fn,
                parameters: vec![],
                result: Type::I32,
                fields: vec![1],
            },
            CoreStateBinding {
                generic_index: 1,
                generic_name: "ReviewG".into(),
                kind: CallableKind::FnOnce,
                parameters: vec![],
                result: Type::I32,
                fields: vec![2],
            },
        ],
    }];
    (core, index)
}

#[test]
fn unused_raw_state_headers_validate_every_binding_and_real_field_selector() {
    let (core, index) = unused_state_core();
    raw_types(
        "unused-state-header-valid",
        &core,
        &Limits::default(),
        false,
    );
    for attack in 0..15 {
        let mut changed = core.clone();
        let f = &mut changed.functions[index];
        match attack {
            0 => f.state_parameters[0].index = 1,
            1 => f.state_parameters.push(f.state_parameters[0].clone()),
            2 => f.state_parameters[0].bindings.swap(0, 1),
            3 => f.state_parameters[0].bindings[1].generic_index = 0,
            4 => f.state_parameters[0].bindings[1].generic_name = "ReviewF".into(),
            5 => f.state_parameters[0].bindings[0].fields.clear(),
            6 => f.state_parameters[0].bindings[0].fields = vec![0],
            7 => f.state_parameters[0].bindings[1].fields = vec![1],
            8 => f.state_parameters[0].bindings[1].fields = vec![3],
            9 => {
                f.state_parameters[0].bindings.pop();
            }
            10 => f.state_parameters[0].bindings[0].fields = vec![1, 1],
            11 => f.type_parameters.push(CoreTypeParameter {
                index: 0,
                name: "T".into(),
                bound: CoreTypeBound::StoreElement,
            }),
            12 => f.callable_parameters.push(CoreCallableParameter {
                index: 0,
                generic_index: 0,
                generic_name: "ReviewF".into(),
                kind: CallableKind::Fn,
                parameters: vec![],
                result: Type::I32,
            }),
            13 => {
                f.callable_return = Some(CoreCallableReturn {
                    kind: CallableKind::Fn,
                    parameters: vec![],
                    result: Type::I32,
                })
            }
            14 => {
                f.receiver = Some(ReceiverShape {
                    ty: Type::I32,
                    mode: PassingMode::Copy,
                    implicit: true,
                })
            }
            _ => unreachable!(),
        }
        raw_types(
            &format!("unused-state-header-attack-{attack}"),
            &changed,
            &Limits::default(),
            true,
        );
    }
}

const TWO_ENVIRONMENTS: &str = r#"
fn small()->impl Fn()->i32{let n=7;return move()=>n}
fn wide()->impl Fn()->i32{let padding:i64=1;let n=9;return move()=>{padding;return n}}
fn main()->i32{
    let bytes=bytes_new();bytes_len(bytes)
    let first=small();let second=wide()
    return first()+second()-16
}
"#;

fn resource_only_pair() -> (TypedModule, CandidatePlan, [ClosureId; 2]) {
    let (_, mut typed, _) = typed_control("actual-factories-before-raw-carrier", TWO_ENVIRONMENTS);
    let roots = ["small", "wide"].map(|name| {
        let function = typed.functions.iter().find(|f| f.name == name).unwrap();
        let Type::Closure(id) = function.return_type else {
            panic!("concrete factory result")
        };
        id
    });
    assert_ne!(roots[0], roots[1]);
    assert_ne!(
        typed.closures[roots[0].0 as usize].captures,
        typed.closures[roots[1].0 as usize].captures
    );
    let element = Type::Package {
        name: "IndependentTwoRoot".into(),
        fields: vec![
            ("first".into(), Type::Closure(roots[0])),
            ("owner".into(), Type::Managed(Box::new(Type::ByteBuffer))),
            ("second".into(), Type::Closure(roots[1])),
            ("tail".into(), Type::I64),
        ],
    };
    typed.resources.push(ResourceDefinition {
        name: "@builtin:OwnedStore<IndependentTwoRoot>".into(),
        payload: Type::OwnedStore {
            element: Box::new(element),
        },
        teardown: None,
    });
    let case = Case::new("raw-unused-two-root-carrier-control");
    case.write("typed.txt", format!("{typed:#?}"));
    let candidate = zil_analyze::analyze(&typed, &Limits::default());
    case.write("analyze.txt", format!("{candidate:#?}"));
    let candidate = candidate.unwrap();
    verify_pair(&case, &typed, &candidate, None);
    (typed, candidate, roots)
}

fn verify_pair(
    case: &Case,
    module: &TypedModule,
    candidate: &CandidatePlan,
    expected: Option<(ResultCategory, &str)>,
) {
    let bytes =
        zil_ir::wire::encode_module_candidate(module, candidate, &Limits::default()).unwrap();
    case.write("candidate.wire", &bytes);
    assert_eq!(
        zil_ir::wire::decode_module_candidate(&bytes, &Limits::default()).unwrap(),
        (module.clone(), candidate.clone())
    );
    let raw = zil_verify::verify_module(
        module.clone(),
        candidate.clone(),
        &module.target,
        &Limits::default(),
    );
    let wire = zil_verify::verify_module_bytes(&bytes, &module.target, &Limits::default());
    case.write("verify.txt", format!("raw={raw:#?}\nwire={wire:#?}"));
    for result in [raw, wire] {
        if let Some((category, code)) = expected {
            let errors = result.unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|d| d.phase == Phase::Verify && d.category == category && d.code == code),
                "{errors:#?}"
            );
        } else {
            result.unwrap();
        }
    }
}

fn raw_store_reject(
    label: &str,
    module: TypedModule,
    original: &CandidatePlan,
    category: ResultCategory,
    code: &str,
) {
    let case = Case::new(label);
    case.write("typed.txt", format!("{module:#?}"));
    let analyzed = zil_analyze::analyze(&module, &Limits::default());
    case.write("analyze.txt", format!("{analyzed:#?}"));
    // Qualifier mutations can be rejected by Analyze. Still present the raw
    // module to both independent Verify entries with an explicitly recorded
    // fallback plan; do not report this as a freshly analyzed candidate.
    case.write(
        "candidate-origin.txt",
        if analyzed.is_ok() {
            "fresh Analyze candidate\n"
        } else {
            "original control plan with actual attacked module fingerprint; Analyze rejected\n"
        },
    );
    let mut candidate = analyzed.unwrap_or_else(|_| original.clone());
    candidate.module_fingerprint = module.fingerprint();
    verify_pair(&case, &module, &candidate, Some((category, code)));
}

#[test]
fn raw_store_checks_bad_second_environment_and_metadata_without_any_store_operation() {
    let (original, plan, roots) = resource_only_pair();
    for (label, capture, category, code) in [
        (
            "borrowed-address",
            Type::Address {
                mutable: false,
                inner: Box::new(Type::I32),
            },
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "store-capture",
            Type::Managed(Box::new(original.resources.last().unwrap().payload.clone())),
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "nested-callable-package",
            Type::Package {
                name: "NestedCallback".into(),
                fields: vec![
                    ("token".into(), Type::Managed(Box::new(Type::ByteBuffer))),
                    ("callback".into(), Type::Closure(roots[0])),
                ],
            },
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        ),
        (
            "self-cycle",
            Type::Closure(roots[1]),
            ResultCategory::SafeReject,
            "E-VERIFY-CLOSURE-LAYOUT",
        ),
        (
            "missing-child",
            Type::Closure(ClosureId(99)),
            ResultCategory::SafeReject,
            "E-VERIFY-CLOSURE",
        ),
    ] {
        let mut changed = original.clone();
        changed.closures[roots[1].0 as usize].captures = vec![capture];
        raw_store_reject(
            &format!("bad-second-{label}"),
            changed,
            &plan,
            category,
            code,
        );
    }
    let mut bad_identity = original.clone();
    bad_identity.closures[roots[1].0 as usize].id = roots[0];
    raw_store_reject(
        "bad-second-definition-id",
        bad_identity,
        &plan,
        ResultCategory::SafeReject,
        "E-VERIFY-CLOSURE",
    );
    let mut cross_cycle = original.clone();
    cross_cycle.closures[roots[0].0 as usize].captures = vec![Type::Closure(roots[1])];
    cross_cycle.closures[roots[1].0 as usize].captures = vec![Type::Closure(roots[0])];
    raw_store_reject(
        "cycle-between-both-roots",
        cross_cycle,
        &plan,
        ResultCategory::SafeReject,
        "E-VERIFY-CLOSURE-LAYOUT",
    );
    let mut three = original.clone();
    let Type::OwnedStore { element } = &mut three.resources.last_mut().unwrap().payload else {
        unreachable!()
    };
    let Type::Package { fields, .. } = element.as_mut() else {
        unreachable!()
    };
    fields.push(("third".into(), Type::Closure(roots[0])));
    raw_store_reject(
        "three-fields-remain-unsupported",
        three,
        &plan,
        ResultCategory::Unsupported,
        "E-UNSUPPORTED",
    );
}

#[test]
fn raw_store_heterogeneous_result_copy_is_checked_on_both_actual_roots() {
    let (original, plan, roots) = resource_only_pair();
    for root in roots {
        let mut changed = original.clone();
        changed.closures[root.0 as usize].result = Type::Address {
            mutable: false,
            inner: Box::new(Type::I32),
        };
        raw_store_reject(
            &format!("heterogeneous-result-root-{}", root.0),
            changed,
            &plan,
            ResultCategory::Unsupported,
            "E-UNSUPPORTED",
        );
    }
    let mut missing = original.clone();
    let Type::OwnedStore { element } = &mut missing.resources.last_mut().unwrap().payload else {
        unreachable!()
    };
    let Type::Package { fields, .. } = element.as_mut() else {
        unreachable!()
    };
    fields[2].1 = Type::Closure(ClosureId(99));
    raw_store_reject(
        "missing-actual-second-root",
        missing,
        &plan,
        ResultCategory::SafeReject,
        "E-VERIFY-CLOSURE",
    );
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
fn raw_store_two_root_work_bounds_replay_the_same_fresh_result() {
    let (typed, plan, _) = resource_only_pair();
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
    Case::new("raw-two-root-exact-budgets").write("boundaries.json",format!("{{\"analyze\":{analyze},\"verify\":{verify},\"checks\":\"N, N-1 ResourceLimit, fresh N\",\"evidence_level\":\"source-generated closure definitions plus raw unused Store carrier\"}}\n"));
    let case = Case::new("stale-schema26-wire");
    let mut wire =
        zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default()).unwrap();
    assert_eq!(u32::from_le_bytes(wire[8..12].try_into().unwrap()), 27);
    wire[8..12].copy_from_slice(&26u32.to_le_bytes());
    case.write("stale26.wire", &wire);
    let errors =
        zil_verify::verify_module_bytes(&wire, &typed.target, &Limits::default()).unwrap_err();
    case.write("verify.txt", format!("{errors:#?}"));
    assert!(errors.iter().any(|d| d.code == "E-WIRE-VERSION"));
}
