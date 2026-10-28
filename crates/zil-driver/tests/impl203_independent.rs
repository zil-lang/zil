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
        let case = Case::new(&format!("borrowed-once-distinct-{distinct}"));
        let report = compile(&case, &borrow_once, None);
        assert_eq!(
            report.status,
            ResultCategory::SafeReject,
            "{}",
            report.to_json()
        );
        assert!(
            report.diagnostics.iter().any(|d| (d.phase == Phase::Expand
                && d.code == "E-GENERIC-ARGUMENT"
                && d.message == "FnOnce requires an owned Store element obtained by pop")
                || (d.phase == Phase::Types && d.code == "E-CLOSURE-MODE")),
            "{}",
            report.to_json()
        );
        assert_eq!(report.verification_runs, 0);
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
    // The original Core aggregate is still a raw declaration even though the
    // parameter now carries its resolved shape. Keep both representations
    // consistent; leaving its old F would be a different, invalid control.
    let declaration = core
        .aggregates
        .iter_mut()
        .find(|a| a.kind == AggregateKind::Package)
        .unwrap();
    declaration.fields[1].ty = Type::Named("ReviewF".into());
    declaration.fields[2].ty = Type::Named("ReviewG".into());
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

#[test]
fn ordinary_templates_keep_error_payload_and_success_payload_separate() {
    let prefix = &ORIGINAL[..ORIGINAL.find("fn run<").unwrap()];
    let source = format!(
        "{HELPERS}{prefix}{}",
        r#"
fn error_route<T:StoreElement>(value:T!void)->T!void{return value}
fn run<T:Copy,F:Fn()->i32,G:FnOnce()->i32>(state:State<T,F,G>)->State<T,F,G>!i32{
    let items:Store<State<T,F,G>>=empty()
    let full:State<T,F,G>!void=push(items,state)
    let routed:State<T,F,G>!void=error_route::<State<T,F,G>>(full)
    routed?
    return 0
}
fn main()->i32{let result=run(pack(make(65,10),make(66,20),17));return 0}
"#
    );
    // Main discards the actual returned sum; its active error owns State.
    native(
        "error-side-complete-state-through-template",
        &source,
        "ZAB",
        0,
    );
    let wrong_success = source.replace(
        "    routed?",
        "    let success=routed?\n    success.second()",
    );
    reject_source(
        "try-on-state-error-cannot-project-error-as-success",
        &wrong_success,
        &[Phase::Expand, Phase::Types],
        None,
    );
}

fn nominal_source(phantom: bool) -> String {
    let mut source = ORIGINAL_MARKERS[..ORIGINAL_MARKERS.find("fn main()").unwrap()].to_owned();
    source = source
        .replace("-> (i32, T)", "-> i32")
        .replace("    let tag = state.tag\n", "")
        .replace(
            "return (first + again + second - 40, tag)",
            "return first + again + second - 40",
        );
    if phantom {
        source = source
            .replace("; tag: T }", " }")
            .replace(", tag: tag }", " }");
    }
    format!(
        "{source}{}",
        r#"
struct Left{value:i32}
struct Right{value:i32}
fn main()->i32{
    let left=run(pack(make(65,10),make(66,20),Left{value:1}))
    let right=run(pack(make(67,10),make(68,20),Right{value:1}))
    return left+right
}
"#
    )
}

#[test]
fn complete_nominal_and_phantom_t_remain_part_of_dual_state_identity() {
    for phantom in [false, true] {
        let source = nominal_source(phantom);
        native(
            &format!("dual-state-nominal-phantom-{phantom}"),
            &source,
            "MPBQZAMPDQZC",
            0,
        );
        let (_, typed, _) = typed_control(&format!("dual-state-family-phantom-{phantom}"), &source);
        let states = typed
            .functions
            .iter()
            .flat_map(|f| f.parameters.iter().map(|id| &f.locals[id.0 as usize].ty))
            .filter_map(|ty| match ty {
                Type::Package { name, fields }
                    if fields
                        .iter()
                        .filter(|(_, t)| matches!(t, Type::Closure(_)))
                        .count()
                        == 2 =>
                {
                    Some(name.clone())
                }
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            states.len(),
            2,
            "same concrete [C,C] still has two distinct nominal data families: {states:#?}"
        );
    }
}

const QUALIFICATION_ONLY: &str = r#"
extern "C" fn putchar(n:i32)->i32;
resource Empty{}
impl Drop for Empty{fn drop(self:Self){unsafe{putchar(90)}}}
package Duo<F,G>{manages owner:Empty;first:F;second:G}
fn first()->impl Fn()->i32{return ()=>7}
fn second()->impl Fn()->i32{return ()=>9}
fn pack<F:Fn()->i32,G:FnOnce()->i32>(f:F,g:G)->Duo<F,G>{return Duo::<F,G>{owner:Empty{},first:f,second:g}}
fn identity<T:StoreElement>(value:T)->T{return value}
fn run<F:Fn()->i32,G:FnOnce()->i32>(state:Duo<F,G>)->i32{
    let next=identity::<Duo<F,G>>(state)
    return 0
}
fn main()->i32{return run(pack(first(),second()))}
"#;

fn no_store_qualification(source: &str) -> (CoreModule, TypedModule, CandidatePlan) {
    let core = expanded("dual-state-no-store-qualification", source);
    let case = Case::new("dual-state-no-store-qualification-typed");
    let typed = zil_types::check(&core, &TargetProfile::host_llvm(), &Limits::default());
    case.write("typed.txt", format!("{typed:#?}"));
    let typed = typed.unwrap();
    let stores = typed
        .resources
        .iter()
        .filter_map(|r| {
            if let Type::OwnedStore { element } = &r.payload {
                Some(element)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(
        !stores.is_empty(),
        "every actual ordinary T binding must carry canonical qualification even without Store operations"
    );
    assert!(stores.iter().any(|element|matches!(element.as_ref(),Type::Package{fields,..} if fields.iter().filter_map(|(_,ty)|if let Type::Closure(id)=ty{Some(*id)}else{None}).collect::<std::collections::BTreeSet<_>>().len()==2)));
    assert!(
        typed
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions)
            .all(|i| !matches!(
                i.kind,
                InstructionKind::OwnedStoreNew { .. }
                    | InstructionKind::OwnedStoreReserve { .. }
                    | InstructionKind::OwnedStorePush { .. }
                    | InstructionKind::OwnedStorePop { .. }
                    | InstructionKind::OwnedStoreBorrow { .. }
                    | InstructionKind::OwnedStoreBorrowMut { .. }
            ))
    );
    let plan = zil_analyze::analyze(&typed, &Limits::default());
    case.write("analyze.txt", format!("{plan:#?}"));
    (core, typed, plan.unwrap())
}

#[test]
fn identity_without_store_operations_still_checks_actual_two_root_layout() {
    let (_, zero, plan) = no_store_qualification(QUALIFICATION_ONLY);
    let case = Case::new("two-empty-environments-zero-actual-element");
    verify_pair(
        &case,
        &zero,
        &plan,
        Some((ResultCategory::Unsupported, "E-UNSUPPORTED")),
    );
    reject_source(
        "source-two-empty-environments-zero-layout",
        QUALIFICATION_ONLY,
        &[Phase::Verify],
        Some("E-UNSUPPORTED"),
    );
    let nonzero = QUALIFICATION_ONLY.replace(
        "fn first()->impl Fn()->i32{return ()=>7}",
        "fn first()->impl Fn()->i32{let n=7;return move()=>n}",
    );
    native(
        "one-nonempty-environment-complete-element",
        &nonzero,
        "Z",
        0,
    );
    let (core, typed, plan) = no_store_qualification(&nonzero);
    verify_pair(
        &Case::new("qualification-only-nonzero-control"),
        &typed,
        &plan,
        None,
    );
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
    Case::new("qualification-only-types-work").write("boundaries.json",format!("{{\"types\":{types},\"checks\":\"N, N-1 ResourceLimit, fresh N equal typed result\",\"store_operations\":0}}\n"));
}

#[test]
fn unused_raw_state_bodies_cannot_acquire_fields_or_repeat_a_declared_once_call() {
    let (core, index) = unused_state_core();
    raw_types(
        "unused-state-body-control",
        &core,
        &Limits::default(),
        false,
    );
    let mut accepted = Vec::new();
    for attack in ["missing-field", "repeated-once"] {
        let mut changed = core.clone();
        let f = &mut changed.functions[index];
        let span = f.span;
        let field = if attack == "missing-field" {
            "not_declared"
        } else {
            "second"
        };
        let count = if attack == "missing-field" { 1 } else { 2 };
        let end = f.body.statements.last().unwrap().clone();
        f.body.statements.clear();
        for i in 0..count {
            let base = 990000 + i * 10;
            f.body.statements.push(CoreStatement {
                id: NodeId(base),
                span,
                kind: CoreStatementKind::Expression(CoreExpr {
                    id: NodeId(base + 1),
                    span,
                    kind: CoreExprKind::Call {
                        callee: Box::new(CoreExpr {
                            id: NodeId(base + 2),
                            span,
                            kind: CoreExprKind::Member {
                                object: Box::new(CoreExpr {
                                    id: NodeId(base + 3),
                                    span,
                                    kind: CoreExprKind::Name(f.parameters[0].name.clone()),
                                }),
                                member: field.into(),
                            },
                        }),
                        arguments: vec![],
                    },
                }),
            });
        }
        f.body.statements.push(end);
        let case = Case::new(&format!("unused-state-body-{attack}"));
        case.write("core.txt", format!("{changed:#?}"));
        let single = zil_types::check(&changed, &TargetProfile::host_llvm(), &Limits::default());
        let many = zil_types::check_program(
            &program(changed.clone()),
            &TargetProfile::host_llvm(),
            &Limits::default(),
        )
        .map(|p| p.module);
        case.write("single.txt", format!("{single:#?}"));
        case.write("program.txt", format!("{many:#?}"));
        for (entry, result) in [("single", single), ("program", many)] {
            match result {
                Ok(_) => accepted.push(format!("{attack}/{entry}")),
                Err(ds) => assert!(
                    ds.iter().any(|d| d.phase == Phase::Types
                        && matches!(
                            d.category,
                            ResultCategory::SafeReject | ResultCategory::Unsupported
                        )),
                    "{ds:#?}"
                ),
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "unused declared State definition checks were skipped: {accepted:?}"
    );
}

fn raw_state_field_take(
    function: &CoreFunction,
    local: &str,
    field: &str,
    base: u32,
) -> CoreStatement {
    let span = function.span;
    CoreStatement {
        id: NodeId(base),
        span,
        kind: CoreStatementKind::Binding {
            name: local.into(),
            mutable: false,
            ty: None,
            initializer: Some(CoreExpr {
                id: NodeId(base + 1),
                span,
                kind: CoreExprKind::Member {
                    object: Box::new(CoreExpr {
                        id: NodeId(base + 2),
                        span,
                        kind: CoreExprKind::Name(function.parameters[0].name.clone()),
                    }),
                    member: field.into(),
                },
            }),
        },
    }
}

#[test]
fn unused_raw_state_field_moves_do_not_gain_copy_from_callable_bounds() {
    let (core, index) = unused_state_core();
    let mut accepted = Vec::new();
    for kind in [CallableKind::Fn, CallableKind::FnMut, CallableKind::FnOnce] {
        let mut control = core.clone();
        let f = &mut control.functions[index];
        f.state_parameters[0].bindings[0].kind = kind;
        let end = f.body.statements.last().unwrap().clone();
        f.body.statements = vec![
            raw_state_field_take(f, "one", "first", 991000),
            raw_state_field_take(f, "two", "second", 991010),
            end.clone(),
        ];
        raw_types(
            &format!("unused-take-distinct-{kind:?}"),
            &control,
            &Limits::default(),
            false,
        );
        for field in ["first", "second"] {
            let mut changed = control.clone();
            let f = &mut changed.functions[index];
            f.body.statements = vec![
                raw_state_field_take(f, "one", field, 991000),
                raw_state_field_take(f, "two", field, 991010),
                end.clone(),
            ];
            let label = format!("unused-take-twice-{kind:?}-{field}");
            if inspect_origin_attack(&label, &changed) {
                accepted.push(label);
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "an uninstantiated declaration cannot copy a callback merely because it has a callable bound: {accepted:?}"
    );
}

#[test]
fn unused_raw_state_value_helper_transfer_cannot_leave_the_source_usable() {
    let (core, index) = unused_state_core();
    let mut accepted = Vec::new();
    for (field, kind) in [
        ("first", CallableKind::Fn),
        ("second", CallableKind::FnOnce),
    ] {
        let mut control = core.clone();
        let f = &control.functions[index];
        let span = f.span;
        let end = f.body.statements.last().unwrap().clone();
        let mut helper = f.clone();
        helper.id = NodeId(992000);
        helper.name = "review_value_helper".into();
        helper.state_parameters.clear();
        helper.callable_parameters = vec![CoreCallableParameter {
            index: 0,
            generic_index: 0,
            generic_name: "ReviewH".into(),
            kind,
            parameters: vec![],
            result: Type::I32,
        }];
        helper.parameters = vec![CoreParameter {
            name: "value".into(),
            ty: Type::Named("ReviewH".into()),
            mode: PassingMode::Infer,
            span,
        }];
        helper.body.statements = vec![CoreStatement {
            id: NodeId(992001),
            span,
            kind: CoreStatementKind::Return(Some(CoreExpr {
                id: NodeId(992002),
                span,
                kind: CoreExprKind::Call {
                    callee: Box::new(CoreExpr {
                        id: NodeId(992003),
                        span,
                        kind: CoreExprKind::Name("value".into()),
                    }),
                    arguments: vec![],
                },
            })),
        }];
        let CoreStatementKind::Binding {
            initializer: Some(argument),
            ..
        } = raw_state_field_take(f, "unused", field, 992010).kind
        else {
            unreachable!()
        };
        let call = CoreStatement {
            id: NodeId(992020),
            span,
            kind: CoreStatementKind::Expression(CoreExpr {
                id: NodeId(992021),
                span,
                kind: CoreExprKind::Call {
                    callee: Box::new(CoreExpr {
                        id: NodeId(992022),
                        span,
                        kind: CoreExprKind::Name(helper.name.clone()),
                    }),
                    arguments: vec![argument],
                },
            }),
        };
        control.functions[index].body.statements = vec![call.clone(), end.clone()];
        control.functions.push(helper);
        raw_types(
            &format!("unused-value-helper-once-{kind:?}"),
            &control,
            &Limits::default(),
            false,
        );
        let f = &mut control.functions[index];
        f.body.statements = vec![call, raw_state_field_take(f, "later", field, 992030), end];
        let label = format!("unused-value-helper-reuse-{kind:?}");
        if inspect_origin_attack(&label, &control) {
            accepted.push(label);
        }
    }
    assert!(
        accepted.is_empty(),
        "an owned helper argument consumes the value even when the declared callable mode is reusable: {accepted:?}"
    );
}

#[test]
fn unused_raw_field_borrow_alias_keeps_its_original_parent_position() {
    let (mut core, index) = unused_state_core();
    let f = &mut core.functions[index];
    let span = f.span;
    let end = f.body.statements.last().unwrap().clone();
    let mut borrowed = raw_state_field_take(f, "view", "first", 993000);
    let CoreStatementKind::Binding { initializer, .. } = &mut borrowed.kind else {
        unreachable!()
    };
    *initializer = Some(CoreExpr {
        id: NodeId(993003),
        span,
        kind: CoreExprKind::Unary {
            op: UnaryOp::Borrow,
            operand: Box::new(initializer.take().unwrap()),
        },
    });
    let take = raw_state_field_take(f, "taken", "first", 993010);
    let call = CoreStatement {
        id: NodeId(993020),
        span,
        kind: CoreStatementKind::Expression(CoreExpr {
            id: NodeId(993021),
            span,
            kind: CoreExprKind::Call {
                callee: Box::new(CoreExpr {
                    id: NodeId(993022),
                    span,
                    kind: CoreExprKind::Name("view".into()),
                }),
                arguments: vec![],
            },
        }),
    };
    f.body.statements = vec![borrowed.clone(), call.clone(), take.clone(), end.clone()];
    raw_types(
        "unused-borrow-last-use-before-take",
        &core,
        &Limits::default(),
        false,
    );
    core.functions[index].body.statements = vec![borrowed, take, call, end];
    let accepted = inspect_origin_attack("unused-borrow-used-after-parent-field-take", &core);

    let prefix = &ORIGINAL[..ORIGINAL.find("fn run<").unwrap()];
    let positive = format!(
        "{prefix}\nfn run<T:Copy,F:Fn()->i32,G:FnOnce()->i32>(state:State<T,F,G>)->i32{{\nlet view=&state.first\nlet first=view()\nlet taken=state.first\nreturn first\n}}\nfn main()->i32{{return run(pack(make(65,10),make(66,20),17))-10}}\n"
    );
    native("used-borrow-last-use-before-take", &positive, "AZB", 0);
    let negative = positive.replace(
        "let first=view()\nlet taken=state.first",
        "let taken=state.first\nlet first=view()",
    );
    assert_ne!(positive, negative);
    reject_source(
        "used-borrow-used-after-parent-field-take",
        &negative,
        &[Phase::Expand, Phase::Types, Phase::Analyze, Phase::Verify],
        None,
    );
    assert!(
        !accepted,
        "an unused declaration cannot use a field alias after its parent value is moved"
    );
}

#[test]
fn unused_state_second_binder_metadata_has_a_real_early_budget() {
    let (core, index) = unused_state_core();
    raw_types(
        "unused-state-budget-positive",
        &core,
        &Limits::default(),
        false,
    );
    for oversized in [false, true] {
        let mut changed = core.clone();
        let limits = if oversized {
            let long = "Second".repeat(4096);
            let f = &mut changed.functions[index];
            f.state_parameters[0].bindings[1].generic_name = long.clone();
            let Type::Package { fields, .. } = &mut f.parameters[0].ty else {
                unreachable!()
            };
            fields[2].1 = Type::Named(long.clone());
            changed
                .aggregates
                .iter_mut()
                .find(|a| a.kind == AggregateKind::Package)
                .unwrap()
                .fields[2]
                .ty = Type::Named(long);
            Limits {
                max_analysis_steps: 512,
                ..Limits::default()
            }
        } else {
            let mut ty = Type::I32;
            for _ in 0..12 {
                ty = Type::Tuple(vec![ty]);
            }
            changed.functions[index].state_parameters[0].bindings[1].result = ty;
            Limits {
                max_call_depth: 4,
                ..Limits::default()
            }
        };
        let case = Case::new(&format!("unused-second-binder-budget-long-{oversized}"));
        case.write("core.txt", format!("{changed:#?}"));
        case.write("limits.txt", format!("{limits:#?}"));
        let single = zil_types::check(&changed, &TargetProfile::host_llvm(), &limits);
        let many = zil_types::check_program(
            &program(changed.clone()),
            &TargetProfile::host_llvm(),
            &limits,
        );
        case.write("single.txt", format!("{single:#?}"));
        case.write("program.txt", format!("{many:#?}"));
        for errors in [single.unwrap_err(), many.unwrap_err()] {
            assert!(errors.iter().any(|d|d.phase==Phase::Types && d.category==ResultCategory::ResourceLimit),"{errors:#?}");
        }
    }
}

#[test]
fn cross_module_equal_named_data_families_keep_their_full_state_instance() {
    let state = ORIGINAL_MARKERS[..ORIGINAL_MARKERS.find("fn main()").unwrap()]
        .replace("package State", "pub package State")
        .replace("fn make(", "pub fn make(")
        .replace("fn pack<", "pub fn pack<")
        .replace("fn run<", "pub fn run<");
    let main = r#"
import state
import left
import right
fn main()->i32{
    let a=state.run(state.pack(state.make(65,10),state.make(66,20),left.Tag{value:17}))
    let b=state.run(state.pack(state.make(67,10),state.make(68,20),right.Tag{value:23}))
    if a.1.value!=17 || b.1.value!=23{return 1}
    return a.0+b.0
}
"#;
    let files = [
        ("main", main),
        ("state", state.as_str()),
        ("left", "pub struct Tag{pub value:i32}"),
        ("right", "pub struct Tag{pub value:i32}"),
    ];
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("module-nominal-State-family");
        for (name, source) in files {
            case.write(&format!("{name}.zil"), source);
        }
        let project = ProjectSources {
            root: ModuleId(0),
            modules: files
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
            paths: files
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
                .collect(),
        };
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
        case.write("compile.json", report.to_json());
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
        case.write("native.txt",format!("optimization={optimization:?}\nexit={:?}\nexpected_exit=0\nexpected_stdout=MPBQZAMPDQZC\n",output.status.code()));
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"MPBQZAMPDQZC");
        assert!(output.stderr.is_empty());
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

fn inspect_origin_attack(label: &str, core: &CoreModule) -> bool {
    let case = Case::new(label);
    case.write("core.txt", format!("{core:#?}"));
    let single = zil_types::check(core, &TargetProfile::host_llvm(), &Limits::default());
    let many = zil_types::check_program(
        &program(core.clone()),
        &TargetProfile::host_llvm(),
        &Limits::default(),
    )
    .map(|p| p.module);
    case.write("single.txt", format!("{single:#?}"));
    case.write("program.txt", format!("{many:#?}"));
    let mut accepted = false;
    for (entry, result) in [("single", single), ("program", many)] {
        match result {
            Ok(typed) => {
                accepted = true;
                let analysis = zil_analyze::analyze(&typed, &Limits::default());
                case.write(&format!("{entry}-analyze.txt"), format!("{analysis:#?}"));
                if let Ok(plan) = analysis {
                    let verification = zil_verify::verify_module(
                        typed.clone(),
                        plan.clone(),
                        &typed.target,
                        &Limits::default(),
                    );
                    let bytes =
                        zil_ir::wire::encode_module_candidate(&typed, &plan, &Limits::default())
                            .unwrap();
                    let decoded =
                        zil_verify::verify_module_bytes(&bytes, &typed.target, &Limits::default());
                    case.write(&format!("{entry}.wire"), bytes);
                    case.write(&format!("{entry}-verify.txt"),format!("raw={verification:#?}\nwire={decoded:#?}\nThese concrete-memory results cannot recover an erased source declaration.\n"));
                }
            }
            Err(errors) => assert!(
                errors.iter().any(|d| d.phase == Phase::Types
                    && matches!(
                        d.category,
                        ResultCategory::SafeReject | ResultCategory::Unsupported
                    )),
                "{errors:#?}"
            ),
        }
    }
    accepted
}

#[test]
fn raw_reused_node_identity_cannot_hide_conflicting_field_origins() {
    let mut core = expanded("node-origin-control", ORIGINAL);
    raw_types("node-origin-valid", &core, &Limits::default(), false);
    let f = core
        .functions
        .iter_mut()
        .find(|f| !f.state_parameters.is_empty())
        .unwrap();
    let mut first = None;
    visit_block(&mut f.body, &mut |e| {
        if matches!(&e.kind,CoreExprKind::Member{member,..} if member=="first") {
            first = Some(e.id);
        }
        false
    });
    let first = first.unwrap();
    let mut changed = 0;
    visit_block(&mut f.body, &mut |e| {
        if matches!(&e.kind,CoreExprKind::Member{member,..} if member=="second") {
            e.id = first;
            changed += 1;
        }
        false
    });
    assert_eq!(changed, 1);
    assert!(
        !inspect_origin_attack("conflicting-field-node-id", &core),
        "a raw node cannot identify both F and G declaration origins"
    );
}

#[test]
fn raw_complete_type_application_cannot_relabel_equal_concrete_callback_slots() {
    let moved = ORIGINAL_MARKERS
        .replace(
            "    let tag = state.tag",
            "    let next = identity::<State<T,F,G>>(state)\n    let tag = next.tag",
        )
        .replace("state.first()", "next.first()")
        .replace("state.second()", "next.second()");
    let source = format!("{HELPERS}{moved}");
    let core = expanded("typeapply-origin-control", &source);
    raw_types("typeapply-origin-valid", &core, &Limits::default(), false);
    let mut resolved =
        zil_types::resolve_program_shapes(&program(core), &Limits::default()).unwrap();
    let mut changed = resolved.modules.remove(0).core;
    let function = changed
        .functions
        .iter_mut()
        .find(|f| !f.state_parameters.is_empty())
        .unwrap();
    let mut application = 0;
    visit_block(&mut function.body, &mut |e| {
        if let CoreExprKind::TypeApply { type_arguments, .. } = &mut e.kind {
            let [Type::Package { fields, .. }] = type_arguments.as_mut_slice() else {
                panic!("complete symbolic State type application")
            };
            let first = fields[1].1.clone();
            let second = fields[2].1.clone();
            assert_ne!(first, second);
            fields[1].1 = second;
            fields[2].1 = first;
            application += 1;
        }
        if let CoreExprKind::Member { object, member } = &mut e.kind
            && matches!(&object.kind,CoreExprKind::Name(name) if name=="next")
        {
            match member.as_str() {
                "first" => *member = "second".into(),
                "second" => *member = "first".into(),
                _ => {}
            }
        }
        false
    });
    assert_eq!(application, 1);
    assert!(
        !inspect_origin_attack("typeapply-relabels-actual-C-C-weak-G", &changed),
        "explicit full T metadata must not relabel a G:FnOnce value as F:Fn when actual ids coincide"
    );
}

#[test]
fn raw_record_values_cannot_relabel_equal_concrete_callback_slots() {
    let source=ORIGINAL_MARKERS.replace("    let tag = state.tag","    let next = State::<T,F,G>{marker:Ticket{code:78},first:state.first,second:state.second,tag:state.tag}\n    let tag = next.tag")
        .replace("state.first()","next.first()").replace("state.second()","next.second()");
    native("record-origin-positive", &source, "MPBQNAZ", 0);
    let mut core = expanded("record-origin-control", &source);
    raw_types("record-origin-valid", &core, &Limits::default(), false);
    let function = core
        .functions
        .iter_mut()
        .find(|f| !f.state_parameters.is_empty())
        .unwrap();
    let mut swapped = 0;
    visit_block(&mut function.body, &mut |e| {
        // Expand stages record fields in a Sequence before the final Record.
        // Mutate the actual reads while retaining all NodeIds and evaluation
        // order; these are run's only reads of state.first/state.second.
        if let CoreExprKind::Member { object, member } = &mut e.kind
            && matches!(&object.kind,CoreExprKind::Name(name) if name=="state")
        {
            match member.as_str() {
                "first" => {
                    *member = "second".into();
                    swapped += 1;
                }
                "second" => {
                    *member = "first".into();
                    swapped += 1;
                }
                _ => {}
            }
        }
        false
    });
    assert_eq!(swapped, 2);
    assert!(
        !inspect_origin_attack("record-relabels-actual-C-C-weak-G", &core),
        "a raw record type cannot erase the weaker declaration of its actual field value"
    );
}

#[test]
fn declared_callee_mode_is_prepared_before_fallible_argument_cleanup() {
    let prefix = ORIGINAL[..ORIGINAL.find("fn run<").unwrap()]
        .replace("Fn() -> i32", "Fn(i32) -> i32")
        .replace("FnOnce() -> i32", "FnOnce(i32) -> i32")
        .replace("move () =>", "move (delta:i32) =>")
        .replace("return value }", "return value+delta }");
    for once in [false, true] {
        for fail in [false, true] {
            let field = if once { "second" } else { "first" };
            let expected = if once { "PBZA" } else { "PZAB" };
            let answer = if once { 21 } else { 11 };
            let source = format!(
                "{prefix}\nfn argument(fail:bool)->void!i32{{unsafe{{putchar(80)}}if fail{{return ()}}return 1}}\nfn run<T:Copy,F:Fn(i32)->i32,G:FnOnce(i32)->i32>(state:State<T,F,G>)->void!i32{{return state.{field}(argument({fail})?)-{answer}}}\nfn main()->void!i32{{return run(pack(make(65,10),make(66,20),17))}}\n"
            );
            native(
                &format!("callee-once-{once}-argument-fail-{fail}"),
                &source,
                expected,
                i32::from(fail),
            );
        }
    }
}

fn value_inferred_identity_source(distinct: bool) -> String {
    let mut prefix = ORIGINAL_MARKERS[..ORIGINAL_MARKERS.find("fn main()").unwrap()].replace(
        "    let ticket = Ticket { code: label }",
        "    unsafe { putchar(label + 32) }\n    let ticket = Ticket { code: label }",
    );
    let second = if distinct {
        let start = prefix.find("fn make(").unwrap();
        let end = prefix.find("fn pack<").unwrap();
        let other = prefix[start..end]
            .replace("fn make(", "fn other(")
            .replace(
                "    let ticket = Ticket",
                "    let padding:i64=1\n    let ticket = Ticket",
            )
            .replace("ticket.code; return", "ticket.code; padding; return");
        prefix.push_str(&other);
        "other"
    } else {
        "make"
    };
    format!(
        "{prefix}\nfn identity<T:StoreElement>(value:T)->T{{return value}}\nfn main()->i32{{\n    let prepared=identity(pack(make(65,10),{second}(66,20),17))\n    let first=prepared.second()\n    let again=prepared.second()\n    if first+again!=40{{return 2}}\n    let result=run(prepared)\n    if result.1!=17{{return 1}}\n    return result.0\n}}\n"
    )
}

#[test]
fn value_inferred_ordinary_identity_prepares_once_without_global_bound_taint() {
    for distinct in [false, true] {
        // Main has no F/G declaration: its actual Fn values remain callable.
        // Entering run restores that function's distinct Fn/FnOnce sources.
        // Lowercase bytes also catch duplicate argument preparation at inference.
        native(
            &format!("value-inferred-identity-distinct-{distinct}"),
            &value_inferred_identity_source(distinct),
            "abMPBQZA",
            0,
        );
    }
}

fn value_inferred_choice_source(phantom: bool, second_type: &str) -> String {
    let nominal = nominal_source(phantom);
    let prefix = &nominal[..nominal.find("fn main()").unwrap()];
    format!(
        "{prefix}\nfn choose<T:StoreElement>(first:T,second:T)->T{{return first}}\nfn main()->i32{{\n    let chosen=choose(pack(make(65,10),make(66,20),Left{{value:1}}),pack(make(67,10),make(68,20),{second_type}{{value:1}}))\n    return run(chosen)\n}}\n"
    )
}

#[test]
fn value_inferred_ordinary_calls_unify_every_argument_and_keep_nominal_identity() {
    for phantom in [false, true] {
        let source = value_inferred_choice_source(phantom, "Left");
        // The unselected owning argument is cleaned before the returned value.
        native(
            &format!("value-inferred-choice-phantom-{phantom}"),
            &source,
            "ZCDMPBQZA",
            0,
        );
        reject_source(
            &format!("value-inferred-conflicting-nominal-phantom-{phantom}"),
            &value_inferred_choice_source(phantom, "Right"),
            &[Phase::Expand, Phase::Types],
            None,
        );
    }
    let control = value_inferred_identity_source(false);
    for (label, source) in [
        (
            "value-inferred-explicit-constraint-conflicts",
            control.replace("identity(pack(", "identity::<i32>(pack("),
        ),
        (
            "value-inferred-template-cannot-be-value",
            control.replace(
                "    let prepared=identity(pack(",
                "    let function=identity\n    let prepared=function(pack(",
            ),
        ),
        (
            "value-inferred-no-argument-no-type-constraint",
            format!(
                "fn empty<T:StoreElement>()->Store<T> contract{{result(create)}}{{return store_new()}}\n{}",
                control.replace(
                    "    let prepared=",
                    "    let unconstrained=empty()\n    let prepared="
                )
            ),
        ),
    ] {
        assert_ne!(source, control);
        reject_source(&label, &source, &[Phase::Expand, Phase::Types], None);
    }
}

#[test]
fn value_inferred_complete_argument_work_has_an_exact_new_path_boundary() {
    let source = value_inferred_choice_source(true, "Left");
    let (core, typed, _) = typed_control("value-inferred-budget-control", &source);
    let unit = SourceUnit::new(FileId(0), ModuleId(0), "independent203.zil", &source);
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
        .map(|actual| assert_eq!(actual.core, core))
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
        .map(|actual| assert_eq!(actual, typed))
    });
    Case::new("value-inferred-full-argument-exact-budgets").write(
        "boundaries.json",
        format!("{{\"expand\":{expand},\"types\":{types},\"checks\":\"N, N-1 ResourceLimit, fresh N equal result\",\"scope\":\"new bare complete value inference, not an old profile budget claim\"}}\n"),
    );
}

#[test]
fn imported_runtime_templates_keep_scalar_ct_arguments_in_i64_context() {
    let state = ORIGINAL_MARKERS[..ORIGINAL_MARKERS.find("fn main()").unwrap()]
        .replace("package State", "pub package State")
        .replace("fn make(", "pub fn make(")
        .replace("fn pack<", "pub fn pack<")
        .replace("fn run<", "pub fn run<")
        .replace("state: State<T,F,G>)", "state: State<T,F,G>, count:i64)")
        .replace(
            "    let tag = state.tag",
            "    let expected:i64=4294967296\n    if count != expected { return (9,state.tag) }\n    let tag = state.tag",
        );
    let files = [
        (
            "main",
            "import state\nimport ordinary\nfn main()->i32{\nlet next=ordinary.identity(state.pack(state.make(65,10),state.make(66,20),17),#(4294967296))\nlet result=state.run(next,#(4294967296))\nif result.1!=17{return 1}\nreturn result.0\n}",
        ),
        ("state", state.as_str()),
        (
            "ordinary",
            "pub fn identity<T:StoreElement>(value:T,count:i64)->T{count;return value}",
        ),
    ];
    for optimization in [Optimization::O0, Optimization::O2] {
        let case = Case::new("imported-template-runtime-i64-ct-argument");
        for (name, source) in files {
            case.write(&format!("{name}.zil"), source);
        }
        let project = ProjectSources {
            root: ModuleId(0),
            modules: files
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
            paths: files
                .iter()
                .enumerate()
                .skip(1)
                .map(|(i, (name, _))| (vec![name.to_string()], ModuleId(i as u32)))
                .collect(),
        };
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
        case.write("compile.json", report.to_json());
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
        case.write("native.txt",format!("optimization={optimization:?}\nexit={:?}\nexpected_exit=0\nexpected_stdout=MPBQZA\n",output.status.code()));
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"MPBQZA");
        assert!(output.stderr.is_empty());
    }
}
